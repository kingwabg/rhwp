//! 변경 추적 사이드카 (Contents/officexTrack.json) — 스펙: mydocs/eng/plans/track-changes.md
//!
//! HWPX zip 에 우리만의 JSON 한 장을 실어 추적 상태를 왕복시킨다.
//! 형식: {"v":1,"changes":[{id,kind,author,date}],"marks":[{sec,para,start,end,tc}]}
//! ⚠ 마크 위치는 **텍스트 문자 인덱스**로 저장한다 — utf16 스트림 위치는 재파싱 시
//! 문단 앞 인라인 컨트롤(SecDef 8유닛 등)만큼 밀려 어긋난다(실측 2026-07-30: 0→16).
//! 텍스트 문자 인덱스는 같은 본문이면 왕복에 불변이다.

use crate::model::document::{Document, TrackChangeRec, TrackKind};
use crate::model::paragraph::TrackMark;

fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// 추적 상태가 비어 있으면 None (사이드카 미기록 — 파일을 더럽히지 않는다)
pub fn build_track_sidecar(doc: &Document) -> Option<String> {
    let has_marks = doc
        .sections
        .iter()
        .any(|s| s.paragraphs.iter().any(|p| !p.track_marks.is_empty()));
    if doc.track_changes.is_empty() && !has_marks {
        return None;
    }
    let changes: Vec<String> = doc
        .track_changes
        .iter()
        .map(|r| {
            format!(
                "{{\"id\":{},\"kind\":\"{}\",\"author\":\"{}\",\"date\":\"{}\"}}",
                r.id,
                match r.kind {
                    TrackKind::Insert => "insert",
                    TrackKind::Delete => "delete",
                },
                esc(&r.author),
                esc(&r.date),
            )
        })
        .collect();
    let mut marks: Vec<String> = Vec::new();
    let push_mark = |marks: &mut Vec<String>,
                     para: &crate::model::paragraph::Paragraph,
                     tm: &TrackMark,
                     sec_idx: usize,
                     para_idx: usize,
                     cell: Option<(usize, usize, usize)>| {
        let start = para
            .char_offsets
            .iter()
            .position(|&o| o >= tm.start_pos)
            .unwrap_or(para.char_offsets.len());
        let end = para
            .char_offsets
            .iter()
            .position(|&o| o >= tm.end_pos)
            .unwrap_or(para.char_offsets.len());
        let cell_json = cell
            .map(|(ci, cei, cpi)| format!(",\"ci\":{},\"cei\":{},\"cpi\":{}", ci, cei, cpi))
            .unwrap_or_default();
        marks.push(format!(
            "{{\"sec\":{},\"para\":{},\"start\":{},\"end\":{},\"tc\":{}{}}}",
            sec_idx, para_idx, start, end, tm.tc_id, cell_json
        ));
    };
    for (sec_idx, sec) in doc.sections.iter().enumerate() {
        for (para_idx, host) in sec.paragraphs.iter().enumerate() {
            for (ctrl_idx, ctrl) in host.controls.iter().enumerate() {
                let crate::model::control::Control::Table(t) = ctrl else {
                    continue;
                };
                for (cell_idx, cell) in t.cells.iter().enumerate() {
                    for (cpi, para) in cell.paragraphs.iter().enumerate() {
                        for tm in &para.track_marks {
                            push_mark(
                                &mut marks,
                                para,
                                tm,
                                sec_idx,
                                para_idx,
                                Some((ctrl_idx, cell_idx, cpi)),
                            );
                        }
                    }
                }
            }
            let para = host;
            for tm in &para.track_marks {
                push_mark(&mut marks, para, tm, sec_idx, para_idx, None);
            }
        }
    }
    Some(format!(
        "{{\"v\":1,\"next\":{},\"changes\":[{}],\"marks\":[{}]}}",
        doc.next_track_id,
        changes.join(","),
        marks.join(",")
    ))
}

/// 파싱 복원 — serde 없이 최소 파서(우리가 쓴 형식만 읽는다)
pub fn restore_track_sidecar(doc: &mut Document, json: &str) {
    // changes
    let mut max_id = 0u32;
    if let Some(arr) = extract_array(json, "changes") {
        for obj in split_objects(&arr) {
            let (Some(id), Some(kind), Some(author), Some(date)) = (
                num_field(&obj, "id"),
                str_field(&obj, "kind"),
                str_field(&obj, "author"),
                str_field(&obj, "date"),
            ) else {
                continue;
            };
            let kind = if kind == "delete" {
                TrackKind::Delete
            } else {
                TrackKind::Insert
            };
            max_id = max_id.max(id);
            doc.track_changes.push(TrackChangeRec {
                id,
                kind,
                author,
                date,
            });
        }
    }
    if let Some(arr) = extract_array(json, "marks") {
        for obj in split_objects(&arr) {
            let (Some(sec), Some(para), Some(start), Some(end), Some(tc)) = (
                num_field(&obj, "sec"),
                num_field(&obj, "para"),
                num_field(&obj, "start"),
                num_field(&obj, "end"),
                num_field(&obj, "tc"),
            ) else {
                continue;
            };
            let (sec, para) = (sec as usize, para as usize);
            // 셀 마크: ci/cei/cpi 가 있으면 표 셀 문단으로 복원
            if let (Some(ci), Some(cei), Some(cpi)) = (
                num_field(&obj, "ci"),
                num_field(&obj, "cei"),
                num_field(&obj, "cpi"),
            ) {
                let target = doc
                    .sections
                    .get_mut(sec)
                    .and_then(|s| s.paragraphs.get_mut(para))
                    .and_then(|host| host.controls.get_mut(ci as usize))
                    .and_then(|ctrl| match ctrl {
                        crate::model::control::Control::Table(t) => t.cells.get_mut(cei as usize),
                        _ => None,
                    })
                    .and_then(|cell| cell.paragraphs.get_mut(cpi as usize));
                if let Some(p) = target {
                    let at = |idx: u32| -> u32 {
                        let idx = idx as usize;
                        if idx < p.char_offsets.len() {
                            p.char_offsets[idx]
                        } else {
                            p.char_offsets.last().map_or(0, |&last| {
                                let ch = p.text.chars().last().unwrap_or('\0');
                                last + ch.len_utf16() as u32
                            })
                        }
                    };
                    p.track_marks.push(TrackMark {
                        start_pos: at(start),
                        end_pos: at(end),
                        tc_id: tc,
                    });
                }
                continue;
            }
            if let Some(p) = doc
                .sections
                .get_mut(sec)
                .and_then(|s| s.paragraphs.get_mut(para))
            {
                // 텍스트 문자 인덱스 → 이 문서의 utf16 위치
                let at = |idx: u32| -> u32 {
                    let idx = idx as usize;
                    if idx < p.char_offsets.len() {
                        p.char_offsets[idx]
                    } else {
                        p.char_offsets.last().map_or(0, |&last| {
                            let ch = p.text.chars().last().unwrap_or('\0');
                            last + ch.len_utf16() as u32
                        })
                    }
                };
                p.track_marks.push(TrackMark {
                    start_pos: at(start),
                    end_pos: at(end),
                    tc_id: tc,
                });
            }
        }
    }
    doc.next_track_id = doc.next_track_id.max(max_id + 1);
}

fn extract_array(json: &str, key: &str) -> Option<String> {
    let pat = format!("\"{}\":[", key);
    let i = json.find(&pat)? + pat.len();
    let mut depth = 1i32;
    let bytes = json.as_bytes();
    let mut j = i;
    let mut in_str = false;
    let mut prev = 0u8;
    while j < bytes.len() {
        let b = bytes[j];
        if in_str {
            if b == b'"' && prev != b'\\' {
                in_str = false;
            }
        } else {
            match b {
                b'"' => in_str = true,
                b'[' => depth += 1,
                b']' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(json[i..j].to_string());
                    }
                }
                _ => {}
            }
        }
        prev = b;
        j += 1;
    }
    None
}

fn split_objects(arr: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = arr.as_bytes();
    let mut depth = 0i32;
    let mut start = None;
    let mut in_str = false;
    let mut prev = 0u8;
    for (i, &b) in bytes.iter().enumerate() {
        if in_str {
            if b == b'"' && prev != b'\\' {
                in_str = false;
            }
        } else {
            match b {
                b'"' => in_str = true,
                b'{' => {
                    if depth == 0 {
                        start = Some(i);
                    }
                    depth += 1;
                }
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        if let Some(s) = start {
                            out.push(arr[s..=i].to_string());
                        }
                    }
                }
                _ => {}
            }
        }
        prev = b;
    }
    out
}

fn num_field(obj: &str, key: &str) -> Option<u32> {
    let pat = format!("\"{}\":", key);
    let i = obj.find(&pat)? + pat.len();
    let rest = &obj[i..];
    let end = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    rest[..end].parse().ok()
}

fn str_field(obj: &str, key: &str) -> Option<String> {
    let pat = format!("\"{}\":\"", key);
    let i = obj.find(&pat)? + pat.len();
    let rest = &obj[i..];
    let mut out = String::new();
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(out),
            '\\' => {
                if let Some(n) = chars.next() {
                    match n {
                        'n' => out.push('\n'),
                        _ => out.push(n),
                    }
                }
            }
            c => out.push(c),
        }
    }
    None
}
