//! [2026-08-11 신고] 그리드 표 → 패널 글자처럼 취급 ON → 폭 축소 → 표 옆 텍스트:
//! 옆 텍스트가 표 **상단**에 떠서 그려진다 (한컴: 표는 줄의 큰 글자 — 옆 텍스트
//! 기준선은 표 하단부, r-분할). 토글로 만들어진 TAC 표의 host 문단에서 같은 줄
//! 텍스트의 세로 배치를 고정하는 핀.

use rhwp::renderer::render_tree::{RenderNode, RenderNodeType};
use rhwp::wasm_api::HwpDocument;

fn table_and_side_text(doc: &mut HwpDocument) -> ((f64, f64, f64, f64), (f64, f64, f64, f64)) {
    let tree = doc.build_page_render_tree(0).unwrap();
    let mut tb: Option<(f64, f64, f64, f64)> = None;
    let mut tx: Option<(f64, f64, f64, f64)> = None;
    fn walk(
        n: &RenderNode,
        tb: &mut Option<(f64, f64, f64, f64)>,
        tx: &mut Option<(f64, f64, f64, f64)>,
    ) {
        if matches!(n.node_type, RenderNodeType::Table(_)) {
            if tb.is_none() {
                *tb = Some((n.bbox.x, n.bbox.y, n.bbox.width, n.bbox.height));
            }
            return;
        }
        if let RenderNodeType::TextRun(tr) = &n.node_type {
            if tr.text.contains("옆글자") {
                *tx = Some((n.bbox.x, n.bbox.y, n.bbox.width, n.bbox.height));
            }
        }
        for c in &n.children {
            walk(c, tb, tx);
        }
    }
    walk(&tree.root, &mut tb, &mut tx);
    (tb.expect("표 노드"), tx.expect("옆글자 TextRun"))
}

/// 사용자 흐름 재현: 비TAC 생성 → treatAsChar 토글 ON → 열 폭 축소 → 표 뒤 텍스트.
fn make_toggled_narrow_doc() -> HwpDocument {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    doc.insert_text(0, 0, 0, "가나다라마바").unwrap();
    // 그리드 경로와 동일: treatAsChar=false 로 생성 (단폭)
    let c: serde_json::Value = serde_json::from_str(
        &doc.create_table_ex(
            r#"{"sectionIdx":0,"paraIdx":0,"charOffset":6,"rowCount":3,"colCount":3,"treatAsChar":false}"#,
        )
        .unwrap(),
    )
    .unwrap();
    let (pi, ci) = (
        c["paraIdx"].as_u64().unwrap() as u32,
        c["controlIdx"].as_u64().unwrap() as u32,
    );
    // 패널 토글과 동일 경로
    doc.set_table_properties(0, pi, ci, r#"{"treatAsChar":true}"#)
        .unwrap();
    // 핸들 리사이즈와 동등: 열 폭 축소 (3열 × 4000HU ≈ 42mm)
    doc.set_table_column_widths(0, pi, ci, "[4000,4000,4000]")
        .unwrap();
    // 표 뒤 텍스트 (논리 끝)
    let len = doc.get_logical_length(0, pi).unwrap();
    doc.insert_text_logical(0, pi, len, "옆글자").unwrap();
    doc
}

/// 옆 텍스트는 표와 **같은 줄**에서 표 하단부에 앉아야 한다 — 텍스트 상자의
/// 세로 중심이 표의 세로 중심보다 **아래**여야 하고(상단 부착 금지), 텍스트
/// 바닥은 표 상단보다 충분히 아래(표 높이의 절반 이상 지점)여야 한다.
#[test]
fn side_text_after_toggle_and_resize_sits_at_table_bottom_zone() {
    let mut doc = make_toggled_narrow_doc();
    let ((_tx0, ty, _tw, th), (_xx, xy, _xw, xh)) = table_and_side_text(&mut doc);
    let text_bottom = xy + xh;
    let table_mid = ty + th / 2.0;
    assert!(
        text_bottom > table_mid,
        "옆 텍스트가 표 상단에 떠 있다(신고 재현): 텍스트 바닥={text_bottom:.1} ≤ 표 중앙={table_mid:.1} \
         (표 y={ty:.1} h={th:.1}, 텍스트 y={xy:.1} h={xh:.1})"
    );
}

/// HWPX 저장 왕복 후에도 동일 규칙 유지 (재로드 재현 관찰과 짝).
#[test]
fn side_text_position_survives_hwpx_roundtrip() {
    let mut doc = make_toggled_narrow_doc();
    let bytes = doc.export_hwpx().unwrap();
    let mut reloaded = HwpDocument::from_bytes(&bytes).unwrap();
    let ((_tx0, ty, _tw, th), (_xx, xy, _xw, xh)) = table_and_side_text(&mut reloaded);
    let text_bottom = xy + xh;
    let table_mid = ty + th / 2.0;
    assert!(
        text_bottom > table_mid,
        "왕복 후 옆 텍스트가 표 상단에 떠 있다: 텍스트 바닥={text_bottom:.1} ≤ 표 중앙={table_mid:.1}"
    );
}
