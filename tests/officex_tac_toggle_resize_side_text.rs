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

/// [2026-08-11 신고 "표 글자처럼 취급 후 줄 간격이 안 먹는다"]
/// 개체(표)만 있는 문단의 줄간격은 **문단 모양에서 다시 계산**돼야 한다. 종전 reflow 는
/// 이전 line_seg 의 line_spacing 을 그대로 복제해, TAC 이후 줄간격을 바꿔도 저장 줄정보가
/// 첫 값(기본 160%)에 고착됐다 — 적용 순서(토글↔줄간격)에 따라 결과도 갈렸다.
#[test]
fn tac_host_paragraph_line_spacing_applies_regardless_of_order() {
    fn build(order_toggle_first: bool, spacing: u32) -> (i32, f64) {
        let mut doc = HwpDocument::create_empty();
        doc.create_blank_document().unwrap();
        doc.insert_text(0, 0, 0, "가나").unwrap();
        doc.split_paragraph(0, 0, 2).unwrap();
        doc.insert_text(0, 1, 0, "뒷문단").unwrap();
        let c: serde_json::Value = serde_json::from_str(
            &doc.create_table_ex(
                r#"{"sectionIdx":0,"paraIdx":0,"charOffset":2,"rowCount":2,"colCount":2,"treatAsChar":false}"#,
            )
            .unwrap(),
        )
        .unwrap();
        let (pi, ci) = (
            c["paraIdx"].as_u64().unwrap() as u32,
            c["controlIdx"].as_u64().unwrap() as u32,
        );
        let json = format!(r#"{{"lineSpacing":{spacing},"lineSpacingType":"Percent"}}"#);
        let apply_all = |doc: &mut HwpDocument| {
            for p in 0..doc.get_paragraph_count(0).unwrap() {
                let _ = doc.apply_para_format(0, p as usize, &json);
            }
        };
        if order_toggle_first {
            doc.set_table_properties(0, pi, ci, r#"{"treatAsChar":true}"#)
                .unwrap();
            apply_all(&mut doc);
        } else {
            apply_all(&mut doc);
            doc.set_table_properties(0, pi, ci, r#"{"treatAsChar":true}"#)
                .unwrap();
        }
        let seg_ls = doc.document().sections[0].paragraphs[pi as usize]
            .line_segs
            .first()
            .map(|l| l.line_spacing)
            .unwrap_or(-1);
        // 표 줄 다음 문단의 y (줄간격이 실제 조판에 반영됐는지)
        let tree = doc.build_page_render_tree(0).unwrap();
        fn walk(n: &RenderNode, needle: &str, out: &mut Option<f64>) {
            if let RenderNodeType::TextRun(tr) = &n.node_type {
                if tr.text.contains(needle) && out.is_none() {
                    *out = Some(n.bbox.y);
                }
            }
            for c in &n.children {
                walk(c, needle, out);
            }
        }
        let mut y = None;
        walk(&tree.root, "뒷문단", &mut y);
        (seg_ls, y.expect("뒷문단 y"))
    }

    // ① 줄간격 100% 는 추가 여백 0
    for toggle_first in [true, false] {
        let (ls, _) = build(toggle_first, 100);
        assert_eq!(
            ls, 0,
            "100% 인데 저장 줄간격이 {ls} (toggle_first={toggle_first}) — 옛 값 고착"
        );
    }
    // ② 300% 는 추가 여백이 실제로 붙고, 적용 순서와 무관하게 같은 결과
    let (ls_a, y_a) = build(true, 300);
    let (ls_b, y_b) = build(false, 300);
    assert!(ls_a > 0, "300% 인데 저장 줄간격이 0 — 줄간격 미반영");
    assert_eq!(
        ls_a, ls_b,
        "적용 순서에 따라 줄간격이 달라짐 ({ls_a} vs {ls_b})"
    );
    assert!(
        (y_a - y_b).abs() < 0.5,
        "적용 순서에 따라 조판이 달라짐 (뒷문단 y {y_a} vs {y_b})"
    );
    // ③ 100% → 300% 로 키우면 표 줄 다음 문단이 확실히 내려간다
    let (_, y100) = build(true, 100);
    assert!(
        y_a > y100 + 20.0,
        "줄간격을 300% 로 올렸는데 조판이 그대로 (100%: {y100}, 300%: {y_a})"
    );
}
