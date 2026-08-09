//! [트랙4 ② / design_track4 스텝7 1단계] 부분폭 어울림 표 단독 + 인접 문단 표 —
//! 현행 동작 characterization 핀.
//!
//! **코퍼스는 판별 표본 부재** (oracle-corpus-mining-20260805 (e): 유일 후보
//! issue1891 은 사실상 전폭 표라 40%+40% 병치 여부에 답하지 못함) — 한컴 정답은
//! 대화형 실측 대기. 이 핀은 정답 주장 없이 **현행 실측을 그대로 고정**해 lane
//! 게이트(para_float_lanes count<2) 이관 작업의 회귀 안전벨트로 쓴다.
//!
//! 실측(현행): 빈 문단 p0 좌측 40% Square 표, p1 우측 40% Square 표, p2 텍스트.
//! - 두 표는 같은 top 병치도, 표 높이만큼 스택도 아니다 — **호스트 문단 줄 순서대로
//!   한 줄 높이(1600HU≈21.3px)씩 어긋난 배치**. x 구간은 서로 소.
//! - 흐름 전진은 표 높이가 아니라 호스트 빈 문단 줄 높이 단위: p2 텍스트 첫 줄은
//!   p0 표 top 기준 두 줄(≈42.7px) 아래에서 시작한다.
use rhwp::renderer::render_tree::{RenderNode, RenderNodeType};
use rhwp::wasm_api::HwpDocument;

const LINE_PX: f64 = 1600.0 / 7200.0 * 96.0; // 빈 문단 한 줄 = 1600HU ≈ 21.33px

fn build() -> (
    Vec<(f64, f64, f64, f64)>,
    Vec<(f64, f64, f64, f64)>,
    HwpDocument,
) {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    doc.insert_paragraph(0, 0).unwrap();
    doc.insert_paragraph(0, 0).unwrap();
    for (para, align) in [(0u32, "Left"), (1, "Right")] {
        let c: serde_json::Value = serde_json::from_str(
            &doc.create_table_ex(&format!(
                r#"{{"sectionIdx":0,"paraIdx":{para},"charOffset":0,"rowCount":2,"colCount":1,"treatAsChar":false}}"#,
            ))
            .unwrap(),
        )
        .unwrap();
        let (pi, ci) = (
            c["paraIdx"].as_u64().unwrap() as u32,
            c["controlIdx"].as_u64().unwrap() as u32,
        );
        doc.set_table_column_widths(0, pi, ci, "[17000]").unwrap();
        doc.set_table_properties(0, pi, ci, &format!(
            r#"{{"treatAsChar":false,"textWrap":"Square","vertRelTo":"Para","horzRelTo":"Column","horzAlign":"{align}","vertOffset":0,"horzOffset":0}}"#,
        ))
        .unwrap();
    }
    doc.insert_text(0, 2, 0, "후속 문단 텍스트").unwrap();
    let tree = doc.build_page_render_tree(0).unwrap();
    let mut tbs = Vec::new();
    let mut txs = Vec::new();
    fn walk(
        n: &RenderNode,
        tbs: &mut Vec<(f64, f64, f64, f64)>,
        txs: &mut Vec<(f64, f64, f64, f64)>,
    ) {
        if matches!(n.node_type, RenderNodeType::Table(_)) {
            tbs.push((n.bbox.x, n.bbox.y, n.bbox.width, n.bbox.height));
            return;
        }
        if let RenderNodeType::TextRun(tr) = &n.node_type {
            if !tr.text.trim().is_empty() {
                txs.push((n.bbox.x, n.bbox.y, n.bbox.width, n.bbox.height));
            }
        }
        for c in &n.children {
            walk(c, tbs, txs);
        }
    }
    walk(&tree.root, &mut tbs, &mut txs);
    (tbs, txs, doc)
}

/// 인접 문단의 좌/우 40% 어울림 표 두 장: 현행은 병치도 전면 스택도 아닌
/// "호스트 줄 순서 한 줄 어긋남" — 그대로 고정.
#[test]
fn adjacent_float_tables_offset_by_one_host_line() {
    let (tbs, _txs, _doc) = build();
    assert_eq!(tbs.len(), 2, "어울림 표 2개 렌더: {tbs:?}");
    let (t0, t1) = (tbs[0], tbs[1]);
    // x 구간 서로 소(좌/우 정렬) — 시각 충돌 없음.
    assert!(
        t1.0 >= t0.0 + t0.2 - 1.0,
        "우측 표가 좌측 표 x 구간과 겹치면 안 된다: t0={t0:?} t1={t1:?}"
    );
    // 세로: 같은 top(병치)도 아니고 t0 아래(표 높이 스택)도 아닌 한 줄 어긋남.
    let dy = t1.1 - t0.1;
    assert!(
        (dy - LINE_PX).abs() <= 3.0,
        "현행 고정: 두 번째 표 top = 첫 표 top + 한 줄({LINE_PX:.1}px) — 실측 dy={dy:.2}"
    );
    assert!(
        t1.1 < t0.1 + t0.3,
        "현행 고정: 두 표의 y 구간은 겹친다(전면 세로 스택 아님): t0={t0:?} t1={t1:?}"
    );
}

/// 후속 문단 흐름 전진: 표 높이 소비가 아니라 호스트 빈 문단 줄 높이 2줄만큼.
#[test]
fn following_paragraph_advances_by_host_lines_not_table_height() {
    let (tbs, txs, doc) = build();
    let t0 = tbs[0];
    let (_, xy, ..) = txs[0];
    let dy = xy - t0.1;
    assert!(
        (dy - 2.0 * LINE_PX).abs() <= 3.0,
        "현행 고정: 후속 텍스트 top = 첫 표 top + 두 줄({:.1}px) — 실측 dy={dy:.2}",
        2.0 * LINE_PX
    );
    // 저장 조판(vpos)도 같은 그림: 문단당 seg 1개, vpos 가 한 줄씩 전진.
    for (pi, expect_vpos) in [(0u32, 0.0f64), (1, 1600.0), (2, 3200.0)] {
        let segs: serde_json::Value =
            serde_json::from_str(&doc.debug_line_seg_tags(0, pi).unwrap()).unwrap();
        let arr = segs.as_array().unwrap();
        assert_eq!(arr.len(), 1, "p{pi} seg 1개: {segs}");
        let vpos = arr[0].as_array().unwrap()[0].as_f64().unwrap();
        assert!(
            (vpos - expect_vpos).abs() <= 100.0,
            "p{pi} vpos 현행 고정: 기대 {expect_vpos} 실측 {vpos}"
        );
    }
}
