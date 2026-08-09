//! [officex/어울림 배선 3/3 스크래치] probe-flow.mjs 의 네이티브 재현 —
//! 분류(visible float 여부)·PageItem 순서·y 흐름을 진단 env 와 함께 실측한다.
//! 사용: RHWP_DEBUG_TAC_CURSOR=1 RHWP_DIAG_TAC=1 cargo run --example diag_wrap3

use rhwp::wasm_api::HwpDocument;

fn snap(doc: &mut HwpDocument, label: &str, para_idx: u32, ctrl_idx: u32) {
    let bbox = doc
        .get_table_bbox(0, para_idx, ctrl_idx)
        .expect("getTableBBox");
    let front = doc.get_selection_rects(0, 0, 5, 0, 15).expect("rects");
    let mid = doc
        .get_selection_rects(0, 0, 170, 0, 180)
        .expect("rects_mid");
    let back = doc.get_selection_rects(0, 0, 380, 0, 390).expect("rects2");
    println!("{label} bbox={bbox} front={front} mid={mid} back={back}");
}

fn main() {
    let mm = 283.46_f64;
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().expect("blank");
    let body = "가나다라마바사아자차카타파하 ".repeat(30);
    doc.insert_text(0, 0, 0, &body).expect("insert");
    let created = doc.create_table(0, 0, 200, 3, 3).expect("createTable");
    println!("created={created}");
    let v: serde_json::Value = serde_json::from_str(&created).expect("json");
    let pi = v["paraIdx"].as_u64().expect("paraIdx") as u32;
    let ci = v["controlIdx"].as_u64().expect("controlIdx") as u32;
    // probe-flow.mjs 와 동일 속성
    doc.set_table_properties(
        0,
        pi,
        ci,
        r#"{"treatAsChar":false,"textWrap":"TopAndBottom","vertRelTo":"Para","restrictInPage":false,"vertOffset":0}"#,
    )
    .expect("setProps");
    snap(&mut doc, "초기        ", pi, ci);
    doc.move_table_offset(0, pi, ci, 0, (25.0 * mm).round() as i32)
        .expect("move+25");
    snap(&mut doc, "아래로 25mm ", pi, ci);
    doc.move_table_offset(0, pi, ci, 0, (-50.0 * mm).round() as i32)
        .expect("move-50");
    snap(&mut doc, "위로 -25mm  ", pi, ci);
    doc.move_table_offset(0, pi, ci, 0, (15.0 * mm).round() as i32)
        .expect("move+15");
    snap(&mut doc, "위로 -10mm  ", pi, ci); // 밴드가 문단 중간 줄에 걸림 — 줄 단위 회피 본편
}
