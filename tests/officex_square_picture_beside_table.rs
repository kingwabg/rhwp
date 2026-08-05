//! [트랙4 ① 2026-08-05] 조합 "표 옆 이미지" — 빈 host 에 어울림 그림+표 병존.
//!
//! 파이프라인: 편집 훅(reflow_paras_for_square_bands)의 렌더트리 walk 가 표·그림
//! 밴드를 **둘 다** 등록 → composer(segs_for_top)가 다중 밴드 합집합(사이 조각)으로
//! 재줄바꿈 → 렌더는 저장 cs/sw 재생. layout 라이브 밴드도 그림/도형 등가 분기
//! (push_square_float_object_band)로 등록된다.
//! 빌더/워크는 officex_square_edit_hook.rs 핀 6/8 의 것을 재사용(복제)한다.
use rhwp::renderer::render_tree::{RenderNode, RenderNodeType};
use rhwp::wasm_api::HwpDocument;

/// 1x1 투명 PNG (67바이트).
const PNG_1X1: [u8; 67] = [
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
    0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
    0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x62, 0x00,
    0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49,
    0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
];

/// 첫 페이지의 **가시 텍스트** 본문 줄(표/글상자 내부·빈 줄 제외): (x, y, w, h).
/// 표 escape 빈 문단의 전폭 빈 줄은 잉크가 없어 겹침 판정 대상이 아니다.
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

fn overlap_with(lines: &[(f64, f64, f64, f64)], tx: f64, ty: f64, tw: f64, th: f64) -> usize {
    lines
        .iter()
        .filter(|(x, y, w, h)| {
            *x < tx + tw - 1.0 && x + w > tx + 1.0 && *y < ty + th - 1.0 && y + h > ty + 1.0
        })
        .count()
}

/// 첫 페이지 렌더트리에서 Image 노드 bbox.
fn image_bbox(doc: &HwpDocument) -> Option<(f64, f64, f64, f64)> {
    let tree = doc.build_page_render_tree(0).unwrap();
    fn walk(n: &RenderNode, out: &mut Option<(f64, f64, f64, f64)>) {
        if out.is_some() {
            return;
        }
        if matches!(n.node_type, RenderNodeType::Image(_)) {
            *out = Some((n.bbox.x, n.bbox.y, n.bbox.width, n.bbox.height));
            return;
        }
        for c in &n.children {
            walk(c, out);
        }
    }
    let mut out = None;
    walk(&tree.root, &mut out);
    out
}

fn seg_cs(doc: &HwpDocument, para_idx: u32) -> Vec<i32> {
    let parsed: Vec<(i32, i32, u32)> =
        serde_json::from_str(&doc.debug_line_seg_tags(0, para_idx).unwrap()).unwrap();
    parsed.iter().map(|t| t.1).collect()
}

fn narrowed_paras(doc: &HwpDocument) -> Vec<u32> {
    (0..doc.get_paragraph_count(0).unwrap())
        .filter(|&p| seg_cs(doc, p).iter().any(|&c| c > 0))
        .collect()
}

/// 4줄 본문(문단 0) + 빈 host(마지막 문단)에 어울림 표(좌)+그림(우) 병존.
/// 두 개체를 본문 줄 위로 끌어 올려 좌·우 병치 밴드를 만든다(핀 6/8 레시피).
/// 반환: (doc, host 문단, 표 컨트롤).
fn build() -> (HwpDocument, u32, u32) {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    let long = "가나다라마바사 아자차카타파하 강물이 흐르고 산이 높다 바람이 분다 구름이 간다 하늘이 푸르다 나무가 자란다 새가 웃는다 경치가 아름답다 보리가 여무다 들판이 넓다 여기에 개체를 놓으면 글이 어떻게 흐르는지 본다 뒤에 말을 더 붙여 여러 줄이 되도록 한다 그래야 어울림이 보인다";
    doc.insert_text(0, 0, 0, long).unwrap();
    let flen = long.chars().count();
    doc.split_paragraph_native(0, 0, flen).unwrap();
    let host = doc.get_paragraph_count(0).unwrap() - 1;

    // 표(좌측): 빈 host 문단에 생성 후 Square 로 — 본문 줄 위로 끌어 올린다.
    let c: serde_json::Value = serde_json::from_str(&doc.create_table_ex(&format!(
        r#"{{"sectionIdx":0,"paraIdx":{host},"charOffset":0,"rowCount":2,"colCount":1,"treatAsChar":false,"colWidths":[15000]}}"#
    )).unwrap()).unwrap();
    let (tp, tc) = (
        c["paraIdx"].as_u64().unwrap() as u32,
        c["controlIdx"].as_u64().unwrap() as u32,
    );
    assert_eq!(tp, host, "표가 빈 host 문단 {host} 에 앵커: {c}");
    doc.set_table_properties(0, tp, tc,
        r#"{"treatAsChar":false,"textWrap":"Square","vertRelTo":"Para","horzRelTo":"Column","horzAlign":"Left","vertOffset":0,"horzOffset":0}"#
    ).unwrap();
    doc.move_table_offset(0, tp, tc, 0, -6000).unwrap();

    // 그림(우측): 표와 **같은 host 문단**에 병존 — 종이 절대좌표로 본문 우측 줄 위.
    // (insertPicture 실측 기본값 Paper/Paper — 핀 6 과 동일 레시피.
    //  본문 우단 x≈679px, 그림 80px → x≈600px=45016HU, y≈140px=10500HU.)
    let r: serde_json::Value = serde_json::from_str(
        &doc.insert_picture(0, tp, 0, "[]", &PNG_1X1, 6000, 6000, 1, 1, "png", "", None, None)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        r["paraIdx"].as_u64(),
        Some(tp as u64),
        "그림이 표 host 문단 {tp} 에 병존해야 함: {r}"
    );
    let pic_ci = r["controlIdx"].as_u64().unwrap() as u32;
    doc.set_picture_properties(0, tp, pic_ci, r#"{"vertOffset":10500,"horzOffset":45016}"#)
        .unwrap();
    (doc, tp, tc)
}

#[test]
fn text_avoids_union_of_picture_and_table_bands() {
    let (mut doc, tp, tc) = build();
    // 편집(insert_text) → 훅 발화. 병존 host 의 밴드 2개가 모두 등록되어야 한다.
    doc.insert_text(0, 0, 0, "덧말 ").unwrap();

    let bb: serde_json::Value =
        serde_json::from_str(&doc.get_table_bbox(0, tp, tc).unwrap()).unwrap();
    let (tx, ty, tw, th) = (
        bb["x"].as_f64().unwrap(),
        bb["y"].as_f64().unwrap(),
        bb["width"].as_f64().unwrap(),
        bb["height"].as_f64().unwrap(),
    );
    let (ix, iy, iw, ih) = image_bbox(&doc).expect("그림 노드가 렌더트리에 없음");

    // ① 그림·표 상호 비겹침 (좌/우 병치).
    assert!(
        tx + tw <= ix + 1.0 || ix + iw <= tx + 1.0 || ty + th <= iy + 1.0 || iy + ih <= ty + 1.0,
        "표({tx:.0},{ty:.0},{tw:.0},{th:.0})와 그림({ix:.0},{iy:.0},{iw:.0},{ih:.0})이 겹침"
    );

    // ② 본문 가시 줄이 두 개체 bbox 합집합을 회피.
    let lines = visible_body_lines(&doc);
    assert_eq!(
        overlap_with(&lines, tx, ty, tw, th),
        0,
        "본문 줄이 표를 뚫음 — 병존 host 표 밴드 미등록. lines={lines:?}"
    );
    assert_eq!(
        overlap_with(&lines, ix, iy, iw, ih),
        0,
        "본문 줄이 그림을 뚫음 — 병존 host 그림 밴드 미등록. lines={lines:?}"
    );

    // ③ 합집합 "사이 조각" 증거: 두 개체 y 가 동시에 걸치는 구간과 겹치는 줄이 존재하고
    //    (전부 아래로 밀린 게 아니라 옆 흐름), 그 줄은 표 오른쪽에서 시작해 그림 왼쪽에서
    //    끝난다.
    let both_top = ty.max(iy);
    let both_bottom = (ty + th).min(iy + ih);
    assert!(both_bottom > both_top, "사전조건: 표·그림 y 구간이 겹쳐야 병치 검증이 된다");
    let mid = lines
        .iter()
        .find(|(_, y, _, h)| *y < both_bottom - 1.0 && y + h > both_top + 1.0);
    let (mx, _, mw, _) = mid.expect("개체 옆(사이 조각)에 선 본문 줄이 없다 — 전부 아래로 스택");
    assert!(
        *mx >= tx + tw - 1.0 && mx + mw <= ix + 1.0,
        "사이 조각 줄(x={mx:.0} w={mw:.0})이 표 오른쪽({:.0})~그림 왼쪽({ix:.0}) 밖",
        tx + tw
    );
    // ④ 좁힘 cs 기록 존재 (재줄바꿈이 저장 진실).
    assert!(!narrowed_paras(&doc).is_empty(), "좁힘 cs 기록이 없음");
}

#[test]
fn host_delete_restores_full_width() {
    let (mut doc, tp, _tc) = build();
    doc.insert_text(0, 0, 0, "덧말 ").unwrap();
    assert!(!narrowed_paras(&doc).is_empty(), "사전조건: 좁힘 문단 존재");

    // host 문단 삭제 → 표·그림 동시 소멸 → 전폭 원복.
    doc.delete_paragraph(0, tp).unwrap();
    let narrowed = narrowed_paras(&doc);
    assert!(
        narrowed.is_empty(),
        "host 삭제 후 전폭 원복 실패 — 좁힘 잔재 문단: {narrowed:?}"
    );
}
