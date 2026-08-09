//! [트랙3] 옆 공간 임계 통일 핀 — 한컴 실측 최소 34px(MIN_SIDE_PX)로 layout(종전 40)과
//! composer(34)를 통일한다. 옆 공간 ≈36px 문서: 부분폭 밴드 + 옆 흐름 on(후속 문단
//! 줄이 표 세로 구간 안에서 좁게 선다). ≈33px 표본: 전폭 접힘 유지 — 경계 양측 고정.
use rhwp::renderer::render_tree::{RenderNode, RenderNodeType};
use rhwp::wasm_api::HwpDocument;

const FILLER: &str = "가나다라마바사 아자차카타파하 강물이 흐르고 산이 높다 바람이 분다";
/// 표 바깥 여백(생성 기본 283HU) 좌+우 px — 밴드 폭에 포함되는 값.
const OUTER_LR_PX: f64 = (283.0 + 283.0) / 7200.0 * 96.0;

/// [텍스트][빈 host 표][빈 이웃][텍스트×2] 구조를 만들고 (doc, 표 pi, ci)를 돌려준다.
fn build() -> (HwpDocument, u32, u32) {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    let flen = FILLER.chars().count();
    doc.insert_text(0, 0, 0, FILLER).unwrap();
    for i in 0..2u32 {
        doc.split_paragraph_native(0, i as usize, flen).unwrap();
        doc.insert_text(0, i + 1, 0, FILLER).unwrap();
    }
    let c: serde_json::Value = serde_json::from_str(&doc.create_table_ex(
        r#"{"sectionIdx":0,"paraIdx":1,"charOffset":0,"rowCount":3,"colCount":1,"treatAsChar":false,"colWidths":[30000]}"#
    ).unwrap()).unwrap();
    let (pi, ci) = (
        c["paraIdx"].as_u64().unwrap() as u32,
        c["controlIdx"].as_u64().unwrap() as u32,
    );
    doc.set_table_properties(0, pi, ci,
        r#"{"treatAsChar":false,"textWrap":"Square","vertRelTo":"Para","horzRelTo":"Column","horzAlign":"Left","vertOffset":0,"horzOffset":0}"#
    ).unwrap();
    (doc, pi, ci)
}

fn column_width(doc: &HwpDocument) -> f64 {
    let tree = doc.build_page_render_tree(0).unwrap();
    fn walk(n: &RenderNode) -> Option<f64> {
        if matches!(n.node_type, RenderNodeType::Column { .. }) {
            return Some(n.bbox.width);
        }
        n.children.iter().find_map(walk)
    }
    walk(&tree.root).expect("Column 노드 없음")
}

fn table_bbox(doc: &HwpDocument, pi: u32, ci: u32) -> (f64, f64, f64, f64) {
    let bb: serde_json::Value =
        serde_json::from_str(&doc.get_table_bbox(0, pi, ci).unwrap()).unwrap();
    (
        bb["x"].as_f64().unwrap(),
        bb["y"].as_f64().unwrap(),
        bb["width"].as_f64().unwrap(),
        bb["height"].as_f64().unwrap(),
    )
}

/// 본문 TextLine(표/글상자 내부 제외) 목록 (y, x, w).
fn body_lines(doc: &HwpDocument) -> Vec<(f64, f64, f64)> {
    let mut out = Vec::new();
    for pg in 0..doc.page_count() {
        let tree = doc.build_page_render_tree(pg).unwrap();
        fn walk(n: &RenderNode, out: &mut Vec<(f64, f64, f64)>) {
            if matches!(
                n.node_type,
                RenderNodeType::Table(_) | RenderNodeType::TextBox
            ) {
                return;
            }
            if let RenderNodeType::TextLine(tl) = &n.node_type {
                if tl.para_index.is_some() {
                    out.push((n.bbox.y, n.bbox.x, n.bbox.width));
                }
            }
            for c in &n.children {
                walk(c, out);
            }
        }
        walk(&tree.root, &mut out);
    }
    out
}

/// 표 폭을 "옆 공간 target_px"가 되도록 조정한다.
fn set_side_room(doc: &mut HwpDocument, pi: u32, ci: u32, target_px: f64) {
    let col_w = column_width(doc);
    let table_w_px = col_w - OUTER_LR_PX - target_px;
    let width_hu = (table_w_px * 7200.0 / 96.0).round() as u32;
    doc.set_table_column_widths(0, pi, ci, &format!("[{width_hu}]"))
        .unwrap();
}

#[test]
fn side_room_36px_flows_text_beside_table() {
    let (mut doc, pi, ci) = build();
    set_side_room(&mut doc, pi, ci, 36.0);
    let (tx, ty, tw, th) = table_bbox(&doc, pi, ci);

    // 옆 흐름 on: 표 세로 구간 안에서 표 오른쪽에 시작하는 좁은 본문 줄 존재.
    let side: Vec<_> = body_lines(&doc)
        .into_iter()
        .filter(|(y, x, _)| *y >= ty - 2.0 && *y < ty + th && *x >= tx + tw - 1.0)
        .collect();
    assert!(
        !side.is_empty(),
        "옆 공간 36px 인데 표 옆 본문 줄이 없다 — [34,40) 구간 전폭 접힘 잔재 (임계 통일 실패)"
    );
    // 부분폭: 옆 줄 폭이 옆 공간(≈36px) 스케일 — 전폭(≈수백px)이 아니다.
    // 글리프 fit 톨러런스로 마지막 글자가 수 px 넘칠 수 있어 상한은 여유를 둔다.
    for (y, x, w) in &side {
        assert!(
            *w <= 55.0,
            "옆 줄(y={y:.1} x={x:.1}) 폭 {w:.1}px 이 옆 공간 스케일(≈36px)을 벗어남 — 전폭 잔재"
        );
    }
    // 후속 문단 line_segs 에 좁힘 흔적(cs>0) — 밴드 기준 재줄바꿈이 기록됐다.
    let narrowed = (0..doc.get_paragraph_count(0).unwrap()).any(|p| {
        let segs: Vec<(i32, i32, u32)> =
            serde_json::from_str(&doc.debug_line_seg_tags(0, p).unwrap()).unwrap();
        segs.iter().any(|s| s.1 > 0)
    });
    assert!(narrowed, "옆 흐름인데 어느 문단에도 좁힘 cs 기록이 없다");
}

#[test]
fn side_room_33px_keeps_full_width_fold() {
    let (mut doc, pi, ci) = build();
    set_side_room(&mut doc, pi, ci, 33.0);
    let (tx, ty, tw, th) = table_bbox(&doc, pi, ci);
    let _ = (tx, tw);

    // 전폭 접힘 유지: 표 세로 구간 안에서 시작하는 본문 줄이 없다 (34px 미만).
    let side: Vec<_> = body_lines(&doc)
        .into_iter()
        .filter(|(y, _, _)| *y + 2.0 > ty && *y < ty + th - 1.0)
        .collect();
    assert!(
        side.is_empty(),
        "옆 공간 33px(<34) 인데 표 세로 구간 안에 본문 줄 — 경계 하한 붕괴. side={side:?}"
    );
}
