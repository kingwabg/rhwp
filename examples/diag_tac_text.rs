//! [신고 2026-08-05] 표를 글자처럼 취급으로 두고 오른쪽에 글자를 치면 두 글자가
//! 같은 자리에 겹쳐 그려진다. 줄 안 글자 x 좌표를 뽑아 확인한다.
use rhwp::renderer::render_tree::{RenderNode, RenderNodeType};
use rhwp::wasm_api::HwpDocument;

fn main() {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    // 문단 0 에 표(글자처럼) 하나 + 뒤에 글자
    let c: serde_json::Value = serde_json::from_str(&doc.create_table_ex(
        r#"{"sectionIdx":0,"paraIdx":0,"charOffset":0,"rowCount":3,"colCount":3,"treatAsChar":true}"#
    ).unwrap()).unwrap();
    let (pi, ci) = (c["paraIdx"].as_u64().unwrap() as u32, c["controlIdx"].as_u64().unwrap() as u32);
    doc.set_table_column_widths(0, pi, ci, "[5000,5000,5000]").unwrap();
    println!("표 문단 pi={pi} ci={ci}");
    let len0 = doc.get_paragraph_length(0, pi).unwrap();
    println!("표 삽입 후 문단 길이(UTF-16): {len0}");

    // 표 뒤에 한 글자씩 입력 — 실사용과 같은 경로
    for (i, ch) in ["하", "나", "둘"].iter().enumerate() {
        let at = doc.get_paragraph_length(0, pi).unwrap();
        doc.insert_text(0, pi, at, ch).unwrap();
        let after = doc.get_paragraph_length(0, pi).unwrap();
        println!("  {i}: '{ch}' at {at} → 길이 {after}");
    }


    // 렌더 트리에서 글자 런 x 좌표
    let tree = doc.build_page_render_tree(0).unwrap();
    fn walk(n: &RenderNode, depth: usize, in_table: bool) {
        match &n.node_type {
            RenderNodeType::Table(_) => {
                println!("{:indent$}[표] x={:.1} w={:.1}", "", n.bbox.x, n.bbox.width, indent = depth * 2);
                return; // 셀 내부는 생략
            }
            RenderNodeType::TextRun(tr) if !in_table => {
                println!("{:indent$}런 x={:.1} w={:.1} {:?}", "", n.bbox.x, n.bbox.width, tr.text, indent = depth * 2);
            }
            RenderNodeType::TextLine(_) if !in_table => {
                println!("{:indent$}줄 y={:.1} x={:.1} w={:.1}", "", n.bbox.y, n.bbox.x, n.bbox.width, indent = depth * 2);
            }
            _ => {}
        }
        for c in &n.children { walk(c, depth + 1, in_table); }
    }
    walk(&tree.root, 0, false);

    // 캐럿 x — 오프셋마다
    let len = doc.get_paragraph_length(0, pi).unwrap();
    print!("캐럿 x: ");
    for off in 0..=len {
        if let Ok(j) = doc.get_cursor_rect(0, pi, off) {
            let v: serde_json::Value = serde_json::from_str(&j).unwrap();
            print!("{}:{:.0} ", off, v["x"].as_f64().unwrap_or(-1.0));
        }
    }
    println!();
}
