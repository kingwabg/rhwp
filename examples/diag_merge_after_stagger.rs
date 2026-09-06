//! [진단 2026-09-06] 열 어긋내기(0,0 오른쪽 경계 +566) 뒤 어긋난 칸과 이웃 (0,0)-(0,2) 병합 → D1 관문 거부 재현.
use rhwp::wasm_api::HwpDocument;

fn table_of(doc: &HwpDocument) -> &rhwp::model::table::Table {
    for para in &doc.document().sections[0].paragraphs {
        for ctrl in &para.controls {
            if let rhwp::model::control::Control::Table(t) = ctrl {
                return t;
            }
        }
    }
    panic!("표 없음");
}
fn cells_vec(t: &rhwp::model::table::Table) -> String {
    t.cells.iter().map(|c| format!("({},{})rs{}cs{} w{} h{}", c.row, c.col, c.row_span, c.col_span, c.width, c.height)).collect::<Vec<_>>().join("  ")
}
fn main() {
    for (label, r0, c0, r1, c1) in [("가로 (0,0)-(0,2)", 0u16, 0u16, 0u16, 2u16), ("세로 (0,0)-(1,0)", 0, 0, 1, 0), ("전체", 0, 0, 2, 3)] {
        let mut doc = HwpDocument::create_empty();
        doc.create_blank_document().unwrap();
        let c: serde_json::Value = serde_json::from_str(&doc.create_table_ex(r#"{"sectionIdx":0,"paraIdx":0,"charOffset":0,"rowCount":3,"colCount":3,"treatAsChar":false}"#).unwrap()).unwrap();
        let (pi, ci) = (c["paraIdx"].as_u64().unwrap() as usize, c["controlIdx"].as_u64().unwrap() as usize);
        let i = table_of(&doc).cells.iter().position(|c| c.row == 0 && c.col == 0).unwrap();
        doc.offset_cell_boundary_native(0, pi, ci, i, true, 566).unwrap();
        println!("== {label}\n어긋낸 뒤 cols={:?}\n  {}", table_of(&doc).get_column_widths(), cells_vec(table_of(&doc)));
        let r = doc.merge_table_cells_native(0, pi, ci, r0, c0, r1, c1);
        println!("병합 → {:?}", r.as_ref().err());
        println!("  cols={:?} Σ={}\n  {}", table_of(&doc).get_column_widths(), table_of(&doc).get_column_widths().iter().sum::<u32>(), cells_vec(table_of(&doc)));
    }
}
