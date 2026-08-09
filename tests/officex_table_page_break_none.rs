//! [officex] 표 나눔 = "나누지 않음"(TablePageBreak::None) 계약 핀.
//!
//! 계약: 남은 공간에 안 들어가도 **한 쪽에는 들어가는** 표는 쪽 경계에서 자르지 않고
//! 통째로 다음 쪽으로 옮긴다. 종전 엔진은 page_break 를 분할 판정에 전혀 쓰지 않아
//! 0/1/2 가 동일하게 분할됐다(2026-07-27 실측).
//!
//! ⚠ 범위 제한 2가지 — 둘 다 실측 근거가 있다:
//!  · 제목 줄 반복(repeat_header) 표는 제외 — 반복 머리행은 분할을 전제한 설정이고,
//!    task1725 문서(한컴 PDF 오라클 242쪽)에서 그런 표를 통째 이동시키면 243쪽으로
//!    어긋난다(issue_1733 핀이 이를 지킨다).
//!  · 글자처럼취급(tac) 표는 제외 — 인라인 원자성 규칙이 따로 있다(#991, §3.5).

use rhwp::wasm_api::HwpDocument;

/// (쪽 수, 표가 걸친 쪽 수) — 걸친 쪽 수 1 이면 분할되지 않은 것이다.
fn layout(page_break: u8, repeat_header: bool) -> (u32, usize) {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().expect("blank");
    // 앞 본문으로 현재 쪽 잔여 공간을 좁힌다 — 표가 남은 공간엔 못 들어가게.
    let filler = "가나다라마바사아자차카타파하 ".repeat(3);
    for _ in 0..20 {
        let len = doc.get_paragraph_length(0, 0).unwrap_or(0);
        doc.insert_text(0, 0, len, &format!("{filler}\n"))
            .expect("text");
    }
    let len = doc.get_paragraph_length(0, 0).unwrap_or(0);
    let created = doc.create_table(0, 0, len, 3, 3).expect("table");
    let v: serde_json::Value = serde_json::from_str(&created).unwrap();
    let (pi, ci) = (
        v["paraIdx"].as_u64().unwrap() as u32,
        v["controlIdx"].as_u64().unwrap() as u32,
    );
    doc.set_table_properties(0, pi, ci, &format!(
        r#"{{"treatAsChar":false,"textWrap":"TopAndBottom","vertRelTo":"Para","pageBreak":{page_break},"repeatHeader":{repeat_header}}}"#
    )).expect("props");
    // 40행 = 한 쪽에는 들어가지만 남은 공간에는 못 들어가는 크기.
    for _ in 0..40 {
        doc.insert_table_row(0, pi, ci, 1, true).expect("row");
    }
    let boxes: serde_json::Value =
        serde_json::from_str(&doc.get_table_cell_bboxes(0, pi, ci, None).expect("bb")).unwrap();
    let mut pages: Vec<i64> = boxes
        .as_array()
        .map(|a| a.iter().filter_map(|b| b["pageIndex"].as_i64()).collect())
        .unwrap_or_default();
    pages.sort_unstable();
    pages.dedup();
    (doc.page_count(), pages.len())
}

#[test]
fn page_break_none_moves_whole_table_instead_of_splitting() {
    let (_, spread) = layout(0, false);
    assert_eq!(
        spread, 1,
        "쪽나눔=나누지 않음(0) 인데 표가 {spread}개 쪽에 걸쳐 분할됨"
    );
}

#[test]
fn page_break_rowbreak_still_splits() {
    let (_, spread) = layout(2, false);
    assert_eq!(
        spread, 2,
        "쪽나눔=나눔(2) 인데 분할되지 않음(걸친 쪽 {spread}) — 이동 규칙이 과하게 적용됨"
    );
}

#[test]
fn repeat_header_table_keeps_splitting_even_when_none() {
    // 제목 줄 반복은 분할 전제 — 이동 대상에서 제외된다(한컴 오라클 근거, 위 문서 주석).
    let (_, spread) = layout(0, true);
    assert_eq!(
        spread, 2,
        "제목 줄 반복 표까지 통째 이동시킴(걸친 쪽 {spread}) — issue_1733 오라클과 어긋난다"
    );
}
