//! 테두리 회귀 재현 — applyDefault 상당(setCellProperties 4셀) 후 setColumnWidths.
use rhwp::wasm_api::HwpDocument;
fn main() {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    doc.insert_text(0, 0, 0, "본문입니다").unwrap();
    let c: serde_json::Value = serde_json::from_str(&doc.create_table_ex(
        r#"{"sectionIdx":0,"paraIdx":0,"charOffset":5,"rowCount":2,"colCount":2,"treatAsChar":false}"#
    ).unwrap()).unwrap();
    let (pi, ci) = (c["paraIdx"].as_u64().unwrap() as u32, c["controlIdx"].as_u64().unwrap() as u32);
    let border = r##"{"borderLeft":{"type":1,"width":4,"color":"#000000"},"borderRight":{"type":1,"width":4,"color":"#000000"},"borderTop":{"type":1,"width":4,"color":"#000000"},"borderBottom":{"type":1,"width":4,"color":"#000000"}}"##;
    for cell in 0..4u32 {
        doc.set_cell_properties(0, pi, ci, cell, border).unwrap();
    }
    let strokes = |doc: &HwpDocument, tag: &str| {
        let svg = doc.render_page_svg(0).unwrap();
        let mut cnt = std::collections::BTreeMap::new();
        for m in svg.match_indices("stroke-width=\"") {
            let rest = &svg[m.0 + 14..];
            let end = rest.find('"').unwrap();
            *cnt.entry(rest[..end].to_string()).or_insert(0) += 1;
        }
        eprintln!("{tag} strokes={cnt:?}");
    };
    strokes(&doc, "apply후");
    doc.set_table_column_widths(0, pi, ci, "[15000,15000]").unwrap();
    strokes(&doc, "colW후");
}
