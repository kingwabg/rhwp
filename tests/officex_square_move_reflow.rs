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
        if let RenderNodeType::TextLine(tl) = &n.node_type {
            // para_index 가 없는 노드는 앵커 문단의 조판부호 마크(zero-width) — 본문 줄이
            // 아니고 렌더 픽셀에도 기여하지 않으므로 겹침 판정에서 제외한다.
            if !in_table && tl.para_index.is_some() {
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

// ─── 양쪽 흐름(BothSides) 2조각 잠금 ───────────────────────────────
// [2026-07-30 조사 결론 = 부록4] 양쪽 2조각은 이미 구현돼 있는데 **테스트가 0건**이었다.
// 게이트·태그·선택 rect 를 손대기 전에 현재 시각 동작을 3튜플로 잠근다.

/// 본문 TextLine(표 내부 제외)을 (x, y, w) 정수 튜플로 덤프 — 시각 진실.
fn body_lines(doc: &HwpDocument) -> Vec<(i32, i32, i32)> {
    let tree = doc.build_page_render_tree(0).unwrap();
    fn walk(n: &RenderNode, in_table: bool, out: &mut Vec<(i32, i32, i32)>) {
        if matches!(n.node_type, RenderNodeType::Table(_)) {
            for c in &n.children {
                walk(c, true, out);
            }
            return;
        }
        if let RenderNodeType::TextLine(tl) = &n.node_type {
            if !in_table && tl.para_index == Some(0) {
                out.push((n.bbox.x as i32, n.bbox.y as i32, n.bbox.width as i32));
            }
        }
        for c in &n.children {
            walk(c, in_table, out);
        }
    }
    let mut out = Vec::new();
    walk(&tree.root, false, &mut out);
    out.sort_by_key(|&(x, y, _)| (y, x));
    out
}

/// 4줄짜리 한 문단 + 빈 앵커 문단(표 host) 구성 — 부록3/4 실측과 같은 구조.
fn doc_with_long_para() -> (HwpDocument, usize) {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    let long = "가나다라마바사 아자차카타파하 강물이 흐르고 산이 높다 바람이 분다 구름이 간다 하늘이 푸르다 나무가 자란다 새가 웃는다 경치가 아름답다 보리가 여무다 들판이 넓다 여기에 표를 놓으면 글이 어떻게 흐르는지 본다 뒤에 말을 더 붙여 여러 줄이 되도록 한다 그래야 어울림이 보인다";
    doc.insert_text(0, 0, 0, long).unwrap();
    let flen = long.chars().count();
    doc.split_paragraph_native(0, 0, flen).unwrap();
    let host = doc.get_paragraph_count(0).unwrap() as usize - 1;
    (doc, host)
}

/// 부분폭 어울림(양쪽) 표를 host 문단에 만들고 (dh, dv) 만큼 이동.
fn bothsides_table(doc: &mut HwpDocument, host: usize, dh: i32, dv: i32) -> (u32, u32) {
    let c: serde_json::Value = serde_json::from_str(&doc.create_table_ex(&format!(
        r#"{{"sectionIdx":0,"paraIdx":{host},"charOffset":0,"rowCount":2,"colCount":2,"treatAsChar":false,"colWidths":[3000,3000]}}"#
    )).unwrap()).unwrap();
    let (pi, ci) = (c["paraIdx"].as_u64().unwrap() as u32, c["controlIdx"].as_u64().unwrap() as u32);
    doc.delete_table_column(0, pi, ci, 1).unwrap();
    doc.set_table_properties(0, pi, ci,
        r#"{"treatAsChar":false,"textWrap":"Square","textFlow":"BothSides","vertRelTo":"Para","horzRelTo":"Column","vertOffset":0,"horzOffset":0}"#
    ).unwrap();
    doc.move_table_offset(0, pi, ci, dh, dv).unwrap();
    (pi, ci)
}

/// 표가 본문 가운데 → 겹치는 줄이 좌·우 두 조각(같은 y)으로 갈라진다.
/// 한컴 O10 과 같은 구조(부록3). 이 잠금이 깨지면 양쪽 흐름이 회귀한 것이다.
#[test]
fn bothsides_center_table_splits_lines_into_two_fragments() {
    let (mut doc, host) = doc_with_long_para();
    let (pi, ci) = bothsides_table(&mut doc, host, 9000, -4500);
    let bb: serde_json::Value =
        serde_json::from_str(&doc.get_table_bbox(0, pi, ci).unwrap()).unwrap();
    let (tx, tw) = (bb["x"].as_f64().unwrap(), bb["width"].as_f64().unwrap());
    let lines = body_lines(&doc);
    eprintln!("표 x={tx:.0} w={tw:.0} / 줄 {lines:?}");

    // 같은 y 를 공유하는 조각 쌍이 최소 1개 — 그게 좌·우 분할의 증거.
    let mut pairs = 0;
    for i in 1..lines.len() {
        if lines[i].1 == lines[i - 1].1 {
            pairs += 1;
            let (lx, _, lw) = lines[i - 1];
            let (rx, _, _) = lines[i];
            assert!(
                (lx + lw) as f64 <= tx + 1.0,
                "좌 조각({lx}+{lw})이 표 좌단({tx:.0})을 넘음"
            );
            assert!(
                rx as f64 >= tx + tw - 1.0,
                "우 조각({rx})이 표 우단({:.0}) 왼쪽에서 시작",
                tx + tw
            );
        }
    }
    assert!(pairs >= 1, "좌·우 조각 쌍이 없다 — 양쪽 흐름 회귀: {lines:?}");
    // 표를 덮는 본문 줄은 없다.
    assert_eq!(overlap_count(&doc, pi, ci), 0);
}

/// 표를 본문 왼쪽에 붙이면(좌측 여유 0) 우측 한쪽 흐름 — 현행 동작 잠금.
/// ⚠ 한컴은 이 경우에도 2세그를 유지하고 좌 세그를 EMPTY 로 남긴다(부록4 갭 #1·#3).
///   시각 결과는 같으므로 이 테스트는 "우측만 흐른다"만 잠근다.
#[test]
fn bothsides_left_edge_table_flows_right_only() {
    let (mut doc, host) = doc_with_long_para();
    let (pi, ci) = bothsides_table(&mut doc, host, 0, -4500);
    let bb: serde_json::Value =
        serde_json::from_str(&doc.get_table_bbox(0, pi, ci).unwrap()).unwrap();
    let (tx, tw) = (bb["x"].as_f64().unwrap(), bb["width"].as_f64().unwrap());
    let lines = body_lines(&doc);
    eprintln!("표 x={tx:.0} y={:.0} h={:.0} w={tw:.0} / 줄 {lines:?}",
        bb["y"].as_f64().unwrap(), bb["height"].as_f64().unwrap());
    // 표와 같은 y 인 줄은 모두 표 우단 이후에서 시작한다.
    for &(x, y, w) in &lines {
        let overlaps_y = (y as f64) < bb["y"].as_f64().unwrap() + bb["height"].as_f64().unwrap()
            && (y + 13) as f64 > bb["y"].as_f64().unwrap();
        if overlaps_y {
            assert!(
                x as f64 >= tx + tw - 1.0,
                "표 옆 줄(x={x} w={w})이 표({tx:.0}~{:.0}) 왼쪽/위에 걸침",
                tx + tw
            );
        }
    }
    assert_eq!(overlap_count(&doc, pi, ci), 0);
}

/// [게이트 2026-07-30] 옆 조각 최소 폭이 한컴 실측(34.3px)까지 내려갔다 —
/// 종전 40px 게이트로는 34~40px 여유가 통째로 버려졌다. samples/pic2.hwpx 근거.
#[test]
fn side_gate_matches_hancom_measured_minimum() {
    use rhwp::model::shape::TextFlow;
    use rhwp::renderer::composer::side_pick_for_band_pub as side_pick_for_band;
    // 좌 여유 36px(한컴 실측 34.3 이상, 종전 게이트 40 미달) → 좌 조각으로 인정된다.
    let picked = side_pick_for_band(600.0, 36.0, 500.0, TextFlow::LeftOnly);
    assert_eq!(picked, Some((0.0, 36.0)), "36px 좌 여유가 버려졌다");
    // 30px 은 실측 하한 미달 → 인정하지 않는다(우리 fill 이 빈 세그를 못 만든다).
    assert_eq!(
        side_pick_for_band(600.0, 30.0, 500.0, TextFlow::LeftOnly),
        None,
        "하한 미달인데 조각으로 인정됐다"
    );
}

/// 좌·우 2세그의 태그가 한컴 인코딩(좌=FIRST-only, 우=LAST-only)이다.
/// pic2.hwpx 실측: flags 131072(0x20000) / 262144(0x40000).
#[test]
fn bothsides_segments_use_hancom_first_last_tags() {
    use rhwp::model::paragraph::LineSeg;
    let (mut doc, host) = doc_with_long_para();
    let (pi, ci) = bothsides_table(&mut doc, host, 9000, -4500);
    let _ = (pi, ci);
    let segs = doc.debug_line_seg_tags(0, 0).unwrap();
    let parsed: Vec<(i32, i32, u32)> = serde_json::from_str(&segs).unwrap();
    eprintln!("segs = {parsed:?}");
    // 같은 vpos 를 공유하는 쌍을 찾아 태그를 검사
    let mut pairs = 0;
    for w in parsed.windows(2) {
        if w[0].0 == w[1].0 {
            pairs += 1;
            assert_eq!(
                w[0].2 & (LineSeg::TAG_FIRST_SEGMENT | LineSeg::TAG_LAST_SEGMENT),
                LineSeg::TAG_FIRST_SEGMENT,
                "좌 세그가 FIRST-only 가 아니다: {:#x}",
                w[0].2
            );
            assert_eq!(
                w[1].2 & (LineSeg::TAG_FIRST_SEGMENT | LineSeg::TAG_LAST_SEGMENT),
                LineSeg::TAG_LAST_SEGMENT,
                "우 세그가 LAST-only 가 아니다: {:#x}",
                w[1].2
            );
        }
    }
    assert!(pairs >= 1, "2세그 쌍이 없다: {parsed:?}");
}

/// [선택 rect 2026-07-30] 2조각 줄의 선택 하이라이트가 표를 덮지 않는다.
/// 종전엔 줄을 단 좌단~우단 전폭으로 칠해 표 위에 하이라이트가 얹혔다(부록4 갭 #2).
#[test]
fn selection_rects_respect_bothsides_fragments() {
    let (mut doc, host) = doc_with_long_para();
    let (pi, ci) = bothsides_table(&mut doc, host, 9000, -4500);
    let bb: serde_json::Value =
        serde_json::from_str(&doc.get_table_bbox(0, pi, ci).unwrap()).unwrap();
    let (tx, ty, tw, th) = (
        bb["x"].as_f64().unwrap(), bb["y"].as_f64().unwrap(),
        bb["width"].as_f64().unwrap(), bb["height"].as_f64().unwrap(),
    );
    let len = doc.get_logical_length(0, 0).unwrap();
    let rs: serde_json::Value =
        serde_json::from_str(&doc.get_selection_rects(0, 0, 0, 0, len).unwrap()).unwrap();
    let mut covering = 0;
    for r in rs.as_array().unwrap() {
        let (x, y, w, h) = (
            r["x"].as_f64().unwrap(), r["y"].as_f64().unwrap(),
            r["width"].as_f64().unwrap(), r["height"].as_f64().unwrap(),
        );
        let x_ov = x < tx + tw - 1.0 && x + w > tx + 1.0;
        let y_ov = y < ty + th - 1.0 && y + h > ty + 1.0;
        eprintln!("  rect x={x:.0} y={y:.0} w={w:.0} ov={}", x_ov && y_ov);
        if x_ov && y_ov {
            covering += 1;
        }
    }
    assert_eq!(covering, 0, "선택 하이라이트 {covering}개가 표를 덮는다");
}
