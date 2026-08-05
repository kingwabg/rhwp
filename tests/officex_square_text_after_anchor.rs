//! [트랙3] 앵커선행 어울림 host — 표 컨트롤이 문단 맨 앞(앞은 공백뿐)이고 뒤에 본문
//! 텍스트가 있는 host 문단의 옆 흐름 핀.
//!
//! 계약: layout 의 square 팔이 앵커선행(text_is_blank_before_control)으로 확장돼
//! host post-text 가 table_y_before(표 top)에서 시작하고, 편집 훅의 host 자격도 같은
//! 판정으로 완화돼 host 자신의 line_segs 가 밴드 기준 cs/sw 로 좁혀진다.
//! TopAndBottom 으로 되돌리면 전폭·표 아래로 원복된다.
use rhwp::renderer::render_tree::{RenderNode, RenderNodeType};
use rhwp::wasm_api::HwpDocument;

const FILLER: &str =
    "가나다라마바사 아자차카타파하 강물이 흐르고 산이 높다 바람이 분다 하늘은 푸르고 바다는 깊다";

/// 앵커선행 host 문서: [텍스트][host: 표앵커+텍스트][텍스트] 구조.
fn build() -> (HwpDocument, u32, u32) {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    let flen = FILLER.chars().count();
    doc.insert_text(0, 0, 0, FILLER).unwrap();
    for i in 0..2u32 {
        doc.split_paragraph_native(0, i as usize, flen).unwrap();
        doc.insert_text(0, i + 1, 0, FILLER).unwrap();
    }
    // 문단1 맨 앞에 어울림 표(좁은 1열) — 자기 빈 문단 앵커 + 아래 빈 이웃 문단 생성.
    let c: serde_json::Value = serde_json::from_str(&doc.create_table_ex(
        r#"{"sectionIdx":0,"paraIdx":1,"charOffset":0,"rowCount":3,"colCount":1,"treatAsChar":false,"colWidths":[15000]}"#
    ).unwrap()).unwrap();
    let (pi, ci) = (
        c["paraIdx"].as_u64().unwrap() as u32,
        c["controlIdx"].as_u64().unwrap() as u32,
    );
    doc.set_table_properties(0, pi, ci,
        r#"{"treatAsChar":false,"textWrap":"Square","vertRelTo":"Para","horzRelTo":"Column","horzAlign":"Left","vertOffset":0,"horzOffset":0}"#
    ).unwrap();
    // 빈 이웃 문단 → 뒤 텍스트 문단을 차례로 병합해 "표앵커 + 텍스트" 한 문단으로.
    doc.merge_paragraph_native(0, pi as usize + 1).unwrap();
    doc.merge_paragraph_native(0, pi as usize + 1).unwrap();
    // 편집 훅 경유 텍스트 입력 — 훅이 host line_segs 를 밴드 기준으로 재생산한다.
    let host_len = doc.get_logical_length(0, pi).unwrap();
    doc.insert_text(0, pi, host_len, " 끝머리 문장").unwrap();
    (doc, pi, ci)
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

/// host 문단의 본문 TextLine 목록 (y, x, w) — line_index 순.
fn host_lines(doc: &HwpDocument, pi: u32) -> Vec<(f64, f64, f64)> {
    let mut out = Vec::new();
    for pg in 0..doc.page_count() {
        let tree = doc.build_page_render_tree(pg).unwrap();
        fn walk(n: &RenderNode, pi: u32, out: &mut Vec<(f64, f64, f64)>) {
            if matches!(n.node_type, RenderNodeType::Table(_)) {
                return; // 셀 내부 제외
            }
            if let RenderNodeType::TextLine(tl) = &n.node_type {
                if tl.para_index == Some(pi as usize) {
                    out.push((n.bbox.y, n.bbox.x, n.bbox.width));
                }
            }
            for c in &n.children {
                walk(c, pi, out);
            }
        }
        walk(&tree.root, pi, &mut out);
    }
    out
}

#[test]
fn anchor_leading_square_host_text_flows_beside_table() {
    let (doc, pi, ci) = build();
    let (tx, ty, tw, th) = table_bbox(&doc, pi, ci);

    // ① host line_segs 에 좁힘 기록(cs>0) — 훅이 host 자신을 재줄바꿈했다.
    let segs: Vec<(i32, i32, u32)> =
        serde_json::from_str(&doc.debug_line_seg_tags(0, pi).unwrap()).unwrap();
    assert!(
        segs.iter().any(|s| s.1 > 0),
        "host line_segs 에 좁힘(cs>0) 기록이 없다 — 훅 host 자격 완화 미작동. segs={segs:?}"
    );

    // ② host 텍스트 line_segs 의 text_start 단조 비감소 (앵커 8코드유닛 갭 재구성 리스크 핀).
    let starts: Vec<u32> = doc
        .document()
        .sections[0]
        .paragraphs[pi as usize]
        .line_segs
        .iter()
        .map(|s| s.text_start)
        .collect();
    assert!(
        starts.windows(2).all(|w| w[0] <= w[1]),
        "host line_segs text_start 비단조 — 앵커 갭 재구성 오류. starts={starts:?}"
    );

    // ③ 렌더트리: host 첫 TextLine 이 표 세로 구간 안(옆 흐름) + 표 상자 밖(오른쪽).
    let lines = host_lines(&doc, pi);
    assert!(!lines.is_empty(), "host TextLine 이 렌더트리에 없다");
    let (y0, x0, _) = lines[0];
    assert!(
        y0 >= ty - 3.0 && y0 < ty + th,
        "host 첫 줄 y={y0:.1} 가 표 세로 구간[{ty:.1},{:.1}] 밖 — 옆 흐름 미작동",
        ty + th
    );
    assert!(
        x0 >= tx + tw - 1.0,
        "host 첫 줄 x={x0:.1} 가 표 상자(오른쪽 끝 {:.1}) 안에서 시작",
        tx + tw
    );

    // ④ 본문 줄이 표 상자 안에서 시작하지 않는다 (어울림 공통 계약).
    for (y, x, _) in &lines {
        let inside_y = *y + 2.0 > ty && *y < ty + th - 1.0;
        assert!(
            !(inside_y && *x < tx + tw - 1.0),
            "host 줄(y={y:.1} x={x:.1})이 표 상자 안에서 시작"
        );
    }
}

#[test]
fn topbottom_revert_restores_full_width_below_table() {
    let (mut doc, pi, ci) = build();
    doc.set_table_properties(0, pi, ci, r#"{"textWrap":"TopAndBottom"}"#)
        .unwrap();
    let (tx, ty, _tw, th) = table_bbox(&doc, pi, ci);

    // 전폭 원복: 좁힘 흔적(cs>0) 소거.
    let segs: Vec<(i32, i32, u32)> =
        serde_json::from_str(&doc.debug_line_seg_tags(0, pi).unwrap()).unwrap();
    assert!(
        segs.iter().all(|s| s.1 == 0),
        "TopAndBottom 복귀 후에도 좁힘 cs 잔재 — 전폭 원복 실패. segs={segs:?}"
    );

    // host 텍스트가 표 아래 전폭으로 — 첫 줄 y ≥ 표 하단, x 는 표 왼쪽 기준선 근방.
    let lines = host_lines(&doc, pi);
    assert!(!lines.is_empty(), "host TextLine 이 렌더트리에 없다");
    let (y0, x0, _) = lines[0];
    assert!(
        y0 >= ty + th - 2.0,
        "TopAndBottom 인데 host 첫 줄 y={y0:.1} 이 표 하단({:.1}) 위 — 옆 흐름 잔재",
        ty + th
    );
    assert!(
        x0 < tx + 20.0,
        "TopAndBottom 인데 host 첫 줄 x={x0:.1} 이 전폭 시작이 아님 (표 x={tx:.1})"
    );
}
