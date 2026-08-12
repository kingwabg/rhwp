//! 감사용 일회성 probe — 커밋 금지. 이중 어긋내기 후 restore 폭 복원 검증.
use rhwp::wasm_api::HwpDocument;

fn table_state(doc: &HwpDocument) -> (u32, Vec<u32>, u16, Vec<(u16, u16, u16, u32)>) {
    for para in &doc.document().sections[0].paragraphs {
        for ctrl in &para.controls {
            if let rhwp::model::control::Control::Table(t) = ctrl {
                let cells: Vec<_> = t
                    .cells
                    .iter()
                    .map(|c| (c.row, c.col, c.col_span, c.width))
                    .collect();
                return (t.common.width, t.get_column_widths(), t.col_count, cells);
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
fn probe_double_offset_then_restore() {
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
    let (w0, cols0, cc0, cells0) = table_state(&doc);
    println!("start: w={w0} cols={cols0:?} cc={cc0}\n  {cells0:?}");

    let i = idx_of(&doc, 1, 0);
    let r1 = doc.offset_cell_boundary_native(0, pi, ci, i, true, 1000);
    let (w1, cols1, cc1, cells1) = table_state(&doc);
    println!("offset1({r1:?}): w={w1} cols={cols1:?} cc={cc1}\n  {cells1:?}");

    let i = idx_of(&doc, 1, 0);
    let r2 = doc.offset_cell_boundary_native(0, pi, ci, i, true, 800);
    let (w2, cols2, cc2, cells2) = table_state(&doc);
    println!("offset2({r2:?}): w={w2} cols={cols2:?} cc={cc2}\n  {cells2:?}");

    let i = idx_of(&doc, 1, 0);
    let r3 = doc.restore_cell_boundary_native(0, pi, ci, i, true);
    let (w3, cols3, cc3, cells3) = table_state(&doc);
    println!("restore1({r3:?}): w={w3} cols={cols3:?} cc={cc3}\n  {cells3:?}");

    let i = idx_of(&doc, 1, 0);
    let r4 = doc.restore_cell_boundary_native(0, pi, ci, i, true);
    let (w4, cols4, cc4, cells4) = table_state(&doc);
    println!("restore2({r4:?}): w={w4} cols={cols4:?} cc={cc4}\n  {cells4:?}");
}
