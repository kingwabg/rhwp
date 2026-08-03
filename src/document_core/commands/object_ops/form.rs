//! 양식 개체 삽입 — 명령 단추·선택 상자·콤보 상자·라디오 단추·입력 상자 (#양식개체).
//!
//! 모델·파서·직렬화·렌더·값편집은 이미 있었고 **문서에 새로 넣는 길만 없었다**.
//! 삽입 골격은 `equation.rs::insert_equation_native` 를 그대로 따른다(같은 확장 컨트롤:
//! 문자코드 0x0B, 본문 8 WCHAR 차지, 인라인 배치).
//!
//! 기본값은 지어내지 않고 **한컴 정답지에서 읽었다**(samples/form-01.hwp, 2026-08-03 실측):
//! 종류별 크기·캡션·배경색·BorderType 이 전부 그 문서의 새 개체 값이다.

use crate::document_core::DocumentCore;
use crate::model::event::DocumentEvent;
use crate::error::HwpError;
use crate::model::control::{Control, FormObject, FormType};

/// 한컴 새 양식 개체 정답지 값: (크기 w×h HWPUNIT, 캡션, 배경색 0x00BBGGRR, BorderType)
fn hancom_defaults(form_type: FormType) -> (u32, u32, &'static str, u32, i32) {
    match form_type {
        FormType::PushButton => (7087, 1984, "명령 단추", 0x00F0F0F0, 4),
        FormType::CheckBox => (9921, 1984, "선택 상자", 0x00FFFFFF, 0),
        FormType::ComboBox => (9921, 1984, "", 0x00F0F0F0, 5),
        FormType::RadioButton => (8504, 1984, "라디오 단추", 0x00FFFFFF, 0),
        FormType::Edit => (7087, 1984, "", 0x00F0F0F0, 5),
    }
}

impl DocumentCore {
    /// 양식 개체를 커서 위치에 삽입한다.
    ///
    /// `props_json` 예: `{"formType":"CheckBox","name":"동의","caption":"동의합니다"}`
    /// - `formType`(필수): PushButton | CheckBox | ComboBox | RadioButton | Edit
    /// - `name`·`caption`·`text`·`width`·`height`·`groupName` 은 선택 — 빠지면 한컴 기본값.
    ///
    /// ponytail: 콤보 상자의 항목(items) 편집은 v1 범위 밖 — 항목은 extra_streams(스크립트)에
    ///   살아서 삽입만으로는 못 채운다. 속성 편집 대화상자가 생길 때 같이 간다.
    pub fn insert_form_object_native(
        &mut self,
        section_idx: usize,
        para_idx: usize,
        char_offset: usize,
        props_json: &str,
    ) -> Result<String, HwpError> {
        use crate::document_core::helpers::{json_i32, json_str};

        if section_idx >= self.document.sections.len() {
            return Err(HwpError::RenderError(format!(
                "구역 인덱스 {} 범위 초과",
                section_idx
            )));
        }
        if para_idx >= self.document.sections[section_idx].paragraphs.len() {
            return Err(HwpError::RenderError(format!(
                "문단 인덱스 {} 범위 초과",
                para_idx
            )));
        }

        let form_type = match json_str(props_json, "formType").as_deref() {
            Some("PushButton") => FormType::PushButton,
            Some("CheckBox") => FormType::CheckBox,
            Some("ComboBox") => FormType::ComboBox,
            Some("RadioButton") => FormType::RadioButton,
            Some("Edit") => FormType::Edit,
            other => {
                return Err(HwpError::InvalidField(format!(
                    "formType 이 필요합니다(PushButton|CheckBox|ComboBox|RadioButton|Edit): {:?}",
                    other
                )))
            }
        };

        let (def_w, def_h, def_caption, back_color, border_type) = hancom_defaults(form_type);
        let name = json_str(props_json, "name")
            .unwrap_or_else(|| format!("{:?}", form_type));
        let caption = json_str(props_json, "caption").unwrap_or_else(|| def_caption.to_string());
        let text = json_str(props_json, "text").unwrap_or_default();
        let width = json_i32(props_json, "width").map(|v| v as u32).unwrap_or(def_w);
        let height = json_i32(props_json, "height").map(|v| v as u32).unwrap_or(def_h);

        let mut form = FormObject {
            form_type,
            name,
            caption,
            text,
            width,
            height,
            fore_color: 0,
            back_color,
            value: 0,
            enabled: true,
            ..Default::default()
        };
        form.properties
            .insert("BorderType".into(), border_type.to_string());
        // 라디오는 그룹이 있어야 상호 배타가 돈다(클릭 처리에서 GroupName 을 본다).
        if let Some(group) = json_str(props_json, "groupName") {
            form.properties.insert("GroupName".into(), group);
        }

        self.document.sections[section_idx].raw_stream = None;
        let paragraph = &mut self.document.sections[section_idx].paragraphs[para_idx];

        // 삽입 위치: 앞선 컨트롤들의 본문 위치와 견줘 char_offset 자리에 끼운다(수식과 동일).
        let insert_idx = {
            let positions = crate::document_core::helpers::find_control_text_positions(paragraph);
            let mut idx = paragraph.controls.len();
            for (i, &pos) in positions.iter().enumerate() {
                if pos > char_offset {
                    idx = i;
                    break;
                }
            }
            idx
        };

        paragraph
            .controls
            .insert(insert_idx, Control::Form(Box::new(form)));
        paragraph.ctrl_data_records.insert(insert_idx, None);

        // 확장 컨트롤 = 본문 8 WCHAR (수식·표와 같은 규약)
        if !paragraph.char_offsets.is_empty() {
            let text_len = paragraph.text.chars().count();
            let safe_offset = char_offset.min(text_len);
            for co in paragraph.char_offsets[safe_offset..].iter_mut() {
                *co += 8;
            }
        }
        paragraph.char_count += 8;
        paragraph.control_mask |= 1u32 << 11;
        paragraph.has_para_text = true;

        // 본문 문단 리플로우(수식 삽입과 같은 경로)
        {
            use crate::renderer::composer::reflow_line_segs;
            use crate::renderer::hwpunit_to_px;
            let page_def = &self.document.sections[section_idx].section_def.page_def;
            let text_width =
                page_def.width as i32 - page_def.margin_left as i32 - page_def.margin_right as i32;
            let available_width = hwpunit_to_px(text_width, self.dpi);
            let para_style = self.styles.para_styles.get(
                self.document.sections[section_idx].paragraphs[para_idx].para_shape_id as usize,
            );
            let margin_left = para_style.map(|s| s.margin_left).unwrap_or(0.0);
            let margin_right = para_style.map(|s| s.margin_right).unwrap_or(0.0);
            let final_width = (available_width - margin_left - margin_right).max(0.0);
            let body_para = &mut self.document.sections[section_idx].paragraphs[para_idx];
            reflow_line_segs(body_para, final_width, &self.styles, self.dpi);
        }

        self.recompose_section(section_idx);
        self.paginate_if_needed();
        self.invalidate_page_tree_cache();

        self.event_log.push(DocumentEvent::PictureInserted {
            section: section_idx,
            para: para_idx,
        });
        Ok(format!(
            "{{\"ok\":true,\"paraIdx\":{},\"controlIdx\":{}}}",
            para_idx, insert_idx
        ))
    }

    /// 양식 개체를 지운다(삽입의 역연산 — 컨트롤 제거 + 본문 8 WCHAR 반환).
    pub fn delete_form_object_native(
        &mut self,
        section_idx: usize,
        para_idx: usize,
        control_idx: usize,
    ) -> Result<String, HwpError> {
        let section = self
            .document
            .sections
            .get_mut(section_idx)
            .ok_or_else(|| HwpError::RenderError(format!("구역 {} 범위 초과", section_idx)))?;
        let paragraph = section
            .paragraphs
            .get_mut(para_idx)
            .ok_or_else(|| HwpError::RenderError(format!("문단 {} 범위 초과", para_idx)))?;
        let ctrl = paragraph
            .controls
            .get(control_idx)
            .ok_or_else(|| HwpError::RenderError(format!("컨트롤 {} 범위 초과", control_idx)))?;
        if !matches!(ctrl, Control::Form(_)) {
            return Err(HwpError::InvalidField("양식 개체가 아닙니다".into()));
        }

        // 이 컨트롤의 본문 위치 — 뒤 글자들의 offset 을 되돌릴 기준.
        let positions = crate::document_core::helpers::find_control_text_positions(paragraph);
        let ctrl_pos = positions.get(control_idx).copied().unwrap_or(0);

        paragraph.controls.remove(control_idx);
        if control_idx < paragraph.ctrl_data_records.len() {
            paragraph.ctrl_data_records.remove(control_idx);
        }
        // 컨트롤 뒤 글자들만 8 을 되돌린다(char_offsets[i] = i번째 글자의 스트림 offset).
        let text_len = paragraph.text.chars().count();
        let safe = ctrl_pos.min(text_len);
        for co in paragraph.char_offsets[safe..].iter_mut() {
            *co = co.saturating_sub(8);
        }
        paragraph.char_count = paragraph.char_count.saturating_sub(8);

        section.raw_stream = None;
        self.recompose_section(section_idx);
        self.paginate_if_needed();
        self.invalidate_page_tree_cache();
        Ok("{\"ok\":true}".to_string())
    }
}
