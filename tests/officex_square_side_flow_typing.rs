//! [어울림 편집 훅 2026-08-04] 신고 재현 — 어울림 표 뒤 문단에 **글자를 입력하면**
//! 옆 흐름이 즉시 갱신돼야 한다. 종전엔 훅이 표 이동·속성 변경에만 걸려 있어,
//! 타이핑 후에는 옛 전폭 줄이 그대로 재생돼 본문이 표를 뚫고 지나갔다
//! (배치를 껐다 켜면 정상 — 계산이 아니라 갱신 누락이라는 증거).
use rhwp::renderer::render_tree::{RenderNode, RenderNodeType};
use rhwp::wasm_api::HwpDocument;

struct Line {
    y: f64,
    x: f64,
    w: f64,
}

fn collect(doc: &mut HwpDocument) -> (Option<(f64, f64, f64, f64)>, Vec<Line>) {
    let tree = doc.build_page_render_tree(0).unwrap();
    let mut table = None;
    let mut lines = Vec::new();
    fn walk(
        n: &RenderNode,
        table: &mut Option<(f64, f64, f64, f64)>,
        lines: &mut Vec<Line>,
    ) {
        if matches!(n.node_type, RenderNodeType::Table(_)) {
            if table.is_none() {
                *table = Some((n.bbox.x, n.bbox.y, n.bbox.width, n.bbox.height));
            }
            return; // 셀 안 줄은 본문이 아니다
        }
        if matches!(n.node_type, RenderNodeType::TextLine(_)) {
            let has_text = n
                .children
                .iter()
                .any(|c| matches!(&c.node_type, RenderNodeType::TextRun(tr) if !tr.text.trim().is_empty()));
            if has_text {
                lines.push(Line { y: n.bbox.y, x: n.bbox.x, w: n.bbox.width });
            }
        }
        for c in &n.children {
            walk(c, table, lines);
        }
    }
    walk(&tree.root, &mut table, &mut lines);
    (table, lines)
}

/// 본문 줄이 표 상자를 가로지르면(수직 겹침 + 수평 침범) 겹침이다.
fn overlaps(table: (f64, f64, f64, f64), lines: &[Line]) -> usize {
    let (tx, ty, tw, th) = table;
    lines
        .iter()
        .filter(|l| l.y + 4.0 > ty && l.y < ty + th - 4.0 && l.x < tx + tw - 1.0 && l.x + l.w > tx + 1.0)
        .count()
}

#[test]
fn typing_beside_square_table_reflows_immediately() {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    let head = "앞 본문이다. 표 위 문단이라 그대로 있어야 한다. ";
    doc.insert_text(0, 0, 0, head).unwrap();

    // 문단 중간에서 표 생성(스튜디오 실경로 = split) → 빈 host 문단이 생긴다
    let c: serde_json::Value = serde_json::from_str(
        &doc.create_table_ex(&format!(
            r#"{{"sectionIdx":0,"paraIdx":0,"charOffset":{},"rowCount":3,"colCount":2,"treatAsChar":false}}"#,
            head.chars().count()
        ))
        .unwrap(),
    )
    .unwrap();
    let (pi, ci) = (
        c["paraIdx"].as_u64().unwrap() as u32,
        c["controlIdx"].as_u64().unwrap() as u32,
    );
    doc.set_table_column_widths(0, pi, ci, "[8500,8500]").unwrap();
    doc.set_table_properties(0, pi, ci,
        r#"{"treatAsChar":false,"textWrap":"Square","textFlow":"BothSides","vertRelTo":"Para","horzRelTo":"Paper","horzAlign":"Left","vertOffset":0,"horzOffset":10630}"#
    ).unwrap();

    // 표 **뒤** 문단(마지막)에 사용자가 직접 타이핑하는 상황
    let last = doc.get_paragraph_count(0).unwrap() - 1;
    let tail = "뒤 본문이다. 어울림이면 이 글이 표 옆으로 흘러야 한다. 길게 이어서 여러 줄이 되게 한다. 계속 이어 쓴다.";
    doc.insert_text(0, last, 0, tail).unwrap();

    let (table, lines) = collect(&mut doc);
    let table = table.expect("표 노드가 있어야 한다");
    let bad = overlaps(table, &lines);
    assert_eq!(
        bad, 0,
        "타이핑 직후 본문 {bad}줄이 표를 뚫었다 — 편집 훅 미배선 회귀 (표={table:?})"
    );

    // 옆 흐름 줄이 실제로 존재한다(표 오른쪽에서 시작하는 좁은 줄)
    let (tx, ty, tw, th) = table;
    let side = lines
        .iter()
        .filter(|l| l.x > tx + tw - 1.0 && l.y + 4.0 > ty && l.y < ty + th)
        .count();
    assert!(side > 0, "표 옆에 선 줄이 없다 — 옆 흐름 미작동 (표={table:?})");
}

/// 지우기 경로도 같은 훅을 탄다 — 줄이 짧아져도 겹침 0.
#[test]
fn deleting_beside_square_table_reflows_immediately() {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    let head = "앞 본문. ";
    doc.insert_text(0, 0, 0, head).unwrap();
    let c: serde_json::Value = serde_json::from_str(
        &doc.create_table_ex(&format!(
            r#"{{"sectionIdx":0,"paraIdx":0,"charOffset":{},"rowCount":3,"colCount":2,"treatAsChar":false}}"#,
            head.chars().count()
        ))
        .unwrap(),
    )
    .unwrap();
    let (pi, ci) = (
        c["paraIdx"].as_u64().unwrap() as u32,
        c["controlIdx"].as_u64().unwrap() as u32,
    );
    doc.set_table_column_widths(0, pi, ci, "[8500,8500]").unwrap();
    doc.set_table_properties(0, pi, ci,
        r#"{"treatAsChar":false,"textWrap":"Square","textFlow":"BothSides","vertRelTo":"Para","horzRelTo":"Paper","horzAlign":"Left","vertOffset":0,"horzOffset":10630}"#
    ).unwrap();

    let last = doc.get_paragraph_count(0).unwrap() - 1;
    doc.insert_text(0, last, 0, "지울 본문이다. 어울림 옆에서 지우기를 한다. 길게 이어서 여러 줄이 되게 한다.").unwrap();
    doc.delete_text(0, last, 0, 12).unwrap();

    let (table, lines) = collect(&mut doc);
    let table = table.expect("표 노드가 있어야 한다");
    assert_eq!(overlaps(table, &lines), 0, "지우기 직후 본문이 표를 뚫었다");
}
