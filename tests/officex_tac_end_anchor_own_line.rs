//! end-anchored 글자취급 표 = **앞 텍스트와 같은 줄** (폭이 남는 한).
//!
//! [oracle-pdf-mining-20260806] 종전 이 파일은 "앞에만 텍스트가 있는 TAC 표는 다음
//! 줄에 놓인다"(웹한글 편집 화면 픽셀 관찰, `tac-inline-baseline-report-20260805`)를
//! 잠그고 있었다. 그 오라클은 **코퍼스·PDF 오라클로 반증돼 정정**됐다:
//!
//! - §1-B: 저장 656파일·글자취급 표 5,751건 전수에서 END-anchor(앞에만 텍스트) 표가
//!   "폭이 남고 `\n` 도 없는" 표본은 **전부 seg 1개**(표+글 한 줄). seg 2개 이상인
//!   15건은 100% 폭 초과 또는 강제 줄바꿈으로 설명된다.
//! - §1-C: `samples/tac-case-001..005`(같은 문단, 표 폭만 다름)는 표가 넓어질 때
//!   **뒤 텍스트만** 다음 줄로 밀리고 표는 앞 텍스트 줄에 남음을 보인다 — 종전 규칙
//!   (앞 텍스트 위 줄 / 표 아래 줄)과 상하 반대.
//! - §1-D: 한컴 인쇄 PDF(`samples/복학원서.pdf`) 실측에서도 표 오른쪽에 같은 기준선의
//!   글자가 렌더된다.
//! - §2-B: 종전 자기 줄 seg 의 `baseline = 바깥여백상 + 표높이`(≈0.99·lh)도 반증
//!   (표 바닥이 기준선보다 30.57pt **아래**).
//!
//! 그래서 end-anchor 표는 특례 없이 다른 TAC 개체와 같은 폭 규칙
//! (`line_breaking.rs::BreakToken::Object`)을 탄다. 폭 임계 전환 자체는
//! `renderer::composer::tests::tac_case_width_threshold_series_matches_hancom_stored_layout`
//! 이 실파일로 잠근다.
//!
//! 원래 증상("표가 텍스트보다 46px 위" + 블록 캐럿)의 진짜 원인은 편집 생성 표의
//! `Table.attr` 오설정이었고 96d24f4b3 에서 수리됐다 — 아래 핀이 비회귀를 지킨다.
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

fn seg_count(doc: &HwpDocument) -> usize {
    let segs: serde_json::Value =
        serde_json::from_str(&doc.debug_line_seg_tags(0, 0).unwrap()).unwrap();
    segs.as_array().unwrap().len()
}

/// 표는 앞 텍스트 **오른쪽, 같은 줄**에 — 가로 순서(글→표)와 세로 동거.
#[test]
fn end_anchor_table_shares_line_with_preceding_text() {
    let mut doc = make_doc_with_end_anchor_table();
    let ((tx, ty, _tw, th), (xx, xy, xw, xh)) = table_and_text_bbox(&mut doc);
    // 가로: 표는 앞 텍스트가 끝난 자리에서 시작한다(TAC 표 = 큰 글자).
    assert!(
        (tx - (xx + xw)).abs() <= 1.5,
        "표는 앞 텍스트 바로 오른쪽에서 시작해야 한다: 텍스트 우변={} 표 x={tx}",
        xx + xw
    );
    // 세로: 같은 줄 — 상자가 겹친다.
    let overlap = (ty + th).min(xy + xh) - ty.max(xy);
    assert!(
        overlap > 0.0,
        "표와 앞 텍스트가 같은 줄이어야 한다(세로 겹침): 표 y=[{ty},{}] 텍스트 y=[{xy},{}]",
        ty + th,
        xy + xh
    );
}

/// line_seg 1개 — 폭이 남으면 자기 줄을 만들지 않는다(오라클 §1-B).
#[test]
fn end_anchor_small_table_keeps_single_line_seg() {
    let doc = make_doc_with_end_anchor_table();
    assert_eq!(
        seg_count(&doc),
        1,
        "소형 end-anchor 는 표+글 한 줄(seg 1개): {}",
        doc.debug_line_seg_tags(0, 0).unwrap()
    );
}

/// 앞 텍스트는 양쪽정렬로 벌어지지 않는다 — 표와 공유하는 줄이 문단의 마지막 줄이다.
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

/// [96d24f4b3 비회귀] 원래 증상 — 표가 텍스트보다 46px 위로 튀는 세로 데싱크와
/// 줄 전체를 덮는 블록 캐럿. 둘 다 `Table.attr` 오설정이 원인이었고 수리됐다.
/// 실측(2026-08-06): 텍스트 줄 top=132.3 / 표 top=137.3(Δ5.0px = 바깥여백급),
/// 캐럿 h=13.3 vs 표 h=34.2.
#[test]
fn no_vertical_desync_nor_block_caret_beside_table() {
    let mut doc = make_doc_with_end_anchor_table();
    let ((_tx, ty, _tw, th), (_xx, xy, _xw, xh)) = table_and_text_bbox(&mut doc);
    assert!(
        (ty - xy).abs() < 10.0,
        "표 top({ty})과 텍스트 줄 top({xy})이 한 줄(바깥여백)급을 넘어 어긋났다 — \
         종전 46px 데싱크 회귀"
    );
    for offset in [0u32, 1, 2] {
        let r: serde_json::Value =
            serde_json::from_str(&doc.get_cursor_rect(0, 0, offset).unwrap()).unwrap();
        let h = r["height"].as_f64().unwrap();
        assert!(
            h < th * 0.8,
            "offset {offset} 캐럿(h={h})이 표 높이({th})급 블록 커서면 안 된다 (텍스트 h={xh})"
        );
    }
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

/// 저장 왕복(HWPX·HWP): seg 1개와 "글 다음 표" 가로 순서가 보존되어야 한다.
#[test]
fn end_anchor_layout_survives_save_roundtrip() {
    let mut doc = make_doc_with_end_anchor_table();
    for bytes in [doc.export_hwpx().unwrap(), doc.export_hwp().unwrap()] {
        let mut reloaded = HwpDocument::from_bytes(&bytes).unwrap();
        assert_eq!(
            seg_count(&reloaded),
            1,
            "왕복 후에도 seg 1개여야 한다: {}",
            reloaded.debug_line_seg_tags(0, 0).unwrap()
        );
        let ((tx, ty, _tw, th), (xx, xy, xw, xh)) = table_and_text_bbox(&mut reloaded);
        assert!(
            (tx - (xx + xw)).abs() <= 1.5,
            "왕복 후에도 표는 앞 텍스트 오른쪽: 텍스트 우변={} 표 x={tx}",
            xx + xw
        );
        assert!(
            (ty + th).min(xy + xh) - ty.max(xy) > 0.0,
            "왕복 후에도 같은 줄: 표 y=[{ty},{}] 텍스트 y=[{xy},{}]",
            ty + th,
            xy + xh
        );
    }
}

/// 표 뒤에 타이핑해도(mid-anchor) 한 줄 유지 — 가로 순서는 앞글·표·뒷글.
#[test]
fn typing_after_table_keeps_single_line_and_x_order() {
    let mut doc = make_doc_with_end_anchor_table();
    doc.insert_text(0, 0, 3, "오른쪽").unwrap();
    assert_eq!(
        seg_count(&doc),
        1,
        "mid-anchor 소형도 한 줄(seg 1개): {}",
        doc.debug_line_seg_tags(0, 0).unwrap()
    );
    let tree = doc.build_page_render_tree(0).unwrap();
    // 본문의 (x, 무엇) 순서를 모아 앞글 → 표 → 뒷글 순인지 본다.
    let mut items: Vec<(f64, String)> = Vec::new();
    fn walk(n: &RenderNode, items: &mut Vec<(f64, String)>) {
        if matches!(n.node_type, RenderNodeType::Table(_)) {
            items.push((n.bbox.x, "표".to_string()));
            return;
        }
        if let RenderNodeType::TextRun(tr) = &n.node_type {
            if !tr.text.trim().is_empty() {
                items.push((n.bbox.x, tr.text.clone()));
            }
        }
        for c in &n.children {
            walk(c, items);
        }
    }
    walk(&tree.root, &mut items);
    items.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let order: Vec<&str> = items.iter().map(|(_, s)| s.as_str()).collect();
    assert_eq!(
        order,
        vec!["왼쪽", "표", "오른쪽"],
        "가로 순서가 앞글·표·뒷글이어야 한다: {items:?}"
    );
}
