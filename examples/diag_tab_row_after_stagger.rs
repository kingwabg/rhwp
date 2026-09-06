//! [진단 2026-09-06] 첫 행 열 경계를 왼쪽으로 어긋낸 뒤 마지막 셀 Tab(행 추가) → 새 행 셀 수.
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
    t.cells
        .iter()
        .map(|c| format!("({},{})rs{}cs{} w{} h{}", c.row, c.col, c.row_span, c.col_span, c.width, c.height))
        .collect::<Vec<_>>()
        .join("  ")
}

fn run(rows: u16, delta: i32) {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    let c: serde_json::Value = serde_json::from_str(
        &doc.create_table_ex(&format!(
            r#"{{"sectionIdx":0,"paraIdx":0,"charOffset":0,"rowCount":{rows},"colCount":3,"treatAsChar":false}}"#
        ))
        .unwrap(),
    )
    .unwrap();
    let (pi, ci) = (c["paraIdx"].as_u64().unwrap() as usize, c["controlIdx"].as_u64().unwrap() as usize);
    println!("== {rows}x3, 첫 행 (0,0) 오른쪽 경계 {delta:+}HU");
    println!("기준   cols={:?}\n       {}", table_of(&doc).get_column_widths(), cells_vec(table_of(&doc)));
    let i = table_of(&doc).cells.iter().position(|c| c.row == 0 && c.col == 0).unwrap();
    let r = doc.offset_cell_boundary_native(0, pi, ci, i, true, delta);
    println!("어긋내기 → {:?}", r.as_ref().err());
    println!("       cols={:?}\n       {}", table_of(&doc).get_column_widths(), cells_vec(table_of(&doc)));
    let last = table_of(&doc).row_count - 1;
    let r = doc.insert_table_row_native(0, pi, ci, last, true);
    println!("Tab 행 추가(row {last} 아래) → {:?}", r.as_ref().err());
    let t = table_of(&doc);
    let new_row = t.row_count - 1;
    let n = t.cells.iter().filter(|c| c.row == new_row).count();
    println!("       cols={:?}  새 행 셀 수={n}\n       {}", t.get_column_widths(), cells_vec(t));
}

fn main() {
    run(2, -975);
    run(1, -975);
    run(3, -975);
}
