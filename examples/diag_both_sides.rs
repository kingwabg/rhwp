//! 진짜 양쪽 흐름 검증 — BothSides 표 좌우에 같은 y 로 글이 서는가.
use rhwp::renderer::render_tree::{RenderNode, RenderNodeType};
use rhwp::wasm_api::HwpDocument;
fn main() {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    let filler = |n: usize| format!("채움 {n:02} 가나다라마바사 아자차카타파하 강물이 흐르고 산이 높다 바다는 깊고");
    doc.insert_text(0, 0, 0, &filler(1)).unwrap();
    for n in (2..=8).rev() {
        let last = doc.get_paragraph_count(0).unwrap() - 1;
        let len = doc.get_paragraph_length(0, last).unwrap();
        doc.split_paragraph(0, last, len).unwrap();
        let last2 = doc.get_paragraph_count(0).unwrap() - 1;
        doc.insert_text(0, last2, 0, &filler(n)).unwrap();
    }
    let last = doc.get_paragraph_count(0).unwrap() - 1;
    let len = doc.get_paragraph_length(0, last).unwrap();
    let c: serde_json::Value = serde_json::from_str(&doc.create_table_ex(&format!(
        r#"{{"sectionIdx":0,"paraIdx":{last},"charOffset":{len},"rowCount":3,"colCount":1,"treatAsChar":false}}"#
    )).unwrap()).unwrap();
    let (pi, ci) = (c["paraIdx"].as_u64().unwrap() as u32, c["controlIdx"].as_u64().unwrap() as u32);
    doc.set_table_column_widths(0, pi, ci, "[12000]").unwrap();
    doc.set_table_properties(0, pi, ci,
        r#"{"treatAsChar":false,"textWrap":"Square","vertRelTo":"Para","horzRelTo":"Column","horzAlign":"Center","vertOffset":-3000,"horzOffset":0,"textFlow":"BothSides"}"#
    ).unwrap();
    let tree = doc.build_page_render_tree(0).unwrap();
    let mut tbl = (0.0, 0.0, 0.0, 0.0);
    let mut lines: Vec<(i32, i32, i32, String)> = Vec::new();
    fn walk(n: &RenderNode, tbl: &mut (f64, f64, f64, f64), out: &mut Vec<(i32, i32, i32, String)>) {
        if matches!(n.node_type, RenderNodeType::Table(_)) {
            *tbl = (n.bbox.x, n.bbox.y, n.bbox.width, n.bbox.height);
            return;
        }
        if matches!(n.node_type, RenderNodeType::TextLine(_)) {
            let txt: String = n.children.iter().filter_map(|c| match &c.node_type {
                RenderNodeType::TextRun(tr) => Some(tr.text.clone()), _ => None,
            }).collect();
            let t = txt.replace(' ', "");
            if !t.is_empty() {
                out.push((n.bbox.y as i32, n.bbox.x as i32, n.bbox.width as i32, t.chars().take(5).collect()));
            }
        }
        for c in &n.children { walk(c, tbl, out); }
    }
    walk(&tree.root, &mut tbl, &mut lines);
    lines.sort();
    println!("표: x={:.0}~{:.0} y={:.0}~{:.0}", tbl.0, tbl.0 + tbl.2, tbl.1, tbl.1 + tbl.3);
    for (y, x, w, t) in lines.iter().filter(|l| (l.0 as f64) > tbl.1 - 60.0 && (l.0 as f64) < tbl.1 + tbl.3 + 40.0) {
        println!("  y={y} x={x} w={w} {t}");
    }
}
