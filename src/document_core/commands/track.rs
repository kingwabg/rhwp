//! 변경 내용 추적 v1 — 스펙: mydocs/eng/plans/track-changes.md
//!
//! 편집 훅(본문 문단만): 추적 ON 삽입 = 정상 삽입 + Insert 마크,
//! 추적 ON 삭제 = 지우지 않고 Delete 마크(자기 Insert 안이면 실삭제).
//! 검토: accept(Del)/reject(Ins) = 실삭제, accept(Ins)/reject(Del) = 마크 해제.
//! 좌표: 마크는 utf16(CharShapeRef 와 같은 이동 규칙), API 는 논리 오프셋.

use super::super::DocumentCore;
use crate::error::HwpError;
use crate::model::document::{TrackChangeRec, TrackKind};
use crate::model::paragraph::{Paragraph, TrackMark};

/// 논리 오프셋(컨트롤 포함) → utf16 위치. insert/delete_text_native 가 받는 논리
/// 어법과 대칭이어야 한다 — split_text_pos_for_logical_offset 로 텍스트 문자 인덱스를
/// 얻고 char_offsets 로 utf16 에 매핑한다.
fn logical_to_utf16(para: &Paragraph, logical: usize) -> u32 {
    let text_pos = para.logical_to_text_pos(logical);
    if text_pos < para.char_offsets.len() {
        para.char_offsets[text_pos]
    } else {
        para.char_offsets.last().map_or(0, |&last| {
            let ch = para.text.chars().last().unwrap_or('\0');
            last + ch.len_utf16() as u32
        })
    }
}

/// utf16 위치 → 논리 오프셋 (역방향).
/// ⚠ 컨트롤 가산을 손으로 계산하지 않는다 — logical_to_text_pos 의 **역함수를 탐색**으로
/// 정의해 어떤 컨트롤 배치에서도 왕복이 맞게 한다(수식 역산은 secd/cold 위치에 따라
/// 어긋났다: getSelectionRects 가 빈 배열을 돌려준 실사고 2026-07-30).
fn utf16_to_logical(para: &Paragraph, utf16: u32) -> usize {
    let text_pos = para
        .char_offsets
        .iter()
        .position(|&o| o >= utf16)
        .unwrap_or(para.char_offsets.len());
    let max = crate::document_core::helpers::logical_paragraph_length(para);
    for logical in 0..=max {
        if para.logical_to_text_pos(logical) >= text_pos {
            return logical;
        }
    }
    max
}

impl DocumentCore {
    /// 추적 켜기/끄기 + 작성자·세션 날짜 설정
    pub fn set_track_changes_native(&mut self, enabled: bool, author: &str, date: &str) {
        self.track_enabled = enabled;
        self.track_author = author.to_string();
        self.track_date = date.to_string();
    }

    pub fn is_track_changes_enabled(&self) -> bool {
        self.track_enabled
    }

    fn alloc_track_id(&mut self, kind: TrackKind) -> u32 {
        let id = self.document.next_track_id.max(1);
        self.document.next_track_id = id + 1;
        self.document.track_changes.push(TrackChangeRec {
            id,
            kind,
            author: self.track_author.clone(),
            date: self.track_date.clone(),
        });
        id
    }

    fn rec_kind(&self, tc_id: u32) -> Option<TrackKind> {
        self.document
            .track_changes
            .iter()
            .find(|r| r.id == tc_id)
            .map(|r| r.kind)
    }

    /// 같은 작성자·종류의 기존 변경 id 목록
    fn own_ids(&self, kind: TrackKind) -> Vec<u32> {
        self.document
            .track_changes
            .iter()
            .filter(|r| r.kind == kind && r.author == self.track_author)
            .map(|r| r.id)
            .collect()
    }

    /// 삽입 훅 — insert_text_native 가 삽입 직후 호출한다(추적 ON 일 때만).
    pub(crate) fn track_note_insert(
        &mut self,
        section_idx: usize,
        para_idx: usize,
        logical_offset: usize,
        inserted_text: &str,
    ) {
        let ids = self.own_ids(TrackKind::Insert);
        let start = {
            let para = &self.document.sections[section_idx].paragraphs[para_idx];
            logical_to_utf16(para, logical_offset)
        };
        let need_new = {
            let para = &mut self.document.sections[section_idx].paragraphs[para_idx];
            !note_insert_on(para, &ids, start, inserted_text)
        };
        if need_new {
            let id = self.alloc_track_id(TrackKind::Insert);
            let para = &mut self.document.sections[section_idx].paragraphs[para_idx];
            note_insert_new(para, id, start, inserted_text);
        }
    }

    /// 셀 안 삽입 훅 — insert_text_in_cell_native_impl 이 삽입 직후 호출.
    /// 셀 문단 오프셋은 논리=텍스트(셀 안 인라인 컨트롤은 v2 미지원 경계).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn track_note_insert_in_cell(
        &mut self,
        section_idx: usize,
        ppi: usize,
        ci: usize,
        cei: usize,
        cpi: usize,
        char_offset: usize,
        inserted_text: &str,
    ) {
        let ids = self.own_ids(TrackKind::Insert);
        let (need_new, start) = {
            let Ok(para) = self.get_cell_paragraph_mut(section_idx, ppi, ci, cei, cpi) else {
                return;
            };
            let start = utf16_at_text_pos(para, char_offset);
            (!note_insert_on(para, &ids, start, inserted_text), start)
        };
        if need_new {
            let id = self.alloc_track_id(TrackKind::Insert);
            if let Ok(para) = self.get_cell_paragraph_mut(section_idx, ppi, ci, cei, cpi) {
                note_insert_new(para, id, start, inserted_text);
            }
        }
    }

    /// 삭제 훅 — delete_text_native 진입부가 추적 ON 일 때 호출한다.
    /// 반환 Some(json) = 여기서 처리 끝(실삭제 없음), None = 원래 삭제를 계속(자기 삽입분).
    pub(crate) fn track_delete(
        &mut self,
        section_idx: usize,
        para_idx: usize,
        char_offset: usize,
        count: usize,
    ) -> Option<String> {
        let ins_ids = self.own_ids(TrackKind::Insert);
        let del_ids = self.own_ids(TrackKind::Delete);
        let (start, end) = {
            let para = &self.document.sections[section_idx].paragraphs[para_idx];
            (
                logical_to_utf16(para, char_offset),
                logical_to_utf16(para, char_offset + count),
            )
        };
        let verdict = {
            let para = &mut self.document.sections[section_idx].paragraphs[para_idx];
            mark_delete_on(para, &ins_ids, &del_ids, start, end)
        };
        match verdict {
            DeleteVerdict::RealDelete => None,
            DeleteVerdict::Done => Some(super::super::helpers::json_ok_with("\"tracked\":true")),
            DeleteVerdict::NeedNew => {
                let id = self.alloc_track_id(TrackKind::Delete);
                let para = &mut self.document.sections[section_idx].paragraphs[para_idx];
                para.track_marks.push(TrackMark {
                    start_pos: start,
                    end_pos: end,
                    tc_id: id,
                });
                Some(super::super::helpers::json_ok_with("\"tracked\":true"))
            }
        }
    }

    /// 셀 안 삭제 훅 — delete_text_in_cell_native 진입부에서 호출.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn track_delete_in_cell(
        &mut self,
        section_idx: usize,
        ppi: usize,
        ci: usize,
        cei: usize,
        cpi: usize,
        char_offset: usize,
        count: usize,
    ) -> Option<String> {
        let ins_ids = self.own_ids(TrackKind::Insert);
        let del_ids = self.own_ids(TrackKind::Delete);
        let verdict = {
            let Ok(para) = self.get_cell_paragraph_mut(section_idx, ppi, ci, cei, cpi) else {
                return None;
            };
            let start = utf16_at_text_pos(para, char_offset);
            let end = utf16_at_text_pos(para, char_offset + count);
            mark_delete_on(para, &ins_ids, &del_ids, start, end)
        };
        match verdict {
            DeleteVerdict::RealDelete => None,
            DeleteVerdict::Done => Some(super::super::helpers::json_ok_with("\"tracked\":true")),
            DeleteVerdict::NeedNew => {
                let id = self.alloc_track_id(TrackKind::Delete);
                if let Ok(para) = self.get_cell_paragraph_mut(section_idx, ppi, ci, cei, cpi) {
                    let start = utf16_at_text_pos(para, char_offset);
                    let end = utf16_at_text_pos(para, char_offset + count);
                    para.track_marks.push(TrackMark {
                        start_pos: start,
                        end_pos: end,
                        tc_id: id,
                    });
                }
                Some(super::super::helpers::json_ok_with("\"tracked\":true"))
            }
        }
    }

    /// 범위 삭제(선택 삭제) 훅 — delete_range_native 진입부에서 호출.
    /// 한 변경 id 로 걸친 문단들에 마크만 남긴다(문단 구조는 바꾸지 않는다 — 한컴처럼
    /// 삭제 표시 상태에서도 문단이 그대로 보인다).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn track_delete_range(
        &mut self,
        section_idx: usize,
        start_para: usize,
        start_offset: usize,
        end_para: usize,
        end_offset: usize,
        cell_ctx: Option<(usize, usize, usize)>,
    ) -> Option<String> {
        let id = self.alloc_track_id(TrackKind::Delete);
        for p in start_para..=end_para {
            let (s_off, e_off) = (
                if p == start_para { start_offset } else { 0 },
                if p == end_para {
                    Some(end_offset)
                } else {
                    None
                },
            );
            let para = match cell_ctx {
                Some((ppi, ci, cei)) => {
                    match self.get_cell_paragraph_mut(section_idx, ppi, ci, cei, p) {
                        Ok(pa) => pa,
                        Err(_) => continue,
                    }
                }
                None => match self.document.sections[section_idx].paragraphs.get_mut(p) {
                    Some(pa) => pa,
                    None => continue,
                },
            };
            let start = utf16_at_text_pos(para, s_off);
            let end = match e_off {
                Some(off) => utf16_at_text_pos(para, off),
                None => utf16_at_text_pos(para, para.text.chars().count()),
            };
            if end > start {
                para.track_marks.push(TrackMark {
                    start_pos: start,
                    end_pos: end,
                    tc_id: id,
                });
            }
        }
        Some(super::super::helpers::json_ok_with("\"tracked\":true"))
    }

    /// 변경 목록 조회 — [{id,kind,author,date,section,para,start,end,text[,cell]}]
    /// (본문은 논리 오프셋, 셀 문단은 cell:{ppi,ci,cei,cpi} + 텍스트 오프셋)
    pub fn get_track_changes_native(&self) -> String {
        let mut items: Vec<String> = Vec::new();
        for (sec_idx, sec) in self.document.sections.iter().enumerate() {
            for (para_idx, host) in sec.paragraphs.iter().enumerate() {
                // 셀 문단들 — 표 컨트롤 안
                for (ctrl_idx, ctrl) in host.controls.iter().enumerate() {
                    let crate::model::control::Control::Table(t) = ctrl else {
                        continue;
                    };
                    for (cell_idx, cell) in t.cells.iter().enumerate() {
                        for (cpi, para) in cell.paragraphs.iter().enumerate() {
                            for tm in &para.track_marks {
                                let Some(rec) = self
                                    .document
                                    .track_changes
                                    .iter()
                                    .find(|r| r.id == tm.tc_id)
                                else {
                                    continue;
                                };
                                let start = utf16_to_logical(para, tm.start_pos);
                                let end = utf16_to_logical(para, tm.end_pos);
                                let text = mark_text(para, tm);
                                items.push(format!(
                                    "{{\"id\":{},\"kind\":\"{}\",\"author\":{},\"date\":{},\"section\":{},\"para\":{},\"start\":{},\"end\":{},\"text\":{},\"cell\":{{\"ppi\":{},\"ci\":{},\"cei\":{},\"cpi\":{}}}}}",
                                    rec.id,
                                    match rec.kind { TrackKind::Insert => "insert", TrackKind::Delete => "delete" },
                                    json_str(&rec.author),
                                    json_str(&rec.date),
                                    sec_idx,
                                    para_idx,
                                    start,
                                    end,
                                    json_str(&text),
                                    para_idx, ctrl_idx, cell_idx, cpi,
                                ));
                            }
                        }
                    }
                }
                let para = host;
                for tm in &para.track_marks {
                    let Some(rec) = self
                        .document
                        .track_changes
                        .iter()
                        .find(|r| r.id == tm.tc_id)
                    else {
                        continue;
                    };
                    let start = utf16_to_logical(para, tm.start_pos);
                    let end = utf16_to_logical(para, tm.end_pos);
                    let text = mark_text(para, tm);
                    items.push(format!(
                        "{{\"id\":{},\"kind\":\"{}\",\"author\":{},\"date\":{},\"section\":{},\"para\":{},\"start\":{},\"end\":{},\"text\":{}}}",
                        rec.id,
                        match rec.kind { TrackKind::Insert => "insert", TrackKind::Delete => "delete" },
                        json_str(&rec.author),
                        json_str(&rec.date),
                        sec_idx,
                        para_idx,
                        start,
                        end,
                        json_str(&text),
                    ));
                }
            }
        }
        format!("[{}]", items.join(","))
    }

    /// 적용/취소 공통 — remove_text=true 면 마크 범위를 실삭제한다
    fn resolve_track(&mut self, tc_id: u32, remove_text: bool) -> Result<(), HwpError> {
        // 마크 위치 수집 (뒤에서부터 지워야 오프셋이 안 흔들린다)
        // 본문: (sec, para, start논리, count) / 셀: + (ppi, ci, cei, cpi)
        let mut body: Vec<(usize, usize, usize, usize)> = Vec::new();
        let mut cells: Vec<(usize, usize, usize, usize, usize, usize, usize)> = Vec::new();
        for (sec_idx, sec) in self.document.sections.iter().enumerate() {
            for (host_idx, host) in sec.paragraphs.iter().enumerate() {
                for (ctrl_idx, ctrl) in host.controls.iter().enumerate() {
                    let crate::model::control::Control::Table(t) = ctrl else {
                        continue;
                    };
                    for (cell_idx, cell) in t.cells.iter().enumerate() {
                        for (cpi, para) in cell.paragraphs.iter().enumerate() {
                            for tm in &para.track_marks {
                                if tm.tc_id != tc_id {
                                    continue;
                                }
                                let start = utf16_to_logical(para, tm.start_pos);
                                let end = utf16_to_logical(para, tm.end_pos);
                                if end > start {
                                    cells.push((
                                        sec_idx,
                                        host_idx,
                                        ctrl_idx,
                                        cell_idx,
                                        cpi,
                                        start,
                                        end - start,
                                    ));
                                }
                            }
                        }
                    }
                }
                for tm in &host.track_marks {
                    if tm.tc_id != tc_id {
                        continue;
                    }
                    let start = utf16_to_logical(host, tm.start_pos);
                    let end = utf16_to_logical(host, tm.end_pos);
                    if end > start {
                        body.push((sec_idx, host_idx, start, end - start));
                    }
                }
            }
        }
        // 마크 먼저 제거 (실삭제 시 delete_text_at 의 마크 조정과 충돌하지 않게)
        for sec in &mut self.document.sections {
            for host in &mut sec.paragraphs {
                for ctrl in &mut host.controls {
                    if let crate::model::control::Control::Table(t) = ctrl {
                        for cell in &mut t.cells {
                            for para in &mut cell.paragraphs {
                                para.track_marks.retain(|tm| tm.tc_id != tc_id);
                            }
                        }
                    }
                }
                host.track_marks.retain(|tm| tm.tc_id != tc_id);
            }
        }
        if remove_text {
            // 추적 훅을 우회해 실삭제
            let saved = self.track_enabled;
            self.track_enabled = false;
            let mut result = Ok(());
            for (sec_idx, para_idx, start, count) in body.into_iter().rev() {
                if let Err(e) = self.delete_text_native(sec_idx, para_idx, start, count) {
                    result = Err(e);
                    break;
                }
            }
            if result.is_ok() {
                for (sec_idx, ppi, ci, cei, cpi, start, count) in cells.into_iter().rev() {
                    if let Err(e) =
                        self.delete_text_in_cell_native(sec_idx, ppi, ci, cei, cpi, start, count)
                    {
                        result = Err(e);
                        break;
                    }
                }
            }
            self.track_enabled = saved;
            result?;
        }
        self.document.track_changes.retain(|r| r.id != tc_id);
        Ok(())
    }

    /// 변경 적용 — Insert 는 마크 해제(글자 유지), Delete 는 실삭제
    pub fn accept_track_change_native(&mut self, tc_id: u32) -> Result<String, HwpError> {
        let kind = self
            .rec_kind(tc_id)
            .ok_or_else(|| HwpError::RenderError(format!("변경 {} 없음", tc_id)))?;
        self.resolve_track(tc_id, kind == TrackKind::Delete)?;
        Ok(super::super::helpers::json_ok())
    }

    /// 변경 취소 — Insert 는 실삭제(입력 되돌림), Delete 는 마크 해제(글자 복원)
    pub fn reject_track_change_native(&mut self, tc_id: u32) -> Result<String, HwpError> {
        let kind = self
            .rec_kind(tc_id)
            .ok_or_else(|| HwpError::RenderError(format!("변경 {} 없음", tc_id)))?;
        self.resolve_track(tc_id, kind == TrackKind::Insert)?;
        Ok(super::super::helpers::json_ok())
    }

    /// 모두 적용/취소
    pub fn resolve_all_track_changes_native(&mut self, accept: bool) -> Result<String, HwpError> {
        let ids: Vec<u32> = self.document.track_changes.iter().map(|r| r.id).collect();
        for id in ids {
            if accept {
                self.accept_track_change_native(id)?;
            } else {
                self.reject_track_change_native(id)?;
            }
        }
        Ok(super::super::helpers::json_ok())
    }
}

/// 마크 범위의 본문 텍스트
fn mark_text(para: &Paragraph, tm: &TrackMark) -> String {
    para.text
        .chars()
        .enumerate()
        .filter(|(i, _)| {
            let o = para.char_offsets.get(*i).copied().unwrap_or(u32::MAX);
            o >= tm.start_pos && o < tm.end_pos
        })
        .map(|(_, c)| c)
        .collect()
}

/// 텍스트 문자 인덱스 → utf16 (셀 문단 등 컨트롤 없는 어법용)
fn utf16_at_text_pos(para: &Paragraph, text_pos: usize) -> u32 {
    if text_pos < para.char_offsets.len() {
        para.char_offsets[text_pos]
    } else {
        para.char_offsets.last().map_or(0, |&last| {
            let ch = para.text.chars().last().unwrap_or('\0');
            last + ch.len_utf16() as u32
        })
    }
}

/// 기존 Insert 마크 확장 시도 — 성공하면 true (새 변경 불필요)
fn note_insert_on(para: &mut Paragraph, ins_ids: &[u32], start: u32, text: &str) -> bool {
    let utf16_len: u32 = text.chars().map(|c| c.len_utf16() as u32).sum();
    if utf16_len == 0 {
        return true; // 길이 0 은 할 일 없음
    }
    if let Some(tm) = para
        .track_marks
        .iter_mut()
        .find(|tm| tm.end_pos == start && ins_ids.contains(&tm.tc_id))
    {
        tm.end_pos = start + utf16_len;
        return true;
    }
    false
}

fn note_insert_new(para: &mut Paragraph, id: u32, start: u32, text: &str) {
    let utf16_len: u32 = text.chars().map(|c| c.len_utf16() as u32).sum();
    if utf16_len == 0 {
        return;
    }
    para.track_marks.push(TrackMark {
        start_pos: start,
        end_pos: start + utf16_len,
        tc_id: id,
    });
}

enum DeleteVerdict {
    /// 자기 삽입분 — 실삭제 계속
    RealDelete,
    /// 기존 Delete 마크에 병합됨 — 처리 끝
    Done,
    /// 새 Delete 변경 필요
    NeedNew,
}

fn mark_delete_on(
    para: &mut Paragraph,
    ins_ids: &[u32],
    del_ids: &[u32],
    start: u32,
    end: u32,
) -> DeleteVerdict {
    if end <= start {
        return DeleteVerdict::Done;
    }
    if para
        .track_marks
        .iter()
        .any(|tm| ins_ids.contains(&tm.tc_id) && tm.start_pos <= start && end <= tm.end_pos)
    {
        return DeleteVerdict::RealDelete;
    }
    if let Some(tm) = para
        .track_marks
        .iter_mut()
        .find(|tm| del_ids.contains(&tm.tc_id) && tm.start_pos <= end && start <= tm.end_pos)
    {
        tm.start_pos = tm.start_pos.min(start);
        tm.end_pos = tm.end_pos.max(end);
        return DeleteVerdict::Done;
    }
    DeleteVerdict::NeedNew
}

fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
