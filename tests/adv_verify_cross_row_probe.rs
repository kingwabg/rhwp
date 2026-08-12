//! 적대 검증용 일회성 probe — 커밋 금지. row1 어긋내기 후 row0 같은 경계 어긋내기 거부 여부.
use rhwp::wasm_api::HwpDocument;

fn table_cells(doc: &HwpDocument) -> Vec<(u16, u16, u16, u32)> {
    for para in &doc.document().sections[0].paragraphs {
        for ctrl in &para.controls {
            if let rhwp::model::control::Control::Table(t) = ctrl {
                return t
                    .cells
                    .iter()
                    .map(|c| (c.row, c.col, c.col_span, c.width))
                    .collect();
            }
        }
    }
    panic!("표 없음");
}

fn idx_of(doc: &HwpDocument, row: u16, col: u16) -> usize {
    for para in &doc.document().sections[0].paragraphs {
        for ctrl in &para.controls {
            if let rhwp::model::control::Control::Table(t) = ctrl {
                return t
                    .cells
                    .iter()
                    .position(|c| c.row == row && c.col == col)
                    .unwrap();
            }
        }
    }
    panic!("표 없음");
}

#[test]
fn probe_cross_row_same_boundary() {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    let c: serde_json::Value = serde_json::from_str(
        &doc.create_table_ex(
            r#"{"sectionIdx":0,"paraIdx":0,"colCount":3,"rowCount":3,"charOffset":0,"treatAsChar":false}"#,
        )
        .unwrap(),
    )
    .unwrap();
    let (pi, ci) = (
        c["paraIdx"].as_u64().unwrap() as usize,
        c["controlIdx"].as_u64().unwrap() as usize,
    );

    // row1 (1,0) 오른쪽 경계를 오른쪽으로 어긋내기
    let i = idx_of(&doc, 1, 0);
    let r1 = doc.offset_cell_boundary_native(0, pi, ci, i, true, 1000);
    println!("row1 offset: {r1:?}");
    println!("cells after row1 offset: {:?}", table_cells(&doc));

    // row0 (0,0) 같은 경계를 오른쪽으로 어긋내기 — 주장: 거부됨
    let i = idx_of(&doc, 0, 0);
    let r2 = doc.offset_cell_boundary_native(0, pi, ci, i, true, 1000);
    println!("row0 offset same boundary: {r2:?}");

    // row0 왼쪽 방향(delta<0)도 확인
    let i = idx_of(&doc, 0, 0);
    let r3 = doc.offset_cell_boundary_native(0, pi, ci, i, true, -800);
    println!("row0 offset same boundary leftward: {r3:?}");
}
