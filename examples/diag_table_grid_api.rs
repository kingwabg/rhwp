//! [격자 12-b 2026-09-03] getTableGrid / getBoundaryMoveRange JSON 계약 확인 — 3×3 → 어긋내기 후.
use rhwp::wasm_api::HwpDocument;
fn main() {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    let c: serde_json::Value = serde_json::from_str(&doc.create_table_ex(
        r#"{"sectionIdx":0,"paraIdx":0,"charOffset":0,"rowCount":3,"colCount":3,"treatAsChar":false}"#).unwrap()).unwrap();
    let (pi, ci) = (c["paraIdx"].as_u64().unwrap() as u32, c["controlIdx"].as_u64().unwrap() as u32);
    println!("range cell0 right = {}", doc.get_boundary_move_range(0, pi, ci, 0, "right").unwrap());
    println!("range cell0 bottom = {}", doc.get_boundary_move_range(0, pi, ci, 0, "bottom").unwrap());
    doc.offset_cell_boundary_native(0, pi as usize, ci as usize, 0, true, 1000).unwrap();
    let g: serde_json::Value = serde_json::from_str(&doc.get_table_grid(0, pi, ci).unwrap()).unwrap();
    println!("after stagger cell0 right +1000:");
    println!("  colX={} rowYEff={} rowCount={} colCount={} minCell={}", g["colX"], g["rowYEff"], g["rowCount"], g["colCount"], g["minCell"]);
    println!("  colLines={}", g["colLines"]);
    println!("  cellGrid={}", g["cellGrid"]);
    println!("  rowColX={}", g["rowColX"]);
}
