//! [어울림 수렴 2026-07-30] 어울림(Square) 표 이동 시 옆 흐름 재줄바꿈의 방향별 회귀.
//!
//! 사용자 신고: "표를 어울림으로 하고 텍스트 쪽으로 이동하면 글이 표를 피해 흐르던 동작이
//! 없어졌다". 실측: 좁힘 결정이 이동 직후 렌더 위치 기준이라, 좁힌 결과가 문단을 표 옆으로
//! 되돌리면 결정과 최종 배치가 어긋났다(특히 위로 끌어 앞 문단과 겹치는 케이스 — 좁힘이
//! 엉뚱한 줄에 붙고 밴드 안 줄은 전폭). 수리 = reflow_paras_for_square_bands 고정점 반복.
use rhwp::renderer::render_tree::{RenderNode, RenderNodeType};
use rhwp::wasm_api::HwpDocument;

fn doc_with_five_paras() -> HwpDocument {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    let filler = "가나다라마바사 아자차카타파하 강물이 흐르고 산이 높다 바람이 분다";
    let flen = filler.chars().count();
    doc.insert_text(0, 0, 0, filler).unwrap();
    for i in 0..4u32 {
        doc.split_paragraph_native(0, i as usize, flen).unwrap();
        doc.insert_text(0, i + 1, 0, filler).unwrap();
    }
    doc
}

fn partial_square_table(doc: &mut HwpDocument, para_idx: usize) -> (u32, u32) {
    let c: serde_json::Value = serde_json::from_str(&doc.create_table_ex(&format!(
        r#"{{"sectionIdx":0,"paraIdx":{para_idx},"charOffset":0,"rowCount":2,"colCount":2,"treatAsChar":false,"colWidths":[3000,3000]}}"#
    )).unwrap()).unwrap();
    let (pi, ci) = (c["paraIdx"].as_u64().unwrap() as u32, c["controlIdx"].as_u64().unwrap() as u32);
    doc.delete_table_column(0, pi, ci, 1).unwrap();
    doc.set_table_properties(0, pi, ci,
        r#"{"treatAsChar":false,"textWrap":"Square","vertRelTo":"Para","horzRelTo":"Column","vertOffset":0}"#
    ).unwrap();
    (pi, ci)
}

/// 본문 TextLine(표 내부 제외)이 표 상자와 겹치는 수를 센다.
fn overlap_count(doc: &HwpDocument, pi: u32, ci: u32) -> usize {
    let bb: serde_json::Value =
        serde_json::from_str(&doc.get_table_bbox(0, pi, ci).unwrap()).unwrap();
    let (tx, ty, tw, th) = (
        bb["x"].as_f64().unwrap(), bb["y"].as_f64().unwrap(),
        bb["width"].as_f64().unwrap(), bb["height"].as_f64().unwrap(),
    );
    let tree = doc.build_page_render_tree(0).unwrap();
    fn walk(n: &RenderNode, in_table: bool, out: &mut Vec<(f64, f64, f64, f64)>) {
        if matches!(n.node_type, RenderNodeType::Table(_)) {
            for c in &n.children { walk(c, true, out); }
            return;
        }
        if let RenderNodeType::TextLine(_) = &n.node_type {
            if !in_table {
                out.push((n.bbox.x, n.bbox.y, n.bbox.width, n.bbox.height));
            }
        }
        for c in &n.children { walk(c, in_table, out); }
    }
    let mut lines = Vec::new();
    walk(&tree.root, false, &mut lines);
    lines.iter().filter(|(x, y, w, h)| {
        x + 0.0 < tx + tw - 1.0 && x + w > tx + 1.0 && *y < ty + th - 1.0 && y + h > ty + 1.0
    }).count()
}

/// 표를 아래로 끌어 뒤 문단들과 겹치는 케이스 — 뒤 문단이 표 옆으로 갈라진다.
#[test]
fn square_down_move_flows_following_text_beside() {
    let mut doc = doc_with_five_paras();
    doc.split_paragraph_native(0, 2, 33).unwrap();
    let (pi, ci) = partial_square_table(&mut doc, 3);
    doc.move_table_offset(0, pi, ci, 6000, 3000).unwrap();
    assert_eq!(overlap_count(&doc, pi, ci), 0, "본문 줄이 표 상자와 겹침");
}

/// 표를 위로 끌어 앞 문단들과 겹치는 케이스 — 수렴 반복 전엔 좁힘이 엉뚱한 줄에 붙었다.
#[test]
fn square_up_move_flows_preceding_text_beside() {
    let mut doc = doc_with_five_paras();
    doc.split_paragraph_native(0, 4, 33).unwrap();
    let pc = doc.get_paragraph_count(0).unwrap();
    let (pi, ci) = partial_square_table(&mut doc, (pc - 1) as usize);
    doc.move_table_offset(0, pi, ci, 6000, -6000).unwrap();
    assert_eq!(overlap_count(&doc, pi, ci), 0, "본문 줄이 표 상자와 겹침");
}

/// [2026-07-30 사용자 결정] 글자처럼취급 표의 드래그 이동은 한컴에 없는 기능 — 완전 무동작.
/// (종전: v_offset 누적 + 문단 경계에서 paragraphs.swap 으로 "문단 사이 이동"을 흉내냈다.)
#[test]
fn tac_table_move_is_noop() {
    let mut doc = doc_with_five_paras();
    let c: serde_json::Value = serde_json::from_str(&doc.create_table_ex(
        r#"{"sectionIdx":0,"paraIdx":2,"charOffset":0,"rowCount":1,"colCount":1,"treatAsChar":true,"colWidths":[3000]}"#
    ).unwrap()).unwrap();
    let (pi, ci) = (c["paraIdx"].as_u64().unwrap() as u32, c["controlIdx"].as_u64().unwrap() as u32);
    let para_text = |doc: &HwpDocument, p: u32| -> String {
        let len = doc.get_paragraph_length(0, p).unwrap();
        if len == 0 { return String::new(); }
        doc.get_text_range(0, p, 0, len).unwrap()
    };
    let pc = doc.get_paragraph_count(0).unwrap();
    let text_before: Vec<String> = (0..pc).map(|p| para_text(&doc, p)).collect();

    let r: serde_json::Value = serde_json::from_str(
        &doc.move_table_offset(0, pi, ci, 0, 99000).unwrap()).unwrap();
    assert_eq!(r["ppi"].as_u64(), Some(pi as u64), "TAC 이동이 문단을 바꿈");

    let text_after: Vec<String> = (0..pc).map(|p| para_text(&doc, p)).collect();
    assert_eq!(text_before, text_after, "TAC 이동이 문단 순서를 교환함 — 무동작이어야 한다");

    let tp: serde_json::Value = serde_json::from_str(
        &doc.get_table_properties(0, pi, ci).unwrap()).unwrap();
    assert_eq!(tp["vertOffset"].as_i64(), Some(0), "TAC 이동이 오프셋을 누적함");
}

/// [실사고 2026-07-30] studio 배치 UX 는 가로 기준을 **종이(Paper)** 로 저장한다 —
/// 종전 필터(Column|Para)가 이 조합을 걸러 어울림 rewrap 이 통째로 죽었다.
#[test]
fn square_move_flows_with_paper_horz_rel() {
    let mut doc = doc_with_five_paras();
    doc.split_paragraph_native(0, 2, 33).unwrap();
    let c: serde_json::Value = serde_json::from_str(&doc.create_table_ex(
        r#"{"sectionIdx":0,"paraIdx":3,"charOffset":0,"rowCount":2,"colCount":2,"treatAsChar":false,"colWidths":[3000,3000]}"#
    ).unwrap()).unwrap();
    let (pi, ci) = (c["paraIdx"].as_u64().unwrap() as u32, c["controlIdx"].as_u64().unwrap() as u32);
    doc.delete_table_column(0, pi, ci, 1).unwrap();
    doc.set_table_properties(0, pi, ci,
        r#"{"treatAsChar":false,"textWrap":"Square","vertRelTo":"Para","horzRelTo":"Paper","horzAlign":"Left","horzOffset":8504,"vertOffset":0}"#
    ).unwrap();
    doc.move_table_offset(0, pi, ci, 0, 3000).unwrap();
    assert_eq!(overlap_count(&doc, pi, ci), 0, "Paper 기준 어울림 표가 rewrap 에서 빠짐");
}
