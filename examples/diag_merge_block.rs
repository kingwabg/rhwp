//! [5단계 2026-09-02] 2×2 병합이 표 높이를 바꾸는 경로 확인(D2 위반 c-merge-block).
use rhwp::model::control::Control;
use rhwp::wasm_api::HwpDocument;

fn dump(doc: &HwpDocument, pi: usize, ci: usize, tag: &str) {
    let Control::Table(t) = &doc.document().sections[0].paragraphs[pi].controls[ci] else {
        panic!()
    };
    println!(
        "--- {tag}: common={}x{} raw={:?} rows={:?} eff={:?}",
        t.common.width,
        t.common.height,
        t.get_raw_row_heights(),
        t.get_row_heights(),
        t.effective_row_heights()
    );
    for (i, c) in t.cells.iter().enumerate() {
        println!(
            "  #{i} ({},{}) span {}x{} h={} w={} paras={}",
            c.row,
            c.col,
            c.row_span,
            c.col_span,
            c.height,
            c.width,
            c.paragraphs.len()
        );
    }
}
fn main() {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    let c: serde_json::Value = serde_json::from_str(&doc.create_table_ex(
        r#"{"sectionIdx":0,"paraIdx":0,"charOffset":0,"rowCount":3,"colCount":3,"treatAsChar":false}"#).unwrap()).unwrap();
    let (pi, ci) = (
        c["paraIdx"].as_u64().unwrap() as usize,
        c["controlIdx"].as_u64().unwrap() as usize,
    );
    dump(&doc, pi, ci, "before");
    doc.merge_table_cells_native(0, pi, ci, 0, 0, 1, 1).unwrap();
    dump(&doc, pi, ci, "after merge (0,0)-(1,1)");
}
