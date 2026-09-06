//! 양식 개체 삽입 — 명령 단추·선택 상자·콤보 상자·라디오 단추·입력 상자 (#양식개체).
//!
//! 모델·파서·직렬화·렌더·값편집은 이미 있었고 **문서에 새로 넣는 길만 없었다**.
//! 삽입 골격은 `equation.rs::insert_equation_native` 를 그대로 따른다(같은 확장 컨트롤:
//! 문자코드 0x0B, 본문 8 WCHAR 차지, 인라인 배치).
//!
//! 기본값은 지어내지 않고 **한컴 정답지에서 읽었다**(samples/form-01.hwp, 2026-08-03 실측):
//! 종류별 크기·캡션·배경색·BorderType 이 전부 그 문서의 새 개체 값이다.

use crate::document_core::DocumentCore;
use crate::error::HwpError;
use crate::model::control::{Control, FormObject, FormType};
use crate::model::event::DocumentEvent;

/// 컨트롤 8-블록이 스트림 위치 `at` 에 끼어들 때(delta=+8) / 빠질 때(delta=-8),
/// 스트림 위치를 참조하는 **모든 장부**를 함께 민다.
///
/// char_offsets 만 밀고 char_shapes 를 안 밀면 개체 뒤 글자들이 남의 서식(크기·굵기)을
/// 뒤집어쓴다 — 2026-08-03 실측: 15pt 경계(스트림 19) 앞(텍스트 2)에 개체를 넣자
/// 뒤 글자가 10pt→15pt 로 둔갑("개체를 넣으면 텍스트가 작아진다" 신고의 본체).
/// 규칙은 insert_text_at 과 같다: 경계가 `at` 뒤면 이동, 정확히 `at` 이면 0이 아닐 때만.
fn shift_stream_refs(para: &mut crate::model::paragraph::Paragraph, at: u32, delta: i32) {
    let apply = |v: &mut u32, ge: bool| {
        let hit = if ge { *v >= at } else { *v > at };
        if hit {
            *v = (*v as i64 + delta as i64).max(0) as u32;
        }
    };
    for cs in &mut para.char_shapes {
        if cs.start_pos > at || (cs.start_pos == at && cs.start_pos > 0) {
            cs.start_pos = (cs.start_pos as i64 + delta as i64).max(0) as u32;
        }
    }
    for tm in &mut para.track_marks {
        apply(&mut tm.start_pos, true);
        apply(&mut tm.end_pos, false);
    }
    for rt in &mut para.range_tags {
        apply(&mut rt.start, true);
        apply(&mut rt.end, true);
    }
}

/// 텍스트 오프셋 K 에 컨트롤을 넣을 때의 스트림 삽입점.
/// K 앞 글자들·같은 자리 선행 컨트롤들 뒤 = "K번째 글자의 현재 스트림 위치"(없으면 스트림 끝).
fn control_stream_point(para: &crate::model::paragraph::Paragraph, text_offset: usize) -> u32 {
    if text_offset < para.char_offsets.len() {
        return para.char_offsets[text_offset];
    }
    // 문단 끝: 마지막 글자 끝 + 후행 컨트롤들
    let text_len = para.text.chars().count();
    let last_end = para
        .char_offsets
        .last()
        .copied()
        .map(|o| o + 1)
        .unwrap_or(0);
    let trailing = para
        .control_text_positions()
        .iter()
        .filter(|&&p| p >= text_len)
        .count() as u32;
    last_end + trailing * 8
}

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

        let (def_w, mut def_h, def_caption, back_color, border_type) = hancom_defaults(form_type);
        // 한컴 정본 크기(1984 HWPUNIT ≈ 19.8pt)는 **개체만 있는 문단** 기준이다. 10pt 본문 사이에
        // 그대로 넣으면 줄 높이가 두 배로 뛰어 글자가 아래로 밀린다(2026-08-03 사용자 신고).
        // 그래서 삽입 지점의 글자 크기에 맞춰 줄인다 — 가로세로 비는 정본 그대로.
        let mut def_w = def_w;
        {
            let para = &self.document.sections[section_idx].paragraphs[para_idx];
            let cs_id = para.char_shape_id_at(char_offset).unwrap_or(0);
            // ResolvedCharStyle.font_size 는 px — HWPUNIT 로 되돌린다(96dpi 기준 1pt=100).
            let font_px = self
                .styles
                .char_styles
                .get(cs_id as usize)
                .map(|s| s.font_size)
                .filter(|v| *v > 0.0)
                .unwrap_or(13.333);
            let font_size = (font_px * 72.0 / 96.0 * 100.0).round() as u32;
            // 글자 잉크와 같은 높이(1.0em) — 조판은 "개체 > 글자 잉크"면 줄을 개체 높이로
            // 키우는 한컴 규칙이라(form-01.hwp 실측: lh=개체높이, baseline=0.85), 1.4em 로
            // 넣으면 개체 하나에 줄이 커지고 글자가 아래로 몰린다(2026-08-04 사용자 신고
            // "중앙정렬이 하단정렬로 내려가"). 1.0em 이면 줄이 전혀 안 변한다.
            // 더 큰 개체를 원하면 속성 패널에서 키우면 되고, 그때는 정본대로 줄이 커진다.
            let target_h = font_size;
            if target_h > 0 && target_h < def_h {
                def_w = ((def_w as f64) * (target_h as f64) / (def_h as f64)).round() as u32;
                def_h = target_h;
            }
        }
        let name = json_str(props_json, "name").unwrap_or_else(|| format!("{:?}", form_type));
        let caption = json_str(props_json, "caption").unwrap_or_else(|| def_caption.to_string());
        let text = json_str(props_json, "text").unwrap_or_default();
        let width = json_i32(props_json, "width")
            .map(|v| v as u32)
            .unwrap_or(def_w);
        let height = json_i32(props_json, "height")
            .map(|v| v as u32)
            .unwrap_or(def_h);

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

        // ⚠ 머리말이 낀 문서 등에서 ctrl_data_records 가 controls 보다 짧게 로드되는
        //   문단이 있다 — 그대로 insert 하면 인덱스 초과 panic(2026-08-04 실측, wasm 이
        //   통째로 죽어 "처음부터 아무것도 안 되는" 증상이 됐다). 길이를 맞춰 놓고 넣는다.
        while paragraph.ctrl_data_records.len() < paragraph.controls.len() {
            paragraph.ctrl_data_records.push(None);
        }
        paragraph
            .controls
            .insert(insert_idx, Control::Form(Box::new(form)));
        paragraph.ctrl_data_records.insert(insert_idx, None);

        // 확장 컨트롤 = 본문 8 WCHAR (수식·표와 같은 규약)
        // ⚠ 글자 위치와 서식 경계를 **함께** 민다 — 하나만 밀면 개체 뒤 글자가 남의 서식을 쓴다.
        {
            let text_len = paragraph.text.chars().count();
            let safe_offset = char_offset.min(text_len);
            let at = control_stream_point(paragraph, safe_offset);
            shift_stream_refs(paragraph, at, 8);
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

    /// 양식 개체 속성을 바꾼다 — 이름·캡션·텍스트·값·크기·색·활성화·그룹.
    ///
    /// `props_json` 에 온 키만 바꾼다(부분 갱신). 크기는 HWPUNIT.
    /// ponytail: 본문 문단 전용 — 셀 안 양식은 setFormValueInCell 배관이 따로 있고,
    ///   속성 편집 UI 가 셀까지 넓어지면 그때 같이 간다.
    pub fn set_form_object_props_native(
        &mut self,
        section_idx: usize,
        para_idx: usize,
        control_idx: usize,
        props_json: &str,
    ) -> Result<String, HwpError> {
        use crate::document_core::helpers::{json_bool, json_i32, json_str};

        let section = self
            .document
            .sections
            .get_mut(section_idx)
            .ok_or_else(|| HwpError::RenderError(format!("구역 {} 범위 초과", section_idx)))?;
        let paragraph = section
            .paragraphs
            .get_mut(para_idx)
            .ok_or_else(|| HwpError::RenderError(format!("문단 {} 범위 초과", para_idx)))?;
        let Some(Control::Form(form)) = paragraph.controls.get_mut(control_idx) else {
            return Err(HwpError::InvalidField("양식 개체가 아닙니다".into()));
        };

        if let Some(v) = json_str(props_json, "name") {
            form.name = v;
        }
        if let Some(v) = json_str(props_json, "caption") {
            form.caption = v;
        }
        if let Some(v) = json_str(props_json, "text") {
            form.text = v;
        }
        if let Some(v) = json_i32(props_json, "value") {
            form.value = v;
        }
        if let Some(v) = json_i32(props_json, "width") {
            if v > 0 {
                form.width = v as u32;
            }
        }
        if let Some(v) = json_i32(props_json, "height") {
            if v > 0 {
                form.height = v as u32;
            }
        }
        if let Some(v) = json_bool(props_json, "enabled") {
            form.enabled = v;
        }
        // 색은 0x00BBGGRR 정수로 받는다(#rrggbb 파싱은 JS 쪽 몫 — 경계를 한 곳에 둔다)
        if let Some(v) = json_i32(props_json, "foreColor") {
            form.fore_color = v as u32;
        }
        if let Some(v) = json_i32(props_json, "backColor") {
            form.back_color = v as u32;
        }
        if let Some(v) = json_str(props_json, "groupName") {
            form.properties.insert("GroupName".into(), v);
        }

        // [2026-08-15 신고 "명령 단추·선택 상자·입력 상자 자유 이동이 안 된다"] 시각 오프셋.
        //
        // 양식 개체는 앵커가 글자 사이(인라인)에 있고, 이 오프셋은 그 앵커로부터의
        // **시각 델타**다(HWPUNIT, 음수 허용). 조판 계약은 건드리지 않는다 — 폭 예약·
        // 줄 높이·캐럿 칸은 그대로라 기존 문서는 비트 동일하게 나온다(델타 0 = +0.0).
        //
        // 저장소는 HWPX 정본 키를 그대로 쓴다. 파서(hwpx/section.rs 의 hp:pos)와
        // 직렬화기(serializer/hwpx/form.rs 의 hp:pos)가 이미 이 키를 왕복시키므로
        // 새 키를 만들면 두 번째 진실이 생긴다.
        //
        // ⚠ 한컴 발산: 한컴 실물 코퍼스는 양식을 전수 인라인(오프셋 0)으로만 저장한다
        // (HWPX 190/190, HWP 214/214 실측). 즉 비0 오프셋은 한컴 미채취 영역이고,
        // 한컴에서 열면 오프셋을 무시해 원래 자리로 보일 가능성이 높다. HWP5 직렬화기는
        // 아직 이 값을 쓰지 않으므로 .hwp 로 저장하면 위치가 0으로 돌아간다.
        if let Some(v) = json_i32(props_json, "horzOffset") {
            form.properties
                .insert("PosHorzOffset".into(), v.to_string());
        }
        if let Some(v) = json_i32(props_json, "vertOffset") {
            form.properties
                .insert("PosVertOffset".into(), v.to_string());
        }

        // 콤보 항목 — `"items":["봄","여름",...]`. 정본 저장소는 properties 의 listItem{N}
        // (HWPX 왕복이 이미 이 키를 쓴다). HWP5 는 항목이 스크립트 스트림(JScript)에 사니
        // 거기도 같이 갱신한다 — 안 하면 한컴에서 열었을 때 항목이 옛것으로 남는다.
        let mut new_items: Option<Vec<String>> = None;
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(props_json) {
            if let Some(arr) = v.get("items").and_then(|a| a.as_array()) {
                new_items = Some(
                    arr.iter()
                        .filter_map(|x| x.as_str().map(|s| s.to_string()))
                        .collect(),
                );
            }
        }
        if let Some(items) = &new_items {
            let mut i = 0;
            while form.properties.remove(&format!("listItem{i}")).is_some()
                || form
                    .properties
                    .remove(&format!("listItemDisplay{i}"))
                    .is_some()
            {
                i += 1;
            }
            for (i, item) in items.iter().enumerate() {
                form.properties.insert(format!("listItem{i}"), item.clone());
            }
        }
        let script_sync = new_items.map(|items| (form.name.clone(), form.text.clone(), items));

        section.raw_stream = None;
        if let Some((name, text, items)) = script_sync {
            sync_combobox_script(&mut self.document, &name, &text, &items);
        }

        self.recompose_section(section_idx);
        self.paginate_if_needed();
        self.invalidate_page_tree_cache();
        Ok("{\"ok\":true}".to_string())
    }

    /// 양식 개체를 텍스트 안에서 옮긴다 — 개체 드래그/화살표 이동의 배관.
    ///
    /// `props_json`:
    /// - `{"delta":-1}` / `{"delta":1}`  — 한 글자 왼쪽/오른쪽 (화살표)
    /// - `{"toPara":N,"offset":M}`      — 절대 위치(드래그 낙하점, 텍스트 좌표)
    ///
    /// 구현은 "빼기(장부 -8) + 다시 넣기(장부 +8)" — 삽입·삭제와 같은 규약이라
    /// 스트림 장부가 어긋날 여지를 새로 만들지 않는다. 반환에 새 controlIdx 를 준다.
    pub fn move_form_object_native(
        &mut self,
        section_idx: usize,
        para_idx: usize,
        control_idx: usize,
        props_json: &str,
    ) -> Result<String, HwpError> {
        use crate::document_core::helpers::{find_control_text_positions, json_i32};

        let section = self
            .document
            .sections
            .get_mut(section_idx)
            .ok_or_else(|| HwpError::RenderError(format!("구역 {} 범위 초과", section_idx)))?;
        let paragraph = section
            .paragraphs
            .get_mut(para_idx)
            .ok_or_else(|| HwpError::RenderError(format!("문단 {} 범위 초과", para_idx)))?;
        if !matches!(paragraph.controls.get(control_idx), Some(Control::Form(_))) {
            return Err(HwpError::InvalidField("양식 개체가 아닙니다".into()));
        }

        let positions = find_control_text_positions(paragraph);
        let cur_pos = positions.get(control_idx).copied().unwrap_or(0);
        let text_len = paragraph.text.chars().count();

        let to_para = json_i32(props_json, "toPara")
            .map(|v| v as usize)
            .unwrap_or(para_idx);
        let target_offset = if let Some(delta) = json_i32(props_json, "delta") {
            let t = cur_pos as i64 + delta as i64;
            t.clamp(0, text_len as i64) as usize
        } else if let Some(off) = json_i32(props_json, "offset") {
            off.max(0) as usize
        } else {
            return Err(HwpError::InvalidField(
                "delta 또는 offset 이 필요합니다".into(),
            ));
        };

        if to_para == para_idx && target_offset == cur_pos {
            return Ok(format!(
                "{{\"ok\":true,\"paraIdx\":{para_idx},\"controlIdx\":{control_idx}}}"
            ));
        }
        if to_para >= section.paragraphs.len() {
            return Err(HwpError::RenderError(format!(
                "대상 문단 {} 범위 초과",
                to_para
            )));
        }

        // ── 빼기 (delete_form_object_native 와 같은 장부) ──
        let paragraph = &mut section.paragraphs[para_idx];
        let form_ctrl = paragraph.controls.remove(control_idx);
        let record = if control_idx < paragraph.ctrl_data_records.len() {
            paragraph.ctrl_data_records.remove(control_idx)
        } else {
            None
        };
        let safe = cur_pos.min(paragraph.text.chars().count());
        {
            let at = control_stream_point(paragraph, safe).saturating_sub(8);
            shift_stream_refs(paragraph, at, -8);
        }
        for co in paragraph.char_offsets[safe..].iter_mut() {
            *co = co.saturating_sub(8);
        }
        paragraph.char_count = paragraph.char_count.saturating_sub(8);

        // ── 다시 넣기 (insert_form_object_native 와 같은 장부) ──
        let dest = &mut section.paragraphs[to_para];
        let dest_len = dest.text.chars().count();
        let target_offset = target_offset.min(dest_len);
        let insert_idx = {
            let positions = find_control_text_positions(dest);
            let mut idx = dest.controls.len();
            for (i, &pos) in positions.iter().enumerate() {
                if pos > target_offset
                    || (pos == target_offset && i >= control_idx && to_para == para_idx)
                {
                    idx = i;
                    break;
                }
            }
            idx
        };
        while dest.ctrl_data_records.len() < dest.controls.len() {
            dest.ctrl_data_records.push(None);
        }
        dest.controls.insert(insert_idx, form_ctrl);
        dest.ctrl_data_records.insert(insert_idx, record);
        {
            let safe_offset = target_offset.min(dest_len);
            let at = control_stream_point(dest, safe_offset);
            shift_stream_refs(dest, at, 8);
            for co in dest.char_offsets[safe_offset..].iter_mut() {
                *co += 8;
            }
        }
        dest.char_count += 8;
        dest.control_mask |= 1u32 << 11;
        dest.has_para_text = true;

        section.raw_stream = None;
        self.recompose_section(section_idx);
        self.paginate_if_needed();
        self.invalidate_page_tree_cache();
        Ok(format!(
            "{{\"ok\":true,\"paraIdx\":{to_para},\"controlIdx\":{insert_idx}}}"
        ))
    }

    /// 논리 칸 `logical` 을 차지하는 양식 개체의 컨트롤 인덱스(없으면 -1).
    ///
    /// Backspace/Delete 가 개체를 글자처럼 지우기 위한 조회 — 캐럿 앞/뒤 칸이 양식이면
    /// 텍스트 삭제 대신 개체 삭제로 갈라탄다(한컴 동작).
    pub fn form_control_at_logical_native(
        &self,
        section_idx: usize,
        para_idx: usize,
        logical: usize,
    ) -> i32 {
        use crate::document_core::helpers::{
            find_control_text_positions, is_logical_inline_control,
        };
        let Some(section) = self.document.sections.get(section_idx) else {
            return -1;
        };
        let Some(para) = section.paragraphs.get(para_idx) else {
            return -1;
        };
        let positions = find_control_text_positions(para);
        // 스트림 순서대로 걸으며 각 논리 인라인 컨트롤의 논리 칸을 센다:
        // 컨트롤의 논리 칸 = 텍스트 위치 + (그보다 앞에 선 논리 인라인 컨트롤 수)
        let mut inline_seen = 0usize;
        for (ci, ctrl) in para.controls.iter().enumerate() {
            if !is_logical_inline_control(ctrl) {
                continue;
            }
            let pos = positions.get(ci).copied().unwrap_or(0);
            let slot = pos + inline_seen;
            if slot == logical {
                return if matches!(ctrl, Control::Form(_)) {
                    ci as i32
                } else {
                    -1
                };
            }
            inline_seen += 1;
        }
        -1
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
        {
            let at = control_stream_point(paragraph, safe).saturating_sub(8);
            shift_stream_refs(paragraph, at, -8);
        }
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

// ─── HWP5 스크립트 스트림(콤보 항목의 정본 저장소) ─────────────────────────
//
// 한컴은 콤보 항목을 /Scripts/DefaultJScript 에 `Name.InsertString("항목", i);` 로 저장한다.
// 스트림 구조(samples/form-01.hwp 실측, 2026-08-03):
//   raw-deflate( [u32 글자수][UTF-16LE 본문] × N ... [u32 0][u32 0][0xFFFFFFFF] )
//   세그먼트0 = 선언부(var …), 세그먼트1 = 함수/블록부.
// JScriptVersion 은 raw-deflate(u32 1, u32 0).

fn decode_script_segments(data: &[u8]) -> Option<Vec<String>> {
    use std::io::Read;
    let mut d = flate2::read::DeflateDecoder::new(data);
    let mut out = Vec::new();
    d.read_to_end(&mut out).ok()?;
    let mut segs = Vec::new();
    let mut off = 0usize;
    while off + 4 <= out.len() {
        let n = u32::from_le_bytes([out[off], out[off + 1], out[off + 2], out[off + 3]]);
        if n == 0xFFFF_FFFF {
            break;
        }
        let n = n as usize;
        let end = off + 4 + n * 2;
        if end > out.len() {
            return None;
        }
        let u16s: Vec<u16> = out[off + 4..end]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        segs.push(String::from_utf16_lossy(&u16s));
        off = end;
    }
    // 뒤의 빈 세그(0,0)는 종결 관례 — 내용 세그만 돌려준다
    while segs.last().is_some_and(|s| s.is_empty()) {
        segs.pop();
    }
    Some(segs)
}

fn encode_script_segments(segs: &[String]) -> Vec<u8> {
    use std::io::Write;
    let mut raw = Vec::new();
    for seg in segs {
        let u16s: Vec<u16> = seg.encode_utf16().collect();
        raw.extend_from_slice(&(u16s.len() as u32).to_le_bytes());
        for u in u16s {
            raw.extend_from_slice(&u.to_le_bytes());
        }
    }
    raw.extend_from_slice(&0u32.to_le_bytes());
    raw.extend_from_slice(&0u32.to_le_bytes());
    raw.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());

    let mut enc = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
    enc.write_all(&raw).ok();
    enc.finish().unwrap_or_default()
}

/// 콤보 항목을 스크립트 스트림에 반영한다 — 이 콤보의 옛 줄만 걷어내고 정본 블록을 덧붙인다.
fn sync_combobox_script(
    document: &mut crate::model::document::Document,
    name: &str,
    text: &str,
    items: &[String],
) {
    let stream_key = "/Scripts/DefaultJScript";
    let existing = document
        .extra_streams
        .iter()
        .find(|(p, _)| p == stream_key || p == "Scripts/DefaultJScript")
        .and_then(|(_, d)| decode_script_segments(d));

    let (mut decl, mut body) = match existing {
        Some(segs) if !segs.is_empty() => {
            let decl = segs.first().cloned().unwrap_or_default();
            let body = segs.get(1).cloned().unwrap_or_default();
            (decl, body)
        }
        _ => (
            "var Documents = XHwpDocuments;\r\nvar Document = Documents.Active_XHwpDocument;\r\n"
                .to_string(),
            "function OnDocument_Open()\r\n{\r\n\t//todo : \r\n}\r\n\r\nfunction OnDocument_New()\r\n{\r\n\t//todo : \r\n}\r\n"
                .to_string(),
        ),
    };

    // 선언이 없으면 추가 (한컴 정본 문구)
    let decl_line = format!("var {name} = Document.XHwpFormComboBoxs.ItemFromName(\"{name}\");");
    if !decl.contains(&decl_line) {
        if !decl.ends_with('\n') {
            decl.push_str("\r\n");
        }
        decl.push_str(&decl_line);
        decl.push_str("\r\n");
    }

    // 이 콤보의 옛 항목 줄 제거 — 남는 빈 {} 블록은 JS 에서 무해하다
    let prefix_calls = [
        format!("{name}.Enabled"),
        format!("{name}.ResetContent"),
        format!("{name}.Text"),
        format!("{name}.InsertString"),
    ];
    body = body
        .lines()
        .filter(|line| {
            let t = line.trim_start();
            !prefix_calls.iter().any(|p| t.starts_with(p.as_str()))
        })
        .collect::<Vec<_>>()
        .join("\r\n");

    // 정본 블록 덧붙이기 (form-01.hwp 실측 형태 그대로)
    body.push_str("\r\n{\r\n");
    body.push_str(&format!("{name}.Enabled = 1;\r\n"));
    body.push_str(&format!("{name}.ResetContent();\r\n"));
    body.push_str(&format!("{name}.Text =\"{text}\"\r\n"));
    for (i, item) in items.iter().enumerate() {
        body.push_str(&format!("{name}.InsertString(\"{item}\",{i});\r\n"));
    }
    body.push_str("}\r\n");

    let encoded = encode_script_segments(&[decl, body]);
    if let Some(slot) = document
        .extra_streams
        .iter_mut()
        .find(|(p, _)| p == stream_key || p == "Scripts/DefaultJScript")
    {
        slot.1 = encoded;
    } else {
        document
            .extra_streams
            .push((stream_key.to_string(), encoded));
    }

    // JScriptVersion 이 없으면 정본(버전 1.0)으로 만들어 둔다
    if !document
        .extra_streams
        .iter()
        .any(|(p, _)| p.contains("JScriptVersion"))
    {
        let mut raw = Vec::new();
        raw.extend_from_slice(&1u32.to_le_bytes());
        raw.extend_from_slice(&0u32.to_le_bytes());
        use std::io::Write;
        let mut enc =
            flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        enc.write_all(&raw).ok();
        document.extra_streams.push((
            "/Scripts/JScriptVersion".to_string(),
            enc.finish().unwrap_or_default(),
        ));
    }
}
