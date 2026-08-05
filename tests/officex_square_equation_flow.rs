//! [트랙4 ④ 2026-08-05] 어울림 수식 — Equation.common.text_wrap 첫 실소비.
//!
//! 1단계: TAC 표 + 비-TAC 수식이 병존해도 **폭이 남는 한 한 줄**이다.
//!
//! [oracle-pdf-mining-20260806] 이 테스트는 원래 "end-anchor solo TAC 표는 자기 줄"
//! 규칙을 사전조건(seg 2개)으로 깔고, 비-TAC 수식이 solo 를 깨서 표가 자기 줄로
//! 오분리되지 않는지를 가드했다. 그 규칙 자체가 **코퍼스·PDF 오라클로 반증돼
//! 제거**되었으므로(§1-B/§1-C: 자기 줄은 폭 초과의 귀결일 뿐) 가드의 전제가 사라졌다.
//! 지금 잠그는 것은 그 후속 계약이다 — 인라인 컨트롤이 몇 개든, 편집을 어떻게 하든
//! **폭 규칙만이 줄을 가른다**. (수식을 무조건 인라인으로 계상하는 height_measurer
//! 가드는 로드측 게이트에서 여전히 쓰인다.)
//! 2단계(옆 흐름): 비-TAC Square 수식이 밴드 host 가 되어(훅 일반화 Equation arm)
//! 옆 문단 줄이 수식 상자를 피해 cs/sw 로 좁혀진다.
use rhwp::renderer::render_tree::{RenderNode, RenderNodeType};
use rhwp::wasm_api::HwpDocument;

fn seg_count(doc: &HwpDocument, para_idx: u32) -> usize {
    let segs: serde_json::Value =
        serde_json::from_str(&doc.debug_line_seg_tags(0, para_idx).unwrap()).unwrap();
    segs.as_array().unwrap().len()
}

fn seg_cs(doc: &HwpDocument, para_idx: u32) -> Vec<i32> {
    let parsed: Vec<(i32, i32, u32)> =
        serde_json::from_str(&doc.debug_line_seg_tags(0, para_idx).unwrap()).unwrap();
    parsed.iter().map(|t| t.1).collect()
}

// ─── 1단계: 비-TAC Square 수식 + TAC 표 병존 — solo 오판정 가드 ──────────

#[test]
fn tac_table_with_non_tac_equation_stays_on_one_line() {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    doc.insert_text(0, 0, 0, "왼쪽").unwrap();
    let c: serde_json::Value = serde_json::from_str(
        &doc.create_table_ex(
            r#"{"sectionIdx":0,"paraIdx":0,"charOffset":2,"rowCount":1,"colCount":1,"treatAsChar":true,"colWidths":[3000]}"#,
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(c["paraIdx"].as_u64(), Some(0), "TAC 표 앵커: {c}");
    // 소형 표(3000HU)는 앞 텍스트와 한 줄 — 폭이 남으므로 줄이 갈리지 않는다(§1-B).
    assert_eq!(
        seg_count(&doc, 0),
        1,
        "소형 end-anchor 표는 앞 텍스트와 한 줄(seg 1개)"
    );

    // 같은 문단에 수식 추가(표 뒤) 후 비-TAC Square 로 — 인라인 컨트롤이 2개가 된다.
    // 비-TAC 이어도 렌더러는 수식을 인라인 취급하므로 폭에 계상된다.
    let e: serde_json::Value = serde_json::from_str(
        &doc.insert_equation(0, 0, 3, "a over b", 1000, 0).unwrap(),
    )
    .unwrap();
    let eq_ci = e["controlIdx"].as_u64().unwrap() as u32;
    doc.set_equation_properties(
        0,
        0,
        eq_ci,
        -1,
        -1,
        r#"{"treatAsChar":false,"textWrap":"Square"}"#,
    )
    .unwrap();
    // 편집으로 재줄바꿈 유발 — 폭이 남는 한 여전히 한 줄이어야 한다.
    doc.insert_text(0, 0, 0, "덧").unwrap();
    assert_eq!(
        seg_count(&doc, 0),
        1,
        "폭이 남는데 줄이 갈렸다 — 폭 규칙 외의 자기 줄 시멘틱이 되살아났는지 확인: {}",
        doc.debug_line_seg_tags(0, 0).unwrap()
    );
}

// ─── 2단계: 비-TAC Square 수식 옆 텍스트 줄 cs/sw 좁힘 ──────────────────

/// 첫 페이지에서 para_index 매칭 Equation 노드 bbox.
fn equation_bbox(doc: &HwpDocument, para_idx: usize) -> Option<(f64, f64, f64, f64)> {
    let tree = doc.build_page_render_tree(0).unwrap();
    fn walk(n: &RenderNode, pi: usize, out: &mut Option<(f64, f64, f64, f64)>) {
        if out.is_some() {
            return;
        }
        if let RenderNodeType::Equation(eq) = &n.node_type {
            if eq.para_index == Some(pi) {
                *out = Some((n.bbox.x, n.bbox.y, n.bbox.width, n.bbox.height));
                return;
            }
        }
        for c in &n.children {
            walk(c, pi, out);
        }
    }
    let mut out = None;
    walk(&tree.root, para_idx, &mut out);
    out
}

/// 첫 페이지의 가시 텍스트 본문 줄(표/글상자 내부 제외).
fn visible_body_lines(doc: &HwpDocument) -> Vec<(f64, f64, f64, f64)> {
    let tree = doc.build_page_render_tree(0).unwrap();
    let mut lines = Vec::new();
    fn walk(n: &RenderNode, in_table: bool, out: &mut Vec<(f64, f64, f64, f64)>) {
        if matches!(n.node_type, RenderNodeType::Table(_)) {
            for c in &n.children {
                walk(c, true, out);
            }
            return;
        }
        if matches!(n.node_type, RenderNodeType::TextBox) {
            return;
        }
        if let RenderNodeType::TextLine(tl) = &n.node_type {
            let has_visible_text = n.children.iter().any(|c| match &c.node_type {
                RenderNodeType::TextRun(tr) => tr.text.chars().any(|ch| !ch.is_whitespace()),
                _ => false,
            });
            if !in_table && tl.para_index.is_some() && has_visible_text {
                out.push((n.bbox.x, n.bbox.y, n.bbox.width, n.bbox.height));
            }
        }
        for c in &n.children {
            walk(c, in_table, out);
        }
    }
    walk(&tree.root, false, &mut lines);
    lines
}

#[test]
fn square_equation_narrows_neighbor_lines() {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    let filler = "가나다라마바사 아자차카타파하 강물이 흐르고 산이 높다 바람이 분다";
    let flen = filler.chars().count();
    doc.insert_text(0, 0, 0, filler).unwrap();
    for i in 0..4u32 {
        doc.split_paragraph_native(0, i as usize, flen).unwrap();
        doc.insert_text(0, i + 1, 0, filler).unwrap();
    }
    // 문단 2 끝에서 분할 → 문단 3 = 빈 host, 문단 4·5 = 후속 본문.
    doc.split_paragraph_native(0, 2, flen).unwrap();
    let e: serde_json::Value = serde_json::from_str(
        &doc
            .insert_equation(0, 3, 0, "{sum from 1 to n} over {k+1}", 3000, 0)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(e["paraIdx"].as_u64(), Some(3), "수식 host: {e}");
    let eq_ci = e["controlIdx"].as_u64().unwrap() as u32;
    doc.set_equation_properties(
        0,
        3,
        eq_ci,
        -1,
        -1,
        r#"{"treatAsChar":false,"textWrap":"Square"}"#,
    )
    .unwrap();
    // 편집 → 훅 발화(Equation arm).
    doc.insert_text(0, 4, 0, "덧말 ").unwrap();

    let (ex, ey, ew, eh) = equation_bbox(&doc, 3).expect("수식 노드가 렌더트리에 없음");
    assert!(eh > 30.0, "사전조건: 수식이 여러 줄을 덮을 만큼 커야 한다 (h={eh:.0})");
    let lines = visible_body_lines(&doc);
    let overlapping = lines
        .iter()
        .filter(|(x, y, w, h)| {
            *x < ex + ew - 1.0 && x + w > ex + 1.0 && *y < ey + eh - 1.0 && y + h > ey + 1.0
        })
        .count();
    assert_eq!(
        overlapping, 0,
        "본문 줄이 어울림 수식 상자를 뚫음 — Equation 밴드 미등록. eq=({ex:.0},{ey:.0},{ew:.0},{eh:.0}) lines={lines:?}"
    );
    // 수식(좌측) 옆으로 좁혀진 줄의 cs 기록 — 재줄바꿈이 저장 진실.
    let narrowed: Vec<u32> = (0..doc.get_paragraph_count(0).unwrap())
        .filter(|&p| seg_cs(&doc, p).iter().any(|&c| c > 0))
        .collect();
    assert!(
        !narrowed.is_empty(),
        "수식 옆 문단에 좁힘 cs 기록이 없음 — Equation host 일반화 미발화"
    );
}
