//! [편집] 자리차지 표 아래로 이동 — 정적 조판 vs move_table_offset 동적 경로 비교.
use rhwp::wasm_api::HwpDocument;

fn build(voff_at_create: i32, move_after: i32) -> Vec<(usize, i32)> {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    let filler = |n: usize| format!("채움 {n:02} 가나다라마바사 아자차카타파하 강물이 흐르고 산이 높다");
    doc.insert_text(0, 0, 0, &filler(14)).unwrap();
    for n in (1..14).rev() {
        doc.insert_paragraph(0, 0).unwrap();
        doc.insert_text(0, 0, 0, &filler(n)).unwrap();
    }
    let c: serde_json::Value = serde_json::from_str(&doc.create_table_ex(
        r#"{"sectionIdx":0,"paraIdx":7,"charOffset":0,"rowCount":3,"colCount":2,"treatAsChar":false}"#
    ).unwrap()).unwrap();
    let (pi, ci) = (c["paraIdx"].as_u64().unwrap() as u32, c["controlIdx"].as_u64().unwrap() as u32);
    doc.set_table_properties(0, pi, ci, &format!(
        r#"{{"treatAsChar":false,"textWrap":"TopAndBottom","vertRelTo":"Para","horzRelTo":"Column","vertOffset":{voff_at_create}}}"#
    )).unwrap();
    if move_after != 0 {
        doc.move_table_offset(0, pi, ci, 0, move_after).unwrap();
    }
    // 렌더트리에서 문단 첫 줄 y + 표 y
    let tree = doc.build_page_render_tree(0).unwrap();
    let mut out: Vec<(usize, i32)> = Vec::new();
    fn walk(n: &rhwp::renderer::render_tree::RenderNode, out: &mut Vec<(usize, i32)>) {
        if let rhwp::renderer::render_tree::RenderNodeType::TextLine(tl) = &n.node_type {
            if tl.line_index == Some(0) {
                if let Some(p) = tl.para_index { out.push((p, n.bbox.y as i32)); }
            }
        }
        if matches!(n.node_type, rhwp::renderer::render_tree::RenderNodeType::Table(_)) {
            out.push((usize::MAX, n.bbox.y as i32));
        }
        for c in &n.children { walk(c, out); }
    }
    walk(&tree.root, &mut out);
    out.sort();
    out
}

fn main() {
    let stat = build(4800, 0);
    let dynm = build(0, 4800);
    println!("{:>6} {:>8} {:>8}", "para", "정적voff", "move후");
    let keys: std::collections::BTreeSet<usize> = stat.iter().chain(&dynm).map(|x| x.0).collect();
    for k in keys {
        let s = stat.iter().find(|x| x.0 == k).map(|x| x.1);
        let d = dynm.iter().find(|x| x.0 == k).map(|x| x.1);
        let name = if k == usize::MAX { "표".to_string() } else { format!("p{k}") };
        println!("{name:>6} {:>8} {:>8} {}", s.map_or("-".into(), |v| v.to_string()), d.map_or("-".into(), |v| v.to_string()), if s != d { "←다름" } else { "" });
    }
}
