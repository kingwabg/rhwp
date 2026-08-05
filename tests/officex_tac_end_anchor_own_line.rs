//! [tac-inline-baseline-report-20260805] end-anchored 글자취급 표는 자기 줄로.
//!
//! 한컴 오라클(웹한글 실측): 앞에만 텍스트가 있는 TAC 표는 앞 텍스트 옆이 아니라
//! **다음 줄**에 놓인다. 종전 rhwp 는 가로로 붙이며 표를 문단 상단(텍스트보다 46px
//! 위)에 그려 "커서가 글자를 뒤덮는" 세로 어긋남을 만들었다.
use rhwp::renderer::render_tree::{RenderNode, RenderNodeType};
use rhwp::wasm_api::HwpDocument;

fn make_doc_with_end_anchor_table() -> HwpDocument {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    doc.insert_text(0, 0, 0, "왼쪽").unwrap();
    let c: serde_json::Value = serde_json::from_str(
        &doc.create_table_ex(
            r#"{"sectionIdx":0,"paraIdx":0,"charOffset":2,"rowCount":2,"colCount":2,"treatAsChar":true}"#,
        )
        .unwrap(),
    )
    .unwrap();
    let (pi, ci) = (
        c["paraIdx"].as_u64().unwrap() as u32,
        c["controlIdx"].as_u64().unwrap() as u32,
    );
    doc.set_table_column_widths(0, pi, ci, "[7087,7087]").unwrap();
    doc
}

/// 렌더 트리에서 (표 bbox, 본문 첫 TextRun bbox)를 얻는다. 셀 안은 본문이 아니다.
fn table_and_text_bbox(doc: &mut HwpDocument) -> ((f64, f64, f64, f64), (f64, f64, f64, f64)) {
    let tree = doc.build_page_render_tree(0).unwrap();
    let mut tb: Option<(f64, f64, f64, f64)> = None;
    let mut xb: Option<(f64, f64, f64, f64)> = None;
    fn walk(
        n: &RenderNode,
        tb: &mut Option<(f64, f64, f64, f64)>,
        xb: &mut Option<(f64, f64, f64, f64)>,
    ) {
        if matches!(n.node_type, RenderNodeType::Table(_)) {
            if tb.is_none() {
                *tb = Some((n.bbox.x, n.bbox.y, n.bbox.width, n.bbox.height));
            }
            return;
        }
        if let RenderNodeType::TextRun(tr) = &n.node_type {
            if !tr.text.trim().is_empty() && xb.is_none() {
                *xb = Some((n.bbox.x, n.bbox.y, n.bbox.width, n.bbox.height));
            }
        }
        for c in &n.children {
            walk(c, tb, xb);
        }
    }
    walk(&tree.root, &mut tb, &mut xb);
    (tb.expect("표 노드"), xb.expect("본문 TextRun"))
}

/// 표는 앞 텍스트의 **다음 줄**에 — 세로로 텍스트 아래, 가로로 줄 시작.
#[test]
fn end_anchor_table_sits_on_its_own_line_below_text() {
    let mut doc = make_doc_with_end_anchor_table();
    let ((_tx, ty, _tw, _th), (_xx, xy, _xw, xh)) = table_and_text_bbox(&mut doc);
    assert!(
        ty >= xy + xh - 1.0,
        "표가 앞 텍스트 줄 아래에 서야 한다(한컴 오라클): 표 top={ty} 텍스트 bottom={}",
        xy + xh
    );
}

/// line_seg 가 2개(텍스트 줄 + 표 줄)로 분리되어야 한다 — 저장/렌더 공용 진실.
#[test]
fn end_anchor_table_gets_its_own_line_seg() {
    let doc = make_doc_with_end_anchor_table();
    let segs: serde_json::Value =
        serde_json::from_str(&doc.debug_line_seg_tags(0, 0).unwrap()).unwrap();
    let n = segs.as_array().unwrap().len();
    assert_eq!(n, 2, "line_seg 2개(텍스트 줄+표 줄)여야 한다: {segs}");
}

/// 앞 텍스트 줄은 양쪽정렬로 벌어지지 않는다 — 뒤따르는 줄이 표의 빈 줄뿐이면
/// 사실상 마지막 텍스트 줄(한컴: "왼쪽"이 자연 폭 그대로).
#[test]
fn text_line_before_table_is_not_justified() {
    let mut doc = make_doc_with_end_anchor_table();
    let (_, (_xx, _xy, xw, _xh)) = table_and_text_bbox(&mut doc);
    assert!(
        xw < 100.0,
        "두 글자 텍스트가 줄 폭으로 벌어졌다(양쪽정렬 오적용): width={xw}"
    );
    let r1: serde_json::Value =
        serde_json::from_str(&doc.get_cursor_rect(0, 0, 1).unwrap()).unwrap();
    let x1 = r1["x"].as_f64().unwrap();
    assert!(
        x1 < 150.0,
        "글자 사이 캐럿이 줄 중앙으로 밀렸다(자간 벌림 오적용): x={x1}"
    );
}

/// 표 앞/뒤 캐럿은 텍스트 줄 높이 — 줄 전체(표 높이)를 덮는 블록 커서가 아니다.
#[test]
fn caret_beside_text_keeps_text_line_height() {
    let mut doc = make_doc_with_end_anchor_table();
    let ((_, _, _, th), (_, _, _, xh)) = table_and_text_bbox(&mut doc);
    let r0: serde_json::Value =
        serde_json::from_str(&doc.get_cursor_rect(0, 0, 0).unwrap()).unwrap();
    let h0 = r0["height"].as_f64().unwrap();
    assert!(
        h0 < th * 0.8,
        "텍스트 줄 캐럿(h={h0})이 표 높이({th})급 블록 커서면 안 된다 (텍스트 h={xh})"
    );
}

/// 표 오른쪽 빈 공간 클릭 → 표 뒤 오프셋(3).
#[test]
fn clicking_right_of_end_anchor_table_places_caret_after_it() {
    let mut doc = make_doc_with_end_anchor_table();
    let ((tx, ty, tw, th), _) = table_and_text_bbox(&mut doc);
    let json = doc
        .hit_test_native(0, tx + tw + 40.0, ty + th / 2.0)
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(
        v["charOffset"].as_u64().unwrap(),
        3,
        "표 오른쪽 밖 클릭은 표 뒤(offset 3)여야 한다: {json}"
    );
}

/// 저장 왕복(HWPX·HWP): 표 줄 분리(line_seg 2개)와 렌더 y 관계가 보존되어야 한다.
#[test]
fn end_anchor_layout_survives_save_roundtrip() {
    let mut doc = make_doc_with_end_anchor_table();
    for bytes in [doc.export_hwpx().unwrap(), doc.export_hwp().unwrap()] {
        let mut reloaded = HwpDocument::from_bytes(&bytes).unwrap();
        let segs: serde_json::Value =
            serde_json::from_str(&reloaded.debug_line_seg_tags(0, 0).unwrap()).unwrap();
        assert_eq!(
            segs.as_array().unwrap().len(),
            2,
            "왕복 후에도 line_seg 2개여야 한다: {segs}"
        );
        let ((_tx, ty, _tw, _th), (_xx, xy, _xw, xh)) = table_and_text_bbox(&mut reloaded);
        assert!(
            ty >= xy + xh - 1.0,
            "왕복 후에도 표가 텍스트 줄 아래여야 한다: 표 top={ty} 텍스트 bottom={}",
            xy + xh
        );
    }
}

/// 표 뒤에 타이핑하면 middle-anchor 로 재수렴 — 기존 인라인(한 줄) 동작 복귀.
#[test]
fn typing_after_table_reconverges_to_inline() {
    let mut doc = make_doc_with_end_anchor_table();
    doc.insert_text(0, 0, 3, "오른쪽").unwrap();
    let segs: serde_json::Value =
        serde_json::from_str(&doc.debug_line_seg_tags(0, 0).unwrap()).unwrap();
    assert_eq!(
        segs.as_array().unwrap().len(),
        1,
        "양옆 텍스트(middle-anchor)면 한 줄 인라인으로 복귀해야 한다: {segs}"
    );
    let ((tx, _ty, _tw, _th), _) = table_and_text_bbox(&mut doc);
    // 표가 다시 앞 텍스트 옆(줄 안)에 서야 한다 — 줄 시작이 아닌 x
    assert!(tx > 20.0, "인라인 복귀 시 표는 앞 텍스트 오른쪽: 표 x={tx}");
}
