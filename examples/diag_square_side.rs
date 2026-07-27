//! 라이브 옆 흐름 검증 — 좁은 Square 표 옆으로 뒤 문단 줄이 흐르는가.
use rhwp::renderer::render_tree::{RenderNode, RenderNodeType};
use rhwp::wasm_api::HwpDocument;
fn main() {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    let filler = |n: usize| format!("채움 {n:02} 가나다라마바사 아자차카타파하 강물이 흐르고 산이 높다 바다는 깊고 하늘은 푸르다");
    doc.insert_text(0, 0, 0, &filler(14)).unwrap();
    for n in (1..14).rev() {
        doc.insert_paragraph(0, 0).unwrap();
        doc.insert_text(0, 0, 0, &filler(n)).unwrap();
    }
    let c: serde_json::Value = serde_json::from_str(&doc.create_table_ex(
        r#"{"sectionIdx":0,"paraIdx":7,"charOffset":0,"rowCount":3,"colCount":1,"treatAsChar":false}"#
    ).unwrap()).unwrap();
    let (pi, ci) = (c["paraIdx"].as_u64().unwrap() as u32, c["controlIdx"].as_u64().unwrap() as u32);
    doc.set_table_column_widths(0, pi, ci, "[15000]").unwrap();
    doc.set_table_properties(0, pi, ci,
        r#"{"treatAsChar":false,"textWrap":"Square","vertRelTo":"Para","horzRelTo":"Column","horzAlign":"Left","vertOffset":0,"horzOffset":0}"#
    ).unwrap();
    let tree = doc.build_page_render_tree(0).unwrap();
    let mut tbl = (0.0, 0.0, 0.0, 0.0);
    let mut lines: Vec<(f64, f64, f64, String)> = Vec::new();
    fn walk(n: &RenderNode, tbl: &mut (f64, f64, f64, f64), lines: &mut Vec<(f64, f64, f64, String)>) {
        if matches!(n.node_type, RenderNodeType::Table(_)) {
            *tbl = (n.bbox.x, n.bbox.y, n.bbox.width, n.bbox.height);
            return;
        }
        if matches!(n.node_type, RenderNodeType::TextLine(_)) {
            let txt: String = n.children.iter().filter_map(|c| {
                if let RenderNodeType::TextRun(tr) = &c.node_type { Some(tr.text.clone()) } else { None }
            }).collect();
            let t = txt.replace(' ', "");
            if !t.is_empty() {
                lines.push((n.bbox.y, n.bbox.x, n.bbox.width, t.chars().take(6).collect()));
            }
        }
        for c in &n.children { walk(c, tbl, lines); }
    }
    walk(&tree.root, &mut tbl, &mut lines);
    lines.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    println!("표: x={:.0} y={:.0} w={:.0} h={:.0}", tbl.0, tbl.1, tbl.2, tbl.3);
    for (y, x, w, t) in lines.iter().filter(|l| l.0 > 380.0 && l.0 < 560.0) {
        println!("  y={y:.0} x={x:.0} w={w:.0} {t}");
    }
}
