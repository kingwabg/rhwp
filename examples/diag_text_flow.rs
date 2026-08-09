//! 본문위치(textFlow) 검증 — 왼쪽/오른쪽/큰쪽에 따라 글이 어느 쪽에 서는가.
//! 표를 단 가운데 두어 좌우 공간을 비슷하게 만들고 flow 별 줄 x 를 본다.
use rhwp::renderer::render_tree::{RenderNode, RenderNodeType};
use rhwp::wasm_api::HwpDocument;

fn probe(flow: &str) -> Vec<(i32, i32, String)> {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    let filler = |n: usize| format!("채움 {n:02} 가나다라마바사 아자차카타파하 강물이 흐르고");
    doc.insert_text(0, 0, 0, &filler(14)).unwrap();
    for n in (1..14).rev() {
        doc.insert_paragraph(0, 0).unwrap();
        doc.insert_text(0, 0, 0, &filler(n)).unwrap();
    }
    let c: serde_json::Value = serde_json::from_str(&doc.create_table_ex(
        r#"{"sectionIdx":0,"paraIdx":7,"charOffset":0,"rowCount":3,"colCount":1,"treatAsChar":false}"#
    ).unwrap()).unwrap();
    let (pi, ci) = (
        c["paraIdx"].as_u64().unwrap() as u32,
        c["controlIdx"].as_u64().unwrap() as u32,
    );
    doc.set_table_column_widths(0, pi, ci, "[12000]").unwrap();
    // 가운데 배치(좌우 공간 균형) + flow 지정
    doc.set_table_properties(0, pi, ci, &format!(
        r#"{{"treatAsChar":false,"textWrap":"Square","vertRelTo":"Para","horzRelTo":"Column","horzAlign":"Center","vertOffset":0,"horzOffset":0,"textFlow":"{flow}"}}"#
    )).unwrap();
    let tree = doc.build_page_render_tree(0).unwrap();
    let mut out = Vec::new();
    fn walk(n: &RenderNode, out: &mut Vec<(i32, i32, String)>) {
        if matches!(n.node_type, RenderNodeType::Table(_)) {
            return;
        }
        if matches!(n.node_type, RenderNodeType::TextLine(_)) {
            let txt: String = n
                .children
                .iter()
                .filter_map(|c| match &c.node_type {
                    RenderNodeType::TextRun(tr) => Some(tr.text.clone()),
                    _ => None,
                })
                .collect();
            let t = txt.replace(' ', "");
            if t.starts_with("채움08") || t.starts_with("채움09") {
                out.push((
                    n.bbox.x as i32,
                    n.bbox.width as i32,
                    t.chars().take(6).collect(),
                ));
            }
        }
        for c in &n.children {
            walk(c, out);
        }
    }
    walk(&tree.root, &mut out);
    out
}

fn main() {
    for flow in ["LeftOnly", "RightOnly", "LargestOnly", "BothSides"] {
        let r = probe(flow);
        println!("{flow:12} {:?}", r.iter().take(2).collect::<Vec<_>>());
    }
}
