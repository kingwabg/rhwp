//! [어울림 실측] 사용자 시나리오 재현 — 본문 → 표(Square, 가로=종이/왼쪽 30mm) → 본문.
//! 핀 테스트(가로=단/Column)는 통과하는데 실문서는 겹친다는 신고의 원인 좁히기.
use rhwp::renderer::render_tree::{RenderNode, RenderNodeType};
use rhwp::wasm_api::HwpDocument;

fn lines_of(doc: &mut HwpDocument) -> Vec<(f64, f64, f64, String)> {
    let tree = doc.build_page_render_tree(0).unwrap();
    let mut out = Vec::new();
    fn walk(n: &RenderNode, out: &mut Vec<(f64, f64, f64, String)>) {
        if matches!(n.node_type, RenderNodeType::Table(_)) {
            if let RenderNodeType::Table(_) = n.node_type {
                out.push((n.bbox.y, n.bbox.x, n.bbox.width, format!("[표 h={:.0}]", n.bbox.height)));
            }
            return;
        }
        if matches!(n.node_type, RenderNodeType::TextLine(_)) {
            let txt: String = n.children.iter().filter_map(|c| match &c.node_type {
                RenderNodeType::TextRun(tr) => Some(tr.text.clone()), _ => None }).collect();
            let t = txt.replace(' ', "");
            if !t.is_empty() { out.push((n.bbox.y, n.bbox.x, n.bbox.width, t.chars().take(12).collect())); }
        }
        for c in &n.children { walk(c, out); }
    }
    walk(&tree.root, &mut out);
    out.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    out
}

fn run(horz_rel: &str, horz_off: i32, flow: &str, label: &str) {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    // 문단 순서: 0=앞 본문, 1=뒤 본문 (insert_paragraph(0,0) 은 앞에 새 문단을 만든다)
    doc.insert_text(0, 0, 0, "뒤 본문이다. 어울림이면 이 글이 표 옆으로 흘러야 한다. 길게 이어서 여러 줄이 되게 한다. 계속 이어 쓴다. 표 옆으로 흐른다. 더 길게 이어서 표 높이를 넘기게 한다.").unwrap();
    doc.insert_paragraph(0, 0).unwrap();
    doc.insert_text(0, 0, 0, "앞 본문이다. 표 위에 있는 문단이다. 충분히 길어야 줄바꿈이 생긴다.").unwrap();
    println!("문단 수: {}", doc.get_paragraph_count(0).unwrap());
    // 뒤 본문(para 1) 앞에 표를 앵커
    let c: serde_json::Value = serde_json::from_str(&doc.create_table_ex(
        r#"{"sectionIdx":0,"paraIdx":1,"charOffset":0,"rowCount":3,"colCount":2,"treatAsChar":false}"#
    ).unwrap()).unwrap();
    let (pi, ci) = (c["paraIdx"].as_u64().unwrap() as u32, c["controlIdx"].as_u64().unwrap() as u32);
    doc.set_table_column_widths(0, pi, ci, "[8500,8500]").unwrap();
    let props = format!(
        r#"{{"treatAsChar":false,"textWrap":"Square","textFlow":"{flow}","width":17000,"vertRelTo":"Para","horzRelTo":"{horz_rel}","horzAlign":"Left","vertOffset":0,"horzOffset":{horz_off}}}"#
    );
    doc.set_table_properties(0, pi, ci, &props).unwrap();
    println!("\n=== {label} (horzRelTo={horz_rel}, off={horz_off}) ===");
    println!("  쪽 수: {}, 문단 수: {}", doc.page_count(), doc.get_paragraph_count(0).unwrap());
    for (y, x, w, t) in lines_of(&mut doc) {
        println!("  y={y:6.1} x={x:6.1} w={w:6.1}  {t}");
    }
}

fn main() {
    run("Column", 0, "BothSides", "핀 조건 + 폭 지정 + 양쪽");
    run("Paper", 10630, "BothSides", "실문서 조건(종이/왼쪽 30mm, 양쪽)");
    run("Paper", 10630, "Left", "실문서 조건 + 본문 위치 왼쪽");
}
