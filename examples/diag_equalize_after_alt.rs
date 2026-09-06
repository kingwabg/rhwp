//! [2026-09-06] Alt+↓ 보상(−d) 뒤 "높이 같게"(표시 기준 델타)가 한 행을 빠뜨리던 재현 — 저장 높이 유령 구간(pad<h<바닥) 스냅 확인.
use rhwp::model::control::Control;
use rhwp::wasm_api::HwpDocument;
fn main() {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    let c: serde_json::Value = serde_json::from_str(&doc.create_table_ex(
        r#"{"sectionIdx":0,"paraIdx":0,"charOffset":0,"rowCount":3,"colCount":3,"treatAsChar":false}"#).unwrap()).unwrap();
    let (pi, ci) = (
        c["paraIdx"].as_u64().unwrap() as usize,
        c["controlIdx"].as_u64().unwrap() as usize,
    );
    let show = |doc: &HwpDocument, tag: &str| {
        let Control::Table(t) = &doc.document().sections[0].paragraphs[pi].controls[ci] else {
            panic!()
        };
        println!(
            "{tag}: stored={:?} eff={:?}",
            t.cells.iter().map(|c| c.height).collect::<Vec<_>>(),
            t.effective_row_heights()
        );
    };
    show(&doc, "load");
    // 스튜디오 Alt+↓×3 (행 0 +849, 행 1 −849 보상) 을 셀별 델타로
    doc.resize_table_cells(0, pi as u32, ci as u32, r#"[{"cellIdx":0,"heightDelta":849},{"cellIdx":1,"heightDelta":849},{"cellIdx":2,"heightDelta":849},{"cellIdx":3,"heightDelta":-849},{"cellIdx":4,"heightDelta":-849},{"cellIdx":5,"heightDelta":-849}]"#).unwrap();
    show(&doc, "after Alt+↓×3 (row0 +849, row1 −849)");
    // 높이 같게(3열): 표시 높이 2133/1284/1284 → 평균 1567 → 델타 −566/+283/+283
    doc.resize_table_cells(0, pi as u32, ci as u32, r#"[{"cellIdx":2,"heightDelta":-566},{"cellIdx":5,"heightDelta":283},{"cellIdx":8,"heightDelta":283}]"#).unwrap();
    show(&doc, "after equalize col2 (want cells 5,8 → 1567)");
}
