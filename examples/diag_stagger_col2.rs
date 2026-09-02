//! [진단 2026-08-17] col0×2 + col1×4 어긋내기 후 col2 큰 델타 드래그가
//! +566(첫 어긋선)에서 멈추는 문제 — 각 스텝의 격자·결과를 찍는다.
use rhwp::wasm_api::HwpDocument;

fn dump(doc: &HwpDocument, tag: &str) {
    for para in &doc.document().sections[0].paragraphs {
        for ctrl in &para.controls {
            if let rhwp::model::control::Control::Table(t) = ctrl {
                let eff = t.effective_row_heights();
                let cells: Vec<String> = t
                    .cells
                    .iter()
                    .map(|c| format!("({},{})s{}h{}", c.row, c.col, c.row_span, c.height))
                    .collect();
                println!(
                    "{tag}: eff={eff:?} sum={} cells={}",
                    eff.iter().sum::<u32>(),
                    cells.join(" ")
                );
                return;
            }
        }
    }
}

fn pos_idx(doc: &HwpDocument, row: u16, col: u16) -> usize {
    for para in &doc.document().sections[0].paragraphs {
        for ctrl in &para.controls {
            if let rhwp::model::control::Control::Table(t) = ctrl {
                return t
                    .cells
                    .iter()
                    .position(|c| c.row == row && c.col == col)
                    .unwrap_or_else(|| panic!("({row},{col}) 없음"));
            }
        }
    }
    panic!("표 없음");
}

fn main() {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    let c: serde_json::Value = serde_json::from_str(
        &doc.create_table_ex(
            r#"{"sectionIdx":0,"paraIdx":0,"charOffset":0,"rowCount":3,"colCount":3,"treatAsChar":false}"#,
        )
        .unwrap(),
    )
    .unwrap();
    let (pi, ci) = (
        c["paraIdx"].as_u64().unwrap() as usize,
        c["controlIdx"].as_u64().unwrap() as usize,
    );
    dump(&doc, "기준");
    for k in 0..2 {
        let i = pos_idx(&doc, 0, 0);
        let r = doc.offset_cell_boundary_native(0, pi, ci, i, false, 283);
        println!("col0 스텝{} → {:?}", k + 1, r.err());
    }
    dump(&doc, "col0×2");
    for k in 0..4 {
        let i = pos_idx(&doc, 0, 1);
        let r = doc.offset_cell_boundary_native(0, pi, ci, i, false, 283);
        println!("col1 스텝{} → {:?}", k + 1, r.err());
        dump(&doc, &format!("col1 스텝{}", k + 1));
    }
    let i = pos_idx(&doc, 0, 2);
    let r = doc.offset_cell_boundary_native(0, pi, ci, i, false, 2250);
    println!("col2 +2250 → {:?}", r.err());
    dump(&doc, "col2 후");
}
