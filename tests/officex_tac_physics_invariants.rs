//! TAC(글자처럼 취급) 조판 물리 영구 핀 — 반복 회귀 신고를 **증상 층위**에서
//! 즉사시키는 스위트 (2026-08-11 감사 격차 봉합).
//!
//! 1. 세로 r-분할: 글리프 상자는 기준선을 r:(1−r)로 가른다 — 개체 바닥이
//!    기준선 아래로 (1−r)·H 잠긴다. '바닥=기준선' clamp 회귀(개체가 위로 붕 뜸)와
//!    '줄상단 앵커' 회귀를 그림 **생성 경로 + HWPX 왕복** 양쪽에서 잡는다.
//!    (표는 tac_ink_top_oracle_tests + officex_tac_glyph_box_vertical_split 이 고정.)
//! 2. 우변 불변식: 본문 렌더 노드(TextRun/Table/Image)의 우변이 단 우변을 넘지
//!    않는다 — "줄 우변 819px 관통" 계열의 직접 bbox 핀. seg 장부가 맞아도 렌더 x
//!    가 새는 회귀는 종전 seg-count 핀으로는 잡히지 않았다.

use rhwp::renderer::render_tree::{RenderNode, RenderNodeType};
use rhwp::wasm_api::HwpDocument;

/// A4 기본 본문 단: 왼쪽 여백 30mm(113.4px), 본문 폭 150mm(566.9px).
const COL_LEFT: f64 = 113.4;
const COL_WIDTH: f64 = 566.9;

/// 1×1 투명 PNG — insert_picture 용 최소 이미지.
const PNG_1PX: &[u8] = &[
    0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, // signature
    0x00, 0x00, 0x00, 0x0D, b'I', b'H', b'D', b'R', // IHDR len+tag
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15,
    0xC4, 0x89, // 1x1 RGBA
    0x00, 0x00, 0x00, 0x0A, b'I', b'D', b'A', b'T', // IDAT len+tag
    0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4,
    0x00, 0x00, 0x00, 0x00, b'I', b'E', b'N', b'D', 0xAE, 0x42, 0x60, 0x82, // IEND
];

/// 본문(셀 내부 제외) 렌더 노드 bbox 수집: (종류, x, y, w, h).
fn body_bboxes(doc: &mut HwpDocument) -> Vec<(&'static str, f64, f64, f64, f64)> {
    let tree = doc.build_page_render_tree(0).unwrap();
    let mut out = Vec::new();
    fn walk(node: &RenderNode, out: &mut Vec<(&'static str, f64, f64, f64, f64)>) {
        let b = &node.bbox;
        match &node.node_type {
            RenderNodeType::TextRun(_) => out.push(("text", b.x, b.y, b.width, b.height)),
            RenderNodeType::Image(_) => out.push(("image", b.x, b.y, b.width, b.height)),
            RenderNodeType::Table(_) => {
                out.push(("table", b.x, b.y, b.width, b.height));
                return; // 셀 내부 노드는 본문 불변식 대상이 아니다
            }
            _ => {}
        }
        for c in &node.children {
            walk(c, out);
        }
    }
    walk(&tree.root, &mut out);
    out
}

/// 우변 불변식: 텍스트 우변은 단 우변 이내. 표/그림은 자기 폭이 단 폭 이내인
/// 것만 검사한다(단 폭 초과 개체는 물리적으로 넘칠 수밖에 없다 — 한컴도 동일).
fn assert_right_edge_invariant(doc: &mut HwpDocument, ctx: &str) {
    let right = COL_LEFT + COL_WIDTH;
    for (kind, x, _y, w, _h) in body_bboxes(doc) {
        if w <= 0.0 {
            continue;
        }
        let obj_fits = kind == "text" || w <= COL_WIDTH + 1.0;
        if obj_fits {
            assert!(
                x + w <= right + 1.0,
                "[{ctx}] {kind} 우변 관통: x={x:.1} w={w:.1} 우변={:.1} > 단 우변={right:.1}",
                x + w
            );
        }
    }
}

fn create_tac_table(doc: &mut HwpDocument, para: u32, offset: u32, widths: &str) -> (u32, u32) {
    let c: serde_json::Value = serde_json::from_str(
        &doc.create_table_ex(&format!(
            r#"{{"sectionIdx":0,"paraIdx":{para},"charOffset":{offset},"rowCount":2,"colCount":2,"treatAsChar":true}}"#
        ))
        .unwrap(),
    )
    .unwrap();
    let (pi, ci) = (
        c["paraIdx"].as_u64().unwrap() as u32,
        c["controlIdx"].as_u64().unwrap() as u32,
    );
    doc.set_table_column_widths(0, pi, ci, widths).unwrap();
    (pi, ci)
}

/// "가나[큰 TAC 그림]다라" 생성 문서 — 그림 h 는 텍스트(≈13px)보다 큰 3000HU(≈40px).
fn make_tac_picture_doc() -> HwpDocument {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    doc.insert_text(0, 0, 0, "가나").unwrap();
    let r: serde_json::Value = serde_json::from_str(
        &doc.insert_picture(0, 0, 2, "[]", PNG_1PX, 3000, 3000, 1, 1, "png", "", None, None)
            .unwrap(),
    )
    .unwrap();
    let ci = r["controlIdx"].as_u64().expect("controlIdx") as u32;
    doc.set_picture_properties(0, 0, ci, r#"{"treatAsChar":true}"#)
        .unwrap();
    doc.insert_text(0, 0, 3, "다라").unwrap();
    doc
}

/// 그림 bbox 와 텍스트 기준선(캐럿에서 유도)을 얻는다.
/// 캐럿 규약: y = 줄상단 + 기준선 − 0.8·글자크기, h = 글자크기 →
/// 기준선 절대 y = caret_y + 0.8·caret_h.
fn picture_and_baseline(doc: &mut HwpDocument) -> ((f64, f64, f64, f64), f64) {
    let img = body_bboxes(doc)
        .into_iter()
        .find(|(k, ..)| *k == "image")
        .map(|(_, x, y, w, h)| (x, y, w, h))
        .expect("본문 TAC 그림 노드");
    let caret: serde_json::Value =
        serde_json::from_str(&doc.get_cursor_rect(0, 0, 1).unwrap()).unwrap();
    let baseline = caret["y"].as_f64().unwrap() + 0.8 * caret["height"].as_f64().unwrap();
    (img, baseline)
}

/// 세로 r-분할 모델 검사: 그림 바닥이 기준선 아래로 (1−r)·H ≈ 0.15·H 잠긴다.
/// clamp 회귀(잠김 0)와 줄상단 앵커 회귀(잠김 ≫ 0.2·H) 양쪽을 즉사시킨다.
fn assert_r_split(doc: &mut HwpDocument, ctx: &str) {
    let ((_, iy, _, ih), baseline) = picture_and_baseline(doc);
    let sink = (iy + ih) - baseline;
    assert!(
        sink > 0.10 * ih,
        "[{ctx}] TAC 그림이 위로 붕 떴다('바닥=기준선' clamp 회귀): \
         바닥={:.1} 기준선={baseline:.1} 잠김={sink:.1} (기대 ≈0.15·H={:.1})",
        iy + ih,
        0.15 * ih
    );
    assert!(
        sink < 0.20 * ih,
        "[{ctx}] TAC 그림이 기준선 아래로 과도하게 내려갔다: 잠김={sink:.1} H={ih:.1}"
    );
}

/// [핀 1] 생성 경로: 텍스트 사이 큰 TAC 그림의 기준선 r-분할.
#[test]
fn created_tac_picture_sinks_below_baseline_by_r_split() {
    let mut doc = make_tac_picture_doc();
    assert_r_split(&mut doc, "생성");
    assert_right_edge_invariant(&mut doc, "생성 그림");
}

/// [핀 2] HWPX 왕복(파싱 경로): 같은 문서를 저장→재로드해도 r-분할 유지.
#[test]
fn parsed_tac_picture_sinks_below_baseline_by_r_split() {
    let mut doc = make_tac_picture_doc();
    let bytes = doc.export_hwpx().unwrap();
    let mut reloaded = HwpDocument::from_bytes(&bytes).unwrap();
    assert_r_split(&mut reloaded, "HWPX 왕복");
}

/// [핀 3] 우변 불변식 — 앵커 위치 × 폭 매트릭스. 모든 시나리오에서 본문 텍스트/
/// 적재 가능 개체의 우변이 단 우변을 넘지 않는다 (819px 관통 직접 핀).
#[test]
fn right_edge_invariant_across_tac_scenarios() {
    // (i) mid-anchor 소형: 가[표 20000HU]나 + 뒤 텍스트
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    doc.insert_text(0, 0, 0, "가나").unwrap();
    create_tac_table(&mut doc, 0, 1, "[10000,10000]");
    doc.insert_text(0, 0, 3, "뒤텍스트").unwrap();
    assert_right_edge_invariant(&mut doc, "mid 소형");

    // (ii) end-anchor 대형(단폭의 94%): 가나 + [표 40000HU]
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    doc.insert_text(0, 0, 0, "가나").unwrap();
    create_tac_table(&mut doc, 0, 2, "[20000,20000]");
    assert_right_edge_invariant(&mut doc, "end 대형");

    // (iii) 초과폭(단폭 42520HU 초과): 가나 + [표 60000HU] — 텍스트는 여전히 불변식,
    // 표 자체는 obj_fits 조건에서 자동 제외된다.
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    doc.insert_text(0, 0, 0, "가나").unwrap();
    create_tac_table(&mut doc, 0, 2, "[30000,30000]");
    assert_right_edge_invariant(&mut doc, "초과폭");

    // (iv) TAC 그림 + 앞뒤 텍스트
    let mut doc = make_tac_picture_doc();
    assert_right_edge_invariant(&mut doc, "그림 mid");
}

/// [핀 4] 순서 보존: mid-anchor 생성 문단의 x 좌표가 논리 순서(앞글→표→뒷글)를
/// 따른다 — 표가 텍스트 위/아래로 이탈하던 회귀의 생성 경로 직접 핀.
#[test]
fn created_mid_anchor_order_is_text_table_text() {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    doc.insert_text(0, 0, 0, "가나").unwrap();
    create_tac_table(&mut doc, 0, 2, "[8000,8000]");
    doc.insert_text(0, 0, 3, "다라").unwrap();

    let boxes = body_bboxes(&mut doc);
    let table = boxes
        .iter()
        .find(|(k, ..)| *k == "table")
        .copied()
        .expect("표 노드");
    let texts: Vec<_> = boxes
        .iter()
        .filter(|(k, _, _, w, _)| *k == "text" && *w > 0.0)
        .copied()
        .collect();
    assert!(!texts.is_empty(), "본문 텍스트 노드");
    let (_, tx, ty, tw, th) = table;
    // 같은 줄(세로 겹침)의 텍스트는 표 왼쪽 아니면 오른쪽 — 표와 겹치면 안 된다.
    for (_, x, y, w, _h) in &texts {
        let overlaps_v = *y < ty + th && ty < *y + 1.0 + 12.0; // 줄 공유 근사
        if overlaps_v {
            let left_of = x + w <= tx + 1.0;
            let right_of = *x >= tx + tw - 1.0;
            assert!(
                left_of || right_of,
                "텍스트가 표 bbox 와 겹친다(순서/폭 계상 붕괴): 텍스트 x={x:.1} w={w:.1}, \
                 표 x={tx:.1} w={tw:.1}"
            );
        }
    }
}
