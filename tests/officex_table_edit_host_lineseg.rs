//! [officex] 표 기하 변형 후 host LINE_SEG 진실성 핀 — qa:rhwp 앱 통합 워크플로 결함의 계약.
//!
//! 표에 행을 넣으면 host 문단의 저장 lh 도 따라 커져야 한다. 종전엔 표만 커지고 host
//! lh 가 옛 값으로 박제돼, 저장→재열기 시 typeset 이 저장 lh 를 믿고(#2319 보정은
//! lineseg 부재 문단만 구제) 넘친 표를 1쪽에 가뒀다(일지 양식 42행 실측: lh=3600 vs
//! 표 91600HU). 수리 = 기하 변형 15곳에서 refresh_table_host_line_segs(host reflow).

use rhwp::model::control::Control;
use rhwp::wasm_api::HwpDocument;

#[test]
fn table_row_insert_updates_host_lineseg_and_repaginates_after_reopen() {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().expect("blank");
    doc.insert_text(0, 0, 0, "정원표").expect("text");
    let created = doc.create_table(0, 0, 3, 2, 3).expect("createTable");
    let v: serde_json::Value = serde_json::from_str(&created).expect("json");
    let pi = v["paraIdx"].as_u64().unwrap() as u32;
    let ci = v["controlIdx"].as_u64().unwrap() as u32;
    doc.set_table_properties(0, pi, ci, r#"{"treatAsChar":true}"#)
        .expect("tac");
    for _ in 0..90 {
        doc.insert_table_row(0, pi, ci, 1, true).expect("row");
    }

    let bytes = doc.export_hwp().expect("export");
    let model = rhwp::parse_document(&bytes).expect("reparse");
    let para = &model.sections[0].paragraphs[pi as usize];
    let table_h = para
        .controls
        .iter()
        .find_map(|c| match c {
            Control::Table(t) => Some(t.common.height as i64),
            _ => None,
        })
        .expect("table");
    let max_seg_lh = para
        .line_segs
        .iter()
        .map(|s| s.line_height as i64)
        .max()
        .unwrap_or(0);

    // 계약 1: 저장 lh 가 표 높이를 반영한다(박제 금지). outer margin 오차 허용.
    assert!(
        max_seg_lh >= table_h,
        "host 저장 lh {max_seg_lh} 가 표 높이 {table_h} 를 반영하지 못함 (박제된 옛 값)"
    );
    // 계약 2: 재열기해도 쪽이 갈라진다 — 1쪽 고착 금지.
    let reopened = HwpDocument::from_bytes(&bytes).expect("reopen");
    assert!(
        reopened.page_count() >= 2,
        "쪽을 넘는 표가 재열기 후 {}쪽 — 재조판이 저장 lh 를 믿고 고착",
        reopened.page_count()
    );
}
