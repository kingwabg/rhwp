//! [어울림] 핀 테스트와 동일 구성에서 textFlow(본문 위치)만 바꿔 옆 흐름 유지 여부 확인.
//! 스튜디오 어울림 스위치는 기본 「양쪽(BothSides)」로 켠다 — 핀은 textFlow 미지정.
use rhwp::renderer::render_tree::{RenderNode, RenderNodeType};
use rhwp::wasm_api::HwpDocument;

fn run(flow: Option<&str>) {
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
    let flow_kv = flow.map(|f| format!(r#""textFlow":"{f}","#)).unwrap_or_default();
    let props = format!(
        r#"{{"treatAsChar":false,"textWrap":"Square",{flow_kv}"vertRelTo":"Para","horzRelTo":"Column","horzAlign":"Left","vertOffset":0,"horzOffset":0}}"#);
    doc.set_table_properties(0, pi, ci, &props).unwrap();

    let tree = doc.build_page_render_tree(0).unwrap();
    let mut lines = Vec::new();
    fn walk(n: &RenderNode, out: &mut Vec<(f64, f64, f64, String)>) {
        if matches!(n.node_type, RenderNodeType::Table(_)) {
            out.push((n.bbox.y, n.bbox.x, n.bbox.width, "[표]".into()));
            return;
        }
        if matches!(n.node_type, RenderNodeType::TextLine(_)) {
            let t: String = n.children.iter().filter_map(|c| match &c.node_type {
                RenderNodeType::TextRun(tr) => Some(tr.text.clone()), _ => None }).collect::<String>().replace(' ', "");
            if !t.is_empty() { out.push((n.bbox.y, n.bbox.x, n.bbox.width, t.chars().take(8).collect())); }
        }
        for c in &n.children { walk(c, out); }
    }
    walk(&tree.root, &mut lines);
    lines.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    println!("\n=== textFlow = {:?} ===", flow.unwrap_or("(미지정)"));
    let side: Vec<_> = lines.iter().filter(|(_, x, w, t)| *x > 300.0 && *w < 420.0 && t != "[표]").collect();
    println!("  옆 흐름 줄 수: {}", side.len());
    for (y, x, w, t) in lines.iter().filter(|(y, ..)| *y > 200.0 && *y < 330.0) {
        println!("   y={y:6.1} x={x:6.1} w={w:6.1}  {t}");
    }
}

fn main() {
    run(None);
    run(Some("BothSides"));
    run(Some("Left"));
    run(Some("Right"));
}
