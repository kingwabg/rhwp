//! [어울림] 스튜디오 실경로 재현 — 한 문단 중간에서 표 생성(split)해 host 를 만든 뒤
//! 어울림(좁은 폭)으로 바꾼다. 사용자 신고: 뒤 문단이 옆으로 안 흐르고 표를 뚫는다.
use rhwp::renderer::render_tree::{RenderNode, RenderNodeType};
use rhwp::wasm_api::HwpDocument;

fn main() {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    let head = "앞 본문이다. 표 위에 있는 문단이다. 충분히 길어야 줄바꿈이 생긴다. ";
    let tail = "뒤 본문이다. 어울림이면 이 글이 표 옆으로 흘러야 한다. 길게 이어서 여러 줄이 되게 한다. 계속 이어 쓴다. 표 옆으로 흐른다. 더 길게 이어서 표 높이를 넘기게 한다. 계속 채운다.";
    let all = format!("{head}{tail}");
    doc.insert_text(0, 0, 0, &all).unwrap();
    let split_at = head.chars().count() as u32;
    let c: serde_json::Value = serde_json::from_str(&doc.create_table_ex(&format!(
        r#"{{"sectionIdx":0,"paraIdx":0,"charOffset":{split_at},"rowCount":3,"colCount":2,"treatAsChar":false}}"#
    )).unwrap()).unwrap();
    let (pi, ci) = (c["paraIdx"].as_u64().unwrap() as u32, c["controlIdx"].as_u64().unwrap() as u32);
    doc.set_table_column_widths(0, pi, ci, "[8500,8500]").unwrap();
    doc.set_table_properties(0, pi, ci,
        r#"{"treatAsChar":false,"textWrap":"Square","textFlow":"BothSides","vertRelTo":"Para","horzRelTo":"Paper","horzAlign":"Left","vertOffset":0,"horzOffset":10630}"#
    ).unwrap();

    let tree = doc.build_page_render_tree(0).unwrap();
    let mut out: Vec<(f64, f64, f64, String)> = Vec::new();
    fn walk(n: &RenderNode, out: &mut Vec<(f64, f64, f64, String)>) {
        if matches!(n.node_type, RenderNodeType::Table(_)) {
            out.push((n.bbox.y, n.bbox.x, n.bbox.width, format!("[표 h={:.0}]", n.bbox.height)));
            return;
        }
        if matches!(n.node_type, RenderNodeType::TextLine(_)) {
            let t: String = n.children.iter().filter_map(|c| match &c.node_type {
                RenderNodeType::TextRun(tr) => Some(tr.text.clone()), _ => None }).collect::<String>().replace(' ', "");
            if !t.is_empty() { out.push((n.bbox.y, n.bbox.x, n.bbox.width, t.chars().take(10).collect())); }
        }
        for c in &n.children { walk(c, out); }
    }
    walk(&tree.root, &mut out);
    out.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    println!("쪽 수: {}", doc.page_count());
    for (y, x, w, t) in &out { println!("  y={y:6.1} x={x:6.1} w={w:6.1}  {t}"); }
    // 판정: 표 bbox 와 겹치면서 표 좌측 영역까지 침범하는 줄이 있으면 겹침 결함
    if let Some((ty, tx, tw, _)) = out.iter().find(|(_, _, _, t)| t.starts_with("[표")) {
        let (ty, tx, tw) = (*ty, *tx, *tw);
        let th = 60.0;
        let overlap: Vec<_> = out.iter().filter(|(y, x, w, t)| !t.starts_with("[표")
            && *y > ty - 5.0 && *y < ty + th && *x < tx + tw && *x + *w > tx).collect();
        println!("\n표 bbox: y={ty:.1} x={tx:.1} w={tw:.1}");
        println!("표와 겹치는 줄: {}개 {:?}", overlap.len(), overlap.iter().map(|(y,x,..)| (y.round(), x.round())).collect::<Vec<_>>());
    }
}
