//! [officex/어울림 본편] 라이브 옆 흐름 — 좁은 Square 표 옆에 본문 줄이 "좁은 폭으로
//! 재줄바꿈되어" 선다. 파이프라인: 편집 훅(reflow_paras_for_square_bands, 렌더트리
//! 2-패스)이 밴드 기준 줄바꿈을 line_segs 줄별 cs/sw 로 기록 → 렌더는 재생 소비.
use rhwp::renderer::render_tree::{RenderNode, RenderNodeType};
use rhwp::wasm_api::HwpDocument;

fn build() -> Vec<(f64, f64, f64, String)> {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    let filler = |n: usize| {
        format!("채움 {n:02} 가나다라마바사 아자차카타파하 강물이 흐르고 산이 높다 바다는 깊고 하늘은 푸르다")
    };
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
    doc.set_table_column_widths(0, pi, ci, "[15000]").unwrap();
    doc.set_table_properties(0, pi, ci,
        r#"{"treatAsChar":false,"textWrap":"Square","vertRelTo":"Para","horzRelTo":"Column","horzAlign":"Left","vertOffset":0,"horzOffset":0}"#
    ).unwrap();
    let tree = doc.build_page_render_tree(0).unwrap();
    let mut lines = Vec::new();
    fn walk(n: &RenderNode, out: &mut Vec<(f64, f64, f64, String)>) {
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
            if !t.is_empty() {
                out.push((n.bbox.y, n.bbox.x, n.bbox.width, t));
            }
        }
        for c in &n.children {
            walk(c, out);
        }
    }
    walk(&tree.root, &mut lines);
    lines
}

#[test]
fn narrow_square_table_flows_text_beside_with_rewrap() {
    let lines = build();
    // ① 옆 흐름 줄 존재: 표(x≈113~313) 오른쪽(x>300)에서 시작하는 좁은(w<420) 줄.
    let side = lines
        .iter()
        .find(|(_, x, w, t)| *x > 300.0 && *w < 420.0 && t.starts_with("채움08"));
    assert!(
        side.is_some(),
        "표 옆에 좁혀 재줄바꿈된 채움08 줄이 없다 — 옆 흐름 미작동. lines={:?}",
        lines
            .iter()
            .map(|(y, x, w, t)| (
                *y as i32,
                *x as i32,
                *w as i32,
                t.chars().take(6).collect::<String>()
            ))
            .collect::<Vec<_>>()
    );
    // ② 좁은 줄바꿈 연속성: 채움08 문단의 다음 줄이 좁은 폭에서 넘어온 내용으로 시작
    //    (전폭 줄바꿈이면 "다"로 시작 — 종전 결함의 지문).
    let follow = lines.iter().find(|(y, _, _, _)| {
        let (sy, _, _, _) = side.unwrap();
        *y > *sy + 1.0 && *y < *sy + 40.0
    });
    if let Some((_, _, _, t)) = follow {
        assert!(
            !t.starts_with("다"),
            "채움08 다음 줄이 전폭 줄바꿈 잔재({t:?}) — 재줄바꿈 미반영"
        );
    }
    // ③ 표 상자 안에서 시작하는 본문 줄 금지(자리차지·어울림 공통 계약).
    for (y, x, _, t) in &lines {
        let inside_band_y = *y + 2.0 > 431.0 && *y < 482.0;
        let inside_band_x = *x < 313.0;
        assert!(
            !(inside_band_y && inside_band_x),
            "본문 줄({t:?} y={y:.0} x={x:.0})이 표 상자 안에서 시작"
        );
    }
}
