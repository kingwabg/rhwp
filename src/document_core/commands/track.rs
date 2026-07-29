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

    /// 삽입 훅 — insert_text_native 가 삽입 직후 호출한다(추적 ON 일 때만).
    /// 직전 Insert 마크의 끝과 맞닿으면 확장(타이핑 1글자 = 1변경이 되지 않게).
    pub(crate) fn track_note_insert(
        &mut self,
        section_idx: usize,
        para_idx: usize,
        logical_offset: usize,
        inserted_text: &str,
    ) {
        let utf16_len: u32 = inserted_text.chars().map(|c| c.len_utf16() as u32).sum();
        if utf16_len == 0 {
            return;
        }
        // 삽입 후의 문단에서 시작 utf16 = (논리 오프셋 위치의 utf16)
        let start = {
            let para = &self.document.sections[section_idx].paragraphs[para_idx];
            logical_to_utf16(para, logical_offset)
        };
        let end = start + utf16_len;
        let author = self.track_author.clone();
        // 인접 확장: 같은 작성자의 Insert 마크가 [.., start] 로 끝나면 늘린다.
        // (insert_text_at 의 위치 이동이 이미 지나갔으므로, 여기 마크 end 는 이동 후 값 —
        //  삽입점과 맞닿았다면 end == start 가 아니라 end == start 인 상태로 남아 있다.)
        let rec_ids: Vec<u32> = self
            .document
            .track_changes
            .iter()
            .filter(|r| r.kind == TrackKind::Insert && r.author == author)
            .map(|r| r.id)
            .collect();
        let para = &mut self.document.sections[section_idx].paragraphs[para_idx];
        if let Some(tm) = para
            .track_marks
            .iter_mut()
            .find(|tm| tm.end_pos == start && rec_ids.contains(&tm.tc_id))
        {
            tm.end_pos = end;
            return;
        }
        let id = self.alloc_track_id(TrackKind::Insert);
        let para = &mut self.document.sections[section_idx].paragraphs[para_idx];
        para.track_marks.push(TrackMark {
            start_pos: start,
            end_pos: end,
            tc_id: id,
        });
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
        let author = self.track_author.clone();
        let (start, end) = {
            let para = &self.document.sections[section_idx].paragraphs[para_idx];
            (
                logical_to_utf16(para, char_offset),
                logical_to_utf16(para, char_offset + count),
            )
        };
        if end <= start {
            return Some(super::super::helpers::json_ok_with("\"tracked\":true"));
        }
        // 자기(같은 작성자) Insert 마크 안이면 실삭제 — 마크는 delete_text_at 이 줄인다
        let own_insert = {
            let rec_ids: Vec<u32> = self
                .document
                .track_changes
                .iter()
                .filter(|r| r.kind == TrackKind::Insert && r.author == author)
                .map(|r| r.id)
                .collect();
            let para = &self.document.sections[section_idx].paragraphs[para_idx];
            para.track_marks
                .iter()
                .any(|tm| rec_ids.contains(&tm.tc_id) && tm.start_pos <= start && end <= tm.end_pos)
        };
        if own_insert {
            return None;
        }
        // Delete 마크 — 인접·겹침이면 병합(같은 작성자 Delete 만)
        let del_ids: Vec<u32> = self
            .document
            .track_changes
            .iter()
            .filter(|r| r.kind == TrackKind::Delete && r.author == author)
            .map(|r| r.id)
            .collect();
        let para = &mut self.document.sections[section_idx].paragraphs[para_idx];
        if let Some(tm) = para.track_marks.iter_mut().find(|tm| {
            del_ids.contains(&tm.tc_id) && tm.start_pos <= end && start <= tm.end_pos
        }) {
            tm.start_pos = tm.start_pos.min(start);
            tm.end_pos = tm.end_pos.max(end);
            return Some(super::super::helpers::json_ok_with("\"tracked\":true"));
        }
        let id = self.alloc_track_id(TrackKind::Delete);
        let para = &mut self.document.sections[section_idx].paragraphs[para_idx];
        para.track_marks.push(TrackMark {
            start_pos: start,
            end_pos: end,
            tc_id: id,
        });
        Some(super::super::helpers::json_ok_with("\"tracked\":true"))
    }

    /// 변경 목록 조회 — [{id,kind,author,date,section,para,start,end,text}] (논리 오프셋)
    pub fn get_track_changes_native(&self) -> String {
        let mut items: Vec<String> = Vec::new();
        for (sec_idx, sec) in self.document.sections.iter().enumerate() {
            for (para_idx, para) in sec.paragraphs.iter().enumerate() {
                for tm in &para.track_marks {
                    let Some(rec) = self.document.track_changes.iter().find(|r| r.id == tm.tc_id)
                    else {
                        continue;
                    };
                    let start = utf16_to_logical(para, tm.start_pos);
                    let end = utf16_to_logical(para, tm.end_pos);
                    let text: String = para
                        .text
                        .chars()
                        .enumerate()
                        .filter(|(i, _)| {
                            let o = para.char_offsets.get(*i).copied().unwrap_or(u32::MAX);
                            o >= tm.start_pos && o < tm.end_pos
                        })
                        .map(|(_, c)| c)
                        .collect();
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
        let mut targets: Vec<(usize, usize, usize, usize)> = Vec::new(); // sec, para, start(논리), count
        for (sec_idx, sec) in self.document.sections.iter().enumerate() {
            for (para_idx, para) in sec.paragraphs.iter().enumerate() {
                for tm in &para.track_marks {
                    if tm.tc_id != tc_id {
                        continue;
                    }
                    let start = utf16_to_logical(para, tm.start_pos);
                    let end = utf16_to_logical(para, tm.end_pos);
                    if end > start {
                        targets.push((sec_idx, para_idx, start, end - start));
                    }
                }
            }
        }
        // 마크 먼저 제거 (실삭제 시 delete_text_at 의 마크 조정과 충돌하지 않게)
        for sec in &mut self.document.sections {
            for para in &mut sec.paragraphs {
                para.track_marks.retain(|tm| tm.tc_id != tc_id);
            }
        }
        if remove_text {
            // 추적 훅을 우회해 실삭제
            let saved = self.track_enabled;
            self.track_enabled = false;
            for (sec_idx, para_idx, start, count) in targets.into_iter().rev() {
                self.delete_text_native(sec_idx, para_idx, start, count)?;
            }
            self.track_enabled = saved;
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
