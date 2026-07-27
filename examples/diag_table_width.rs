//! 표 폭 축소 경로 실측 — createTableEx colWidths / resizeTableCells 델타 / setTableColumnWidths.
use rhwp::wasm_api::HwpDocument;
fn dump(doc: &HwpDocument, pi: u32, ci: u32, tag: &str) {
    let p: serde_json::Value = serde_json::from_str(&doc.get_table_properties(0, pi, ci).unwrap()).unwrap();
    let b: serde_json::Value = serde_json::from_str(&doc.get_table_bbox(0, pi, ci).unwrap()).unwrap();
    let c0: serde_json::Value = serde_json::from_str(&doc.get_cell_properties(0, pi, ci, 0).unwrap()).unwrap();
    let c1: serde_json::Value = serde_json::from_str(&doc.get_cell_properties(0, pi, ci, 1).unwrap()).unwrap();
    println!("{tag:28} tableWidth={:6} bboxW={:6.1} cell0={:6} cell1={:6}", p["tableWidth"], b["width"].as_f64().unwrap(), c0["width"], c1["width"]);
}
fn main() {
    // A: createTableEx colWidths
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    doc.insert_text(0, 0, 0, "본문").unwrap();
    let c: serde_json::Value = serde_json::from_str(&doc.create_table_ex(r#"{"sectionIdx":0,"paraIdx":0,"charOffset":2,"rowCount":2,"colCount":2,"colWidths":[10000,10000]}"#).unwrap()).unwrap();
    let (pi, ci) = (c["paraIdx"].as_u64().unwrap() as u32, c["controlIdx"].as_u64().unwrap() as u32);
    dump(&doc, pi, ci, "A createTableEx colWidths");
    // B: resizeTableCells 델타
    let _ = doc.resize_table_cells(0, pi, ci, r#"[{"cellIdx":0,"widthDelta":-8000},{"cellIdx":1,"widthDelta":-8000}]"#);
    dump(&doc, pi, ci, "B resizeTableCells -8000x2");
    // C: setTableColumnWidths
    let r = doc.set_table_column_widths(0, pi, ci, "[10000,10000]");
    println!("  setTableColumnWidths resp={:?}", r.map(|x| x.chars().take(60).collect::<String>()));
    dump(&doc, pi, ci, "C setTableColumnWidths");
}
