//! 머리말/꼬리말 생성·조회·텍스트 편집 관련 native 메서드

use crate::document_core::helpers::{
    build_tab_def_from_json, json_has_border_keys, json_has_tab_keys, parse_json_i16_array,
    parse_para_shape_mods,
};
use crate::document_core::DocumentCore;
use crate::error::HwpError;
use crate::model::control::Control;
use crate::model::event::DocumentEvent;
use crate::model::header_footer::{Footer, Header, HeaderFooterApply};
use crate::model::paragraph::Paragraph;
use crate::renderer::composer::reflow_line_segs;

/// applyTo u8 값 → HeaderFooterApply 변환
fn apply_from_u8(v: u8) -> HeaderFooterApply {
    match v {
        1 => HeaderFooterApply::Even,
        2 => HeaderFooterApply::Odd,
        _ => HeaderFooterApply::Both,
    }
}

/// HeaderFooterApply → u8 변환
fn apply_to_u8(a: HeaderFooterApply) -> u8 {
    match a {
        HeaderFooterApply::Both => 0,
        HeaderFooterApply::Even => 1,
        HeaderFooterApply::Odd => 2,
    }
}

/// HeaderFooterApply → 표시 레이블
fn apply_label(a: HeaderFooterApply) -> &'static str {
    match a {
        HeaderFooterApply::Both => "양 쪽",
        HeaderFooterApply::Even => "짝수 쪽",
        HeaderFooterApply::Odd => "홀수 쪽",
    }
}

impl DocumentCore {
    /// 구역의 문단들에서 특정 apply_to의 머리말 또는 꼬리말 컨트롤 위치를 찾는다.
    /// 반환: (para_index, control_index)
    fn find_header_footer_control(
        &self,
        section_idx: usize,
        is_header: bool,
        apply_to: HeaderFooterApply,
    ) -> Option<(usize, usize)> {
        let section = self.document.sections.get(section_idx)?;
        for (pi, para) in section.paragraphs.iter().enumerate() {
            for (ci, ctrl) in para.controls.iter().enumerate() {
                match ctrl {
                    Control::Header(h) if is_header && h.apply_to == apply_to => {
                        return Some((pi, ci));
                    }
                    Control::Footer(f) if !is_header && f.apply_to == apply_to => {
                        return Some((pi, ci));
                    }
                    _ => {}
                }
            }
        }
        None
    }

    /// 머리말/꼬리말 조회 — JSON 반환
    ///
    /// 존재하면: `{"ok":true,"exists":true,"applyTo":0,"paraCount":N,"text":"..."}`
    /// 없으면: `{"ok":true,"exists":false}`
    pub fn get_header_footer_native(
        &self,
        section_idx: usize,
        is_header: bool,
        apply_to: u8,
    ) -> Result<String, HwpError> {
        if section_idx >= self.document.sections.len() {
            return Err(HwpError::RenderError(format!(
                "구역 인덱스 {} 범위 초과 (총 {}개)",
                section_idx,
                self.document.sections.len()
            )));
        }
        let apply = apply_from_u8(apply_to);
        if let Some((pi, ci)) = self.find_header_footer_control(section_idx, is_header, apply) {
            let section = &self.document.sections[section_idx];
            let ctrl = &section.paragraphs[pi].controls[ci];
            let (paragraphs, at) = match ctrl {
                Control::Header(h) => (&h.paragraphs, h.apply_to),
                Control::Footer(f) => (&f.paragraphs, f.apply_to),
                _ => unreachable!(),
            };
            let text: String = paragraphs
                .iter()
                .map(|p| p.text.clone())
                .collect::<Vec<_>>()
                .join("\n");
            let kind = if is_header { "header" } else { "footer" };
            let label = apply_label(at);
            Ok(format!(
                "{{\"ok\":true,\"exists\":true,\"kind\":\"{}\",\"applyTo\":{},\"label\":\"{}\",\"paraIndex\":{},\"controlIndex\":{},\"paraCount\":{},\"text\":\"{}\"}}",
                kind, apply_to_u8(at), label, pi, ci, paragraphs.len(),
                super::super::helpers::json_escape(&text)
            ))
        } else {
            Ok(format!("{{\"ok\":true,\"exists\":false}}"))
        }
    }

    /// 머리말/꼬리말 생성 — 빈 문단 1개 포함
    ///
    /// 이미 같은 apply_to의 머리말/꼬리말이 있으면 에러.
    /// 구역의 첫 번째 문단(SectionDef 컨트롤이 있는 문단)에 컨트롤을 추가한다.
    pub fn create_header_footer_native(
        &mut self,
        section_idx: usize,
        is_header: bool,
        apply_to: u8,
    ) -> Result<String, HwpError> {
        if section_idx >= self.document.sections.len() {
            return Err(HwpError::RenderError(format!(
                "구역 인덱스 {} 범위 초과 (총 {}개)",
                section_idx,
                self.document.sections.len()
            )));
        }
        let apply = apply_from_u8(apply_to);
        if self
            .find_header_footer_control(section_idx, is_header, apply)
            .is_some()
        {
            let kind = if is_header { "머리말" } else { "꼬리말" };
            return Err(HwpError::RenderError(format!(
                "이미 {}({})이 존재합니다",
                kind,
                apply_label(apply)
            )));
        }

        // 빈 문단 생성
        let empty_para = Paragraph::default();

        // 컨트롤 생성
        let ctrl = if is_header {
            Control::Header(Box::new(Header {
                apply_to: apply,
                paragraphs: vec![empty_para],
                raw_attr: apply_to as u32,
                raw_ctrl_extra: Vec::new(),
                ..Default::default()
            }))
        } else {
            Control::Footer(Box::new(Footer {
                apply_to: apply,
                paragraphs: vec![empty_para],
                raw_attr: apply_to as u32,
                raw_ctrl_extra: Vec::new(),
                ..Default::default()
            }))
        };

        // 구역의 첫 번째 문단에 컨트롤 추가 (SectionDef 컨트롤이 있는 곳)
        let section = &mut self.document.sections[section_idx];
        if section.paragraphs.is_empty() {
            return Err(HwpError::RenderError("구역에 문단이 없습니다".to_string()));
        }
        section.paragraphs[0].controls.push(ctrl);
        // 컨트롤 1개 = UTF-16 8 code units → char_count 갱신
        section.paragraphs[0].char_count += 8;
        section.raw_stream = None;

        // 재페이지네이션 (머리말/꼬리말이 추가되면 페이지 레이아웃에 영향)
        self.mark_section_dirty(section_idx);
        self.paginate_if_needed();

        // 생성된 컨트롤 위치 반환
        let (pi, ci) = self
            .find_header_footer_control(section_idx, is_header, apply)
            .expect("방금 생성한 컨트롤을 찾을 수 없음");

        let kind = if is_header { "header" } else { "footer" };
        let label = apply_label(apply);
        Ok(format!(
            "{{\"ok\":true,\"kind\":\"{}\",\"applyTo\":{},\"label\":\"{}\",\"paraIndex\":{},\"controlIndex\":{}}}",
            kind, apply_to, label, pi, ci
        ))
    }

    /// 머리말/꼬리말 내부 문단에 대한 가변 참조를 얻는다.
    fn get_hf_paragraph_mut(
        &mut self,
        section_idx: usize,
        is_header: bool,
        apply_to: u8,
        hf_para_idx: usize,
    ) -> Result<&mut Paragraph, HwpError> {
        let apply = apply_from_u8(apply_to);
        let (pi, ci) = self
            .find_header_footer_control(section_idx, is_header, apply)
            .ok_or_else(|| {
                let kind = if is_header { "머리말" } else { "꼬리말" };
                HwpError::RenderError(format!(
                    "{}({})이 존재하지 않습니다",
                    kind,
                    apply_label(apply)
                ))
            })?;

        let ctrl = &mut self.document.sections[section_idx].paragraphs[pi].controls[ci];
        match ctrl {
            Control::Header(h) => {
                if hf_para_idx >= h.paragraphs.len() {
                    return Err(HwpError::RenderError(format!(
                        "머리말 문단 인덱스 {} 범위 초과 (총 {}개)",
                        hf_para_idx,
                        h.paragraphs.len()
                    )));
                }
                Ok(&mut h.paragraphs[hf_para_idx])
            }
            Control::Footer(f) => {
                if hf_para_idx >= f.paragraphs.len() {
                    return Err(HwpError::RenderError(format!(
                        "꼬리말 문단 인덱스 {} 범위 초과 (총 {}개)",
                        hf_para_idx,
                        f.paragraphs.len()
                    )));
                }
                Ok(&mut f.paragraphs[hf_para_idx])
            }
            _ => Err(HwpError::RenderError(
                "컨트롤이 머리말/꼬리말이 아닙니다".to_string(),
            )),
        }
    }

    /// 머리말/꼬리말 내부 문단에 대한 불변 참조를 얻는다.
    fn get_hf_paragraph_ref(
        &self,
        section_idx: usize,
        is_header: bool,
        apply_to: u8,
        hf_para_idx: usize,
    ) -> Option<&Paragraph> {
        let apply = apply_from_u8(apply_to);
        let (pi, ci) = self.find_header_footer_control(section_idx, is_header, apply)?;
        let ctrl = &self.document.sections[section_idx].paragraphs[pi].controls[ci];
        match ctrl {
            Control::Header(h) => h.paragraphs.get(hf_para_idx),
            Control::Footer(f) => f.paragraphs.get(hf_para_idx),
            _ => None,
        }
    }

    /// 머리말/꼬리말 내 텍스트 삽입
    pub fn insert_text_in_header_footer_native(
        &mut self,
        section_idx: usize,
        is_header: bool,
        apply_to: u8,
        hf_para_idx: usize,
        char_offset: usize,
        text: &str,
    ) -> Result<String, HwpError> {
        if section_idx >= self.document.sections.len() {
            return Err(HwpError::RenderError(format!(
                "구역 인덱스 {} 범위 초과 (총 {}개)",
                section_idx,
                self.document.sections.len()
            )));
        }

        let hf_para = self.get_hf_paragraph_mut(section_idx, is_header, apply_to, hf_para_idx)?;
        let new_chars_count = text.chars().count();
        hf_para.insert_text_at(char_offset, text);

        // 리플로우 (머리말/꼬리말 영역 폭 기반)
        self.reflow_hf_paragraph(section_idx, is_header, apply_to, hf_para_idx);

        // raw 스트림 무효화, 재페이지네이션
        self.document.sections[section_idx].raw_stream = None;
        self.mark_section_dirty(section_idx);
        self.paginate_if_needed();

        let new_offset = char_offset + new_chars_count;
        self.event_log.push(DocumentEvent::TextInserted {
            section: section_idx,
            para: 0,
            offset: char_offset,
            len: new_chars_count,
        });
        Ok(super::super::helpers::json_ok_with(&format!(
            "\"charOffset\":{}",
            new_offset
        )))
    }

    /// 머리말/꼬리말 내 텍스트 삭제
    pub fn delete_text_in_header_footer_native(
        &mut self,
        section_idx: usize,
        is_header: bool,
        apply_to: u8,
        hf_para_idx: usize,
        char_offset: usize,
        count: usize,
    ) -> Result<String, HwpError> {
        if section_idx >= self.document.sections.len() {
            return Err(HwpError::RenderError(format!(
                "구역 인덱스 {} 범위 초과 (총 {}개)",
                section_idx,
                self.document.sections.len()
            )));
        }

        let hf_para = self.get_hf_paragraph_mut(section_idx, is_header, apply_to, hf_para_idx)?;
        hf_para.delete_text_at(char_offset, count);

        // 리플로우
        self.reflow_hf_paragraph(section_idx, is_header, apply_to, hf_para_idx);

        // raw 스트림 무효화, 재페이지네이션
        self.document.sections[section_idx].raw_stream = None;
        self.mark_section_dirty(section_idx);
        self.paginate_if_needed();

        self.event_log.push(DocumentEvent::TextDeleted {
            section: section_idx,
            para: 0,
            offset: char_offset,
            count,
        });
        Ok(super::super::helpers::json_ok_with(&format!(
            "\"charOffset\":{}",
            char_offset
        )))
    }

    /// 머리말/꼬리말 내 문단 분할 (Enter 키)
    pub fn split_paragraph_in_header_footer_native(
        &mut self,
        section_idx: usize,
        is_header: bool,
        apply_to: u8,
        hf_para_idx: usize,
        char_offset: usize,
    ) -> Result<String, HwpError> {
        if section_idx >= self.document.sections.len() {
            return Err(HwpError::RenderError(format!(
                "구역 인덱스 {} 범위 초과",
                section_idx
            )));
        }

        let apply = apply_from_u8(apply_to);
        let (pi, ci) = self
            .find_header_footer_control(section_idx, is_header, apply)
            .ok_or_else(|| {
                let kind = if is_header { "머리말" } else { "꼬리말" };
                HwpError::RenderError(format!("{}이 존재하지 않습니다", kind))
            })?;

        // 문단 분할
        let new_para = {
            let ctrl = &mut self.document.sections[section_idx].paragraphs[pi].controls[ci];
            let paragraphs = match ctrl {
                Control::Header(h) => &mut h.paragraphs,
                Control::Footer(f) => &mut f.paragraphs,
                _ => return Err(HwpError::RenderError("컨트롤 타입 불일치".to_string())),
            };
            if hf_para_idx >= paragraphs.len() {
                return Err(HwpError::RenderError(format!(
                    "문단 인덱스 {} 범위 초과",
                    hf_para_idx
                )));
            }
            paragraphs[hf_para_idx].split_at(char_offset)
        };

        // 새 문단 삽입
        let new_para_idx = hf_para_idx + 1;
        {
            let ctrl = &mut self.document.sections[section_idx].paragraphs[pi].controls[ci];
            let paragraphs = match ctrl {
                Control::Header(h) => &mut h.paragraphs,
                Control::Footer(f) => &mut f.paragraphs,
                _ => unreachable!(),
            };
            paragraphs.insert(new_para_idx, new_para);
        }

        // 리플로우
        self.reflow_hf_paragraph(section_idx, is_header, apply_to, hf_para_idx);
        self.reflow_hf_paragraph(section_idx, is_header, apply_to, new_para_idx);

        self.document.sections[section_idx].raw_stream = None;
        self.mark_section_dirty(section_idx);
        self.paginate_if_needed();

        self.event_log.push(DocumentEvent::ParagraphSplit {
            section: section_idx,
            para: hf_para_idx,
            offset: char_offset,
        });
        Ok(super::super::helpers::json_ok_with(&format!(
            "\"hfParaIndex\":{},\"charOffset\":0",
            new_para_idx
        )))
    }

    /// 머리말/꼬리말 내 문단 병합 (Backspace at start)
    pub fn merge_paragraph_in_header_footer_native(
        &mut self,
        section_idx: usize,
        is_header: bool,
        apply_to: u8,
        hf_para_idx: usize,
    ) -> Result<String, HwpError> {
        if section_idx >= self.document.sections.len() {
            return Err(HwpError::RenderError(format!(
                "구역 인덱스 {} 범위 초과",
                section_idx
            )));
        }
        if hf_para_idx == 0 {
            return Err(HwpError::RenderError(
                "첫 번째 문단은 이전 문단과 병합할 수 없습니다".to_string(),
            ));
        }

        let apply = apply_from_u8(apply_to);
        let (pi, ci) = self
            .find_header_footer_control(section_idx, is_header, apply)
            .ok_or_else(|| {
                let kind = if is_header { "머리말" } else { "꼬리말" };
                HwpError::RenderError(format!("{}이 존재하지 않습니다", kind))
            })?;

        // 병합
        let merge_offset;
        {
            let ctrl = &mut self.document.sections[section_idx].paragraphs[pi].controls[ci];
            let paragraphs = match ctrl {
                Control::Header(h) => &mut h.paragraphs,
                Control::Footer(f) => &mut f.paragraphs,
                _ => return Err(HwpError::RenderError("컨트롤 타입 불일치".to_string())),
            };
            if hf_para_idx >= paragraphs.len() {
                return Err(HwpError::RenderError(format!(
                    "문단 인덱스 {} 범위 초과",
                    hf_para_idx
                )));
            }
            merge_offset = paragraphs[hf_para_idx - 1].text.chars().count();
            let removed = paragraphs.remove(hf_para_idx);
            paragraphs[hf_para_idx - 1].merge_from(&removed);
        }

        let prev_idx = hf_para_idx - 1;
        self.reflow_hf_paragraph(section_idx, is_header, apply_to, prev_idx);

        self.document.sections[section_idx].raw_stream = None;
        self.mark_section_dirty(section_idx);
        self.paginate_if_needed();

        self.event_log.push(DocumentEvent::ParagraphMerged {
            section: section_idx,
            para: hf_para_idx,
        });
        Ok(super::super::helpers::json_ok_with(&format!(
            "\"hfParaIndex\":{},\"charOffset\":{}",
            prev_idx, merge_offset
        )))
    }

    /// 머리말/꼬리말 문단의 정보를 반환한다 (문단 수, 현재 문단 텍스트 길이 등).
    pub fn get_header_footer_para_info_native(
        &self,
        section_idx: usize,
        is_header: bool,
        apply_to: u8,
        hf_para_idx: usize,
    ) -> Result<String, HwpError> {
        if section_idx >= self.document.sections.len() {
            return Err(HwpError::RenderError(format!(
                "구역 인덱스 {} 범위 초과",
                section_idx
            )));
        }
        let apply = apply_from_u8(apply_to);
        let (pi, ci) = self
            .find_header_footer_control(section_idx, is_header, apply)
            .ok_or_else(|| {
                let kind = if is_header { "머리말" } else { "꼬리말" };
                HwpError::RenderError(format!("{}이 존재하지 않습니다", kind))
            })?;

        let ctrl = &self.document.sections[section_idx].paragraphs[pi].controls[ci];
        let paragraphs = match ctrl {
            Control::Header(h) => &h.paragraphs,
            Control::Footer(f) => &f.paragraphs,
            _ => return Err(HwpError::RenderError("컨트롤 타입 불일치".to_string())),
        };

        let para_count = paragraphs.len();
        let char_count = if hf_para_idx < para_count {
            paragraphs[hf_para_idx].text.chars().count()
        } else {
            0
        };

        Ok(format!(
            "{{\"ok\":true,\"paraCount\":{},\"charCount\":{}}}",
            para_count, char_count
        ))
    }

    /// 머리말/꼬리말 삭제 (컨트롤 자체를 제거)
    pub fn delete_header_footer_native(
        &mut self,
        section_idx: usize,
        is_header: bool,
        apply_to: u8,
    ) -> Result<String, HwpError> {
        if section_idx >= self.document.sections.len() {
            return Err(HwpError::RenderError(format!(
                "구역 인덱스 {} 범위 초과 (총 {}개)",
                section_idx,
                self.document.sections.len()
            )));
        }
        let apply = apply_from_u8(apply_to);
        let (pi, ci) = self
            .find_header_footer_control(section_idx, is_header, apply)
            .ok_or_else(|| {
                let kind = if is_header { "머리말" } else { "꼬리말" };
                HwpError::RenderError(format!(
                    "{}({})이 존재하지 않습니다",
                    kind,
                    apply_label(apply)
                ))
            })?;

        self.document.sections[section_idx].paragraphs[pi]
            .controls
            .remove(ci);
        // 컨트롤 1개 = UTF-16 8 code units → char_count 갱신
        self.document.sections[section_idx].paragraphs[pi].char_count =
            self.document.sections[section_idx].paragraphs[pi]
                .char_count
                .saturating_sub(8);
        self.document.sections[section_idx].raw_stream = None;
        self.mark_section_dirty(section_idx);
        self.paginate_if_needed();

        Ok(format!("{{\"ok\":true}}"))
    }

    /// 문서 전체의 머리말/꼬리말 목록을 반환한다.
    ///
    /// JSON: `{"ok":true,"items":[{"sectionIdx":N,"isHeader":bool,"applyTo":N,"label":"..."},...],
    ///         "currentIndex":N}`
    /// currentSectionIdx/currentIsHeader/currentApplyTo로 현재 편집 중인 항목의 인덱스를 반환.
    pub fn get_header_footer_list_native(
        &self,
        current_section_idx: usize,
        current_is_header: bool,
        current_apply_to: u8,
    ) -> Result<String, HwpError> {
        let mut items = Vec::new();
        let mut current_index: i32 = -1;
        let current_apply = apply_from_u8(current_apply_to);

        for (si, section) in self.document.sections.iter().enumerate() {
            for (pi, para) in section.paragraphs.iter().enumerate() {
                for (ci, ctrl) in para.controls.iter().enumerate() {
                    let (is_header, apply) = match ctrl {
                        Control::Header(h) => (true, h.apply_to),
                        Control::Footer(f) => (false, f.apply_to),
                        _ => continue,
                    };
                    let kind = if is_header { "머리말" } else { "꼬리말" };
                    let label = apply_label(apply);
                    let at = apply_to_u8(apply);

                    if si == current_section_idx
                        && is_header == current_is_header
                        && apply == current_apply
                    {
                        current_index = items.len() as i32;
                    }

                    items.push(format!(
                        "{{\"sectionIdx\":{},\"isHeader\":{},\"applyTo\":{},\"label\":\"{}({})\"}}",
                        si, is_header, at, kind, label
                    ));
                }
            }
        }

        Ok(format!(
            "{{\"ok\":true,\"items\":[{}],\"currentIndex\":{}}}",
            items.join(","),
            current_index
        ))
    }

    /// 페이지 단위로 이전/다음 머리말·꼬리말로 이동한다.
    ///
    /// 현재 페이지에서 direction 방향으로 탐색하여 머리말/꼬리말이 있는 다음 페이지를 찾는다.
    /// 홀수/짝수 페이지에 따라 다른 컨트롤(apply_to)을 반환할 수 있다.
    ///
    /// 반환: JSON `{"ok":true,"pageIndex":N,"sectionIdx":N,"isHeader":bool,"applyTo":N}`
    /// 또는 더 이상 이동할 페이지가 없으면 `{"ok":false}`
    pub fn navigate_header_footer_by_page_native(
        &self,
        current_page: u32,
        is_header: bool,
        direction: i32, // -1 또는 +1
    ) -> Result<String, HwpError> {
        let total = self.page_count();
        if total == 0 {
            return Ok("{\"ok\":false}".to_string());
        }

        // 현재 페이지의 머리말/꼬리말 참조 (동일 컨트롤 스킵용)
        let current_ref = if let Ok((pc, _, _)) = self.find_page(current_page) {
            if is_header {
                pc.active_header.clone()
            } else {
                pc.active_footer.clone()
            }
        } else {
            None
        };

        let mut page = current_page as i64 + direction as i64;
        while page >= 0 && page < total as i64 {
            let p = page as u32;
            if let Ok((pc, _, _)) = self.find_page(p) {
                let hf_ref = if is_header {
                    &pc.active_header
                } else {
                    &pc.active_footer
                };
                if let Some(hf) = hf_ref {
                    // 다른 컨트롤이거나, 같은 컨트롤이라도 다른 페이지이면 이동 대상
                    let is_different_control = match &current_ref {
                        Some(cr) => {
                            cr.para_index != hf.para_index
                                || cr.control_index != hf.control_index
                                || cr.source_section_index != hf.source_section_index
                        }
                        None => true,
                    };
                    // 같은 컨트롤이라도 페이지가 달라지면 이동
                    let section_idx = hf.source_section_index;
                    let para_idx = hf.para_index;
                    let ctrl_idx = hf.control_index;

                    // apply_to 추출
                    let apply_to = if let Some(section) = self.document.sections.get(section_idx) {
                        if let Some(para) = section.paragraphs.get(para_idx) {
                            if let Some(ctrl) = para.controls.get(ctrl_idx) {
                                match ctrl {
                                    Control::Header(h) => apply_to_u8(h.apply_to),
                                    Control::Footer(f) => apply_to_u8(f.apply_to),
                                    _ => 0,
                                }
                            } else {
                                0
                            }
                        } else {
                            0
                        }
                    } else {
                        0
                    };

                    return Ok(format!(
                        "{{\"ok\":true,\"pageIndex\":{},\"sectionIdx\":{},\"isHeader\":{},\"applyTo\":{}}}",
                        p, section_idx, is_header, apply_to
                    ));
                }
            }
            page += direction as i64;
        }

        Ok("{\"ok\":false}".to_string())
    }

    /// 특정 페이지의 머리말/꼬리말 감추기를 토글한다.
    ///
    /// 반환: JSON `{"ok":true,"hidden":bool}`
    pub fn toggle_hide_header_footer_native(
        &mut self,
        page_num: u32,
        is_header: bool,
    ) -> Result<String, HwpError> {
        let total = self.page_count();
        if page_num >= total {
            return Err(HwpError::RenderError(format!(
                "페이지 인덱스 {} 범위 초과 (총 {}개)",
                page_num, total
            )));
        }
        let key = (page_num, is_header);
        // [page-section/결함4·5] override 는 자기 이전 값(초기 false=표시)만 뒤집는다.
        // 모델(PageHide/section_def)을 읽지 않으므로, setPageHide 로 감춘 상태에서 처음
        // 토글하면 hidden=true 로 '재확인', 다시 토글하면 hidden=false 로 강제 표시가 되어
        // 응답과 실제 렌더가 항상 일치한다. 값은 getPageHide 로도 되읽힌다.
        let prev = self
            .hidden_header_footer
            .get(&key)
            .copied()
            .unwrap_or(false);
        let hidden = !prev;
        self.hidden_header_footer.insert(key, hidden);
        // 렌더 트리 캐시 무효화
        let mut cache = self.page_tree_cache.borrow_mut();
        if let Some(slot) = cache.get_mut(page_num as usize) {
            *slot = None;
        }
        Ok(format!("{{\"ok\":true,\"hidden\":{}}}", hidden))
    }

    /// 특정 페이지의 머리말/꼬리말이 감추기 상태인지 확인한다.
    pub fn is_header_footer_hidden(&self, page_num: u32, is_header: bool) -> bool {
        self.hidden_header_footer
            .get(&(page_num, is_header))
            .copied()
            .unwrap_or(false)
    }

    /// 머리말/꼬리말 문단 리플로우
    fn reflow_hf_paragraph(
        &mut self,
        section_idx: usize,
        is_header: bool,
        apply_to: u8,
        hf_para_idx: usize,
    ) {
        use crate::renderer::hwpunit_to_px;

        // 머리말/꼬리말 영역 폭 = 페이지 텍스트 영역 폭
        let available_width = {
            let section = &self.document.sections[section_idx];
            let page_def = &section.section_def.page_def;
            let text_width =
                page_def.width as i32 - page_def.margin_left as i32 - page_def.margin_right as i32;
            hwpunit_to_px(text_width, self.dpi)
        };

        // 문단 여백 적용
        let para_shape_id =
            match self.get_hf_paragraph_ref(section_idx, is_header, apply_to, hf_para_idx) {
                Some(p) => p.para_shape_id,
                None => return,
            };
        let para_style = self.styles.para_styles.get(para_shape_id as usize);
        let margin_left = para_style.map(|s| s.margin_left).unwrap_or(0.0);
        let margin_right = para_style.map(|s| s.margin_right).unwrap_or(0.0);
        let final_width = (available_width - margin_left - margin_right).max(0.0);

        // 가변 참조로 리플로우 실행
        let apply = apply_from_u8(apply_to);
        if let Some((pi, ci)) = self.find_header_footer_control(section_idx, is_header, apply) {
            let ctrl = &mut self.document.sections[section_idx].paragraphs[pi].controls[ci];
            let paragraphs = match ctrl {
                Control::Header(h) => &mut h.paragraphs,
                Control::Footer(f) => &mut f.paragraphs,
                _ => return,
            };
            if let Some(para) = paragraphs.get_mut(hf_para_idx) {
                reflow_line_segs(para, final_width, &self.styles, self.dpi);
            }
        }
    }

    /// 머리말/꼬리말 문단의 문단 속성을 조회한다.
    pub fn get_para_properties_in_hf_native(
        &self,
        section_idx: usize,
        is_header: bool,
        apply_to: u8,
        hf_para_idx: usize,
    ) -> Result<String, HwpError> {
        let para = self
            .get_hf_paragraph_ref(section_idx, is_header, apply_to, hf_para_idx)
            .ok_or_else(|| {
                HwpError::RenderError("머리말/꼬리말 문단을 찾을 수 없음".to_string())
            })?;
        Ok(self.build_para_properties_json(para.para_shape_id, section_idx))
    }

    /// 머리말/꼬리말 문단에 문단 서식을 적용한다.
    pub fn apply_para_format_in_hf_native(
        &mut self,
        section_idx: usize,
        is_header: bool,
        apply_to: u8,
        hf_para_idx: usize,
        props_json: &str,
    ) -> Result<String, HwpError> {
        // 현재 para_shape_id 조회
        let base_id = {
            let para = self
                .get_hf_paragraph_ref(section_idx, is_header, apply_to, hf_para_idx)
                .ok_or_else(|| {
                    HwpError::RenderError("머리말/꼬리말 문단을 찾을 수 없음".to_string())
                })?;
            para.para_shape_id
        };

        let mut mods = parse_para_shape_mods(props_json);

        // 탭 설정 변경 처리
        if json_has_tab_keys(props_json) {
            let base_tab_def_id = self
                .document
                .doc_info
                .para_shapes
                .get(base_id as usize)
                .map(|ps| ps.tab_def_id)
                .unwrap_or(0);
            let new_td = build_tab_def_from_json(
                props_json,
                base_tab_def_id,
                &self.document.doc_info.tab_defs,
            );
            let new_tab_id = self.document.find_or_create_tab_def(new_td);
            mods.tab_def_id = Some(new_tab_id);
        }

        // 테두리/배경 변경 처리
        if json_has_border_keys(props_json) {
            let bf_id = self.create_border_fill_from_json(props_json);
            mods.border_fill_id = Some(bf_id);
        }
        if let Some(arr) = parse_json_i16_array(props_json, "borderSpacing", 4) {
            mods.border_spacing = Some([arr[0], arr[1], arr[2], arr[3]]);
        }

        let new_id = self.document.find_or_create_para_shape(base_id, &mods);

        // para_shape_id 갱신
        {
            let para = self.get_hf_paragraph_mut(section_idx, is_header, apply_to, hf_para_idx)?;
            para.para_shape_id = new_id;
        }

        // 줄간격 변경 시 LineSeg 재계산
        if mods.line_spacing.is_some() || mods.line_spacing_type.is_some() {
            self.reflow_hf_paragraph(section_idx, is_header, apply_to, hf_para_idx);
        }

        self.document.sections[section_idx].raw_stream = None;
        self.rebuild_section(section_idx);
        self.event_log.push(DocumentEvent::ParaFormatChanged {
            section: section_idx,
            para: 0,
        });
        Ok("{\"ok\":true}".to_string())
    }

    /// 머리말/꼬리말 문단에 필드 마커를 삽입한다.
    /// field_type: 1=쪽번호(\u{0015}), 2=총쪽수(\u{0016}), 3=파일이름(\u{0017})
    pub fn insert_field_in_hf_native(
        &mut self,
        section_idx: usize,
        is_header: bool,
        apply_to: u8,
        hf_para_idx: usize,
        char_offset: usize,
        field_type: u8,
    ) -> Result<String, HwpError> {
        let marker = match field_type {
            1 => "\u{0015}", // 현재 쪽번호
            2 => "\u{0016}", // 총 쪽수
            3 => "\u{0017}", // 파일 이름
            _ => {
                return Err(HwpError::RenderError(format!(
                    "알 수 없는 필드 타입: {}",
                    field_type
                )))
            }
        };

        let hf_para = self.get_hf_paragraph_mut(section_idx, is_header, apply_to, hf_para_idx)?;
        hf_para.insert_text_at(char_offset, marker);

        self.reflow_hf_paragraph(section_idx, is_header, apply_to, hf_para_idx);

        self.document.sections[section_idx].raw_stream = None;
        self.mark_section_dirty(section_idx);
        self.paginate_if_needed();

        let new_offset = char_offset + 1;
        self.event_log.push(DocumentEvent::TextInserted {
            section: section_idx,
            para: 0,
            offset: char_offset,
            len: 1,
        });
        Ok(super::super::helpers::json_ok_with(&format!(
            "\"charOffset\":{}",
            new_offset
        )))
    }

    /// 머리말/꼬리말 마당(템플릿)을 적용한다.
    ///
    /// template_id:
    /// - 0: 빈 머리말/꼬리말
    /// - 1: 왼쪽 쪽번호 (기본)
    /// - 2: 가운데 쪽번호 (기본)
    /// - 3: 오른쪽 쪽번호 (기본)
    /// - 4: 쪽번호(왼)+파일이름(오) (기본)
    /// - 5: 파일이름(왼)+쪽번호(오) (기본)
    /// - 6~10: 위 1~5와 동일 배치, 볼드+밑줄 스타일
    pub fn apply_hf_template_native(
        &mut self,
        section_idx: usize,
        is_header: bool,
        apply_to: u8,
        template_id: u8,
    ) -> Result<String, HwpError> {
        if section_idx >= self.document.sections.len() {
            return Err(HwpError::RenderError(format!(
                "구역 인덱스 {} 범위 초과",
                section_idx
            )));
        }
        if template_id > 10 {
            return Err(HwpError::RenderError(format!(
                "알 수 없는 템플릿 ID: {}",
                template_id
            )));
        }

        let apply = apply_from_u8(apply_to);

        // 1) 기존 HF가 있으면 삭제
        if self
            .find_header_footer_control(section_idx, is_header, apply)
            .is_some()
        {
            self.delete_header_footer_native(section_idx, is_header, apply_to)?;
        }

        // 2) 새 HF 생성 (빈 문단 1개)
        self.create_header_footer_native(section_idx, is_header, apply_to)?;

        // 빈 템플릿이면 여기서 종료
        if template_id == 0 {
            self.mark_section_dirty(section_idx);
            self.paginate_if_needed();
            return Ok("{\"ok\":true}".to_string());
        }

        // 3) 배치/스타일 결정
        let layout = if template_id <= 5 {
            template_id
        } else {
            template_id - 5
        };
        let styled = template_id > 5; // bold + underline

        // 4) 텍스트 내용 결정
        let text = match layout {
            1 => "\u{0015}".to_string(),           // 왼쪽 쪽번호
            2 => "\u{0015}".to_string(),           // 가운데 쪽번호
            3 => "\u{0015}".to_string(),           // 오른쪽 쪽번호
            4 => "\u{0015}\t\u{0017}".to_string(), // 쪽번호(왼) + 탭 + 파일이름(오)
            5 => "\u{0017}\t\u{0015}".to_string(), // 파일이름(왼) + 탭 + 쪽번호(오)
            _ => String::new(),
        };

        // 5) 정렬 결정
        use crate::model::style::{
            Alignment, CharShapeMods, ParaShapeMods, TabDef, TabItem, UnderlineType,
        };

        let alignment = match layout {
            1 => Alignment::Left,
            2 => Alignment::Center,
            3 => Alignment::Right,
            4 | 5 => Alignment::Left, // 탭으로 오른쪽 배치
            _ => Alignment::Left,
        };

        // 6) 텍스트 삽입
        {
            let hf_para = self.get_hf_paragraph_mut(section_idx, is_header, apply_to, 0)?;
            hf_para.text = text;
            // char_offsets 재계산
            hf_para.char_offsets = hf_para
                .text
                .char_indices()
                .map(|(byte_idx, _)| byte_idx as u32)
                .collect();
        }

        // 7) 문단 정렬 적용
        let base_para_id = {
            let para = self
                .get_hf_paragraph_ref(section_idx, is_header, apply_to, 0)
                .unwrap();
            para.para_shape_id
        };
        let mut para_mods = ParaShapeMods::default();
        para_mods.alignment = Some(alignment);

        // 8) 좌+우 배치 템플릿: 오른쪽 정렬 탭 추가
        if layout == 4 || layout == 5 {
            let section = &self.document.sections[section_idx];
            let page_def = &section.section_def.page_def;
            let text_width =
                page_def.width as i32 - page_def.margin_left as i32 - page_def.margin_right as i32;

            let new_td = TabDef {
                raw_data: None,
                attr: 0,
                tabs: vec![TabItem {
                    position: text_width as u32,
                    tab_type: 1, // 오른쪽 정렬 탭
                    fill_type: 0,
                }],
                auto_tab_left: false,
                auto_tab_right: false,
            };
            let tab_id = self.document.find_or_create_tab_def(new_td);
            para_mods.tab_def_id = Some(tab_id);
        }

        let new_para_id = self
            .document
            .find_or_create_para_shape(base_para_id, &para_mods);
        {
            let hf_para = self.get_hf_paragraph_mut(section_idx, is_header, apply_to, 0)?;
            hf_para.para_shape_id = new_para_id;
        }

        // 9) 볼드+밑줄 스타일 적용
        if styled {
            let base_char_id = {
                let para = self
                    .get_hf_paragraph_ref(section_idx, is_header, apply_to, 0)
                    .unwrap();
                para.char_shapes
                    .first()
                    .map(|cs| cs.char_shape_id)
                    .unwrap_or(0)
            };
            let mut char_mods = CharShapeMods::default();
            char_mods.bold = Some(true);
            char_mods.underline_type = Some(UnderlineType::Bottom);
            let new_char_id = self
                .document
                .find_or_create_char_shape(base_char_id, &char_mods);

            let hf_para = self.get_hf_paragraph_mut(section_idx, is_header, apply_to, 0)?;
            // 전체 텍스트에 새 CharShape 적용
            for cs in &mut hf_para.char_shapes {
                cs.char_shape_id = new_char_id;
            }
        }

        // 10) 리플로우 + 스타일 재해소 + 재페이지네이션
        self.reflow_hf_paragraph(section_idx, is_header, apply_to, 0);
        self.document.sections[section_idx].raw_stream = None;
        self.rebuild_section(section_idx);

        Ok("{\"ok\":true}".to_string())
    }
}
