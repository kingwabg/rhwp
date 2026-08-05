//! [트랙4 ⑥ / oracle-corpus-mining-20260805 (h)] start-anchor·다중 TAC 표 —
//! 코퍼스 법칙 핀 승격.
//!
//! 코퍼스 확정(정적 637파일): start-anchor·다중 TAC 표에 **자기 줄 시멘틱은 없다**.
//! 자기 줄/인라인/병치는 전부 폭 기준 줄바꿈의 귀결(12법칙 #4 "TAC 표는 큰 글자다").
//! - 소형 start-anchor: 표+후행 텍스트 seg 1개 동거 (exam_science s0#61)
//! - 다중 소형(합폭≤줄폭): seg 1개, 1줄 가로 병치 (calendar_year s0#4)
//! - 대형(폭>줄폭) start-anchor: 표 자기 줄 + 후행 텍스트 다음 줄 = seg 2개
//!   (2025년 기부·답례품 실적 지자체 보고서_양식 s0#25)
//!
//! 현행 엔진(end_anchored_solo_tac_table 이 start-anchor·다중에 None → 기존 인라인
//! 유지)의 **저장 조판**은 소형·다중에서 코퍼스와 정합(seg 1개)이다. 어긋남 3건은
//! 고치지 않고 어긋남 명시 characterization 으로 고정(수리는 별도 결정):
//! - (a) 렌더 세로 데싱크: 소형 start-anchor 의 후행 텍스트가 표와 같은 줄(seg 1)
//!   인데 렌더는 텍스트 상자를 표 아래(46px 데싱크)에 그림 — line_seg/composed
//!   데싱크 실증 문서(0747e431a) 계열.
//! - (b) 다중 소형 렌더: 코퍼스는 1줄 가로 병치인데 렌더는 같은 x 에 세로 스택.
//! - (c) 대형 start-anchor: 코퍼스는 seg 2개(표 줄+텍스트 줄)인데 현행은 seg 1개
//!   유지 + 표가 줄폭을 넘겨 우측으로 넘침.
use rhwp::renderer::render_tree::{RenderNode, RenderNodeType};
use rhwp::wasm_api::HwpDocument;

/// 빈 문단 offset 0 에 TAC 표 생성(start-anchor) 후 오프셋 1(표 뒤)에 텍스트.
fn make_start_anchor_doc(col_widths: &str) -> HwpDocument {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    let c: serde_json::Value = serde_json::from_str(
        &doc.create_table_ex(
            r#"{"sectionIdx":0,"paraIdx":0,"charOffset":0,"rowCount":2,"colCount":2,"treatAsChar":true}"#,
        )
        .unwrap(),
    )
    .unwrap();
    let (pi, ci) = (
        c["paraIdx"].as_u64().unwrap() as u32,
        c["controlIdx"].as_u64().unwrap() as u32,
    );
    doc.set_table_column_widths(0, pi, ci, col_widths).unwrap();
    doc.insert_text(0, 0, 1, "다음글").unwrap();
    doc
}

fn seg_count(doc: &HwpDocument) -> usize {
    let segs: serde_json::Value =
        serde_json::from_str(&doc.debug_line_seg_tags(0, 0).unwrap()).unwrap();
    segs.as_array().unwrap().len()
}

/// 렌더 트리의 (본문 표 bbox 목록, 본문 TextRun bbox 목록). 셀 내부는 제외.
fn tables_and_texts(
    doc: &mut HwpDocument,
) -> (Vec<(f64, f64, f64, f64)>, Vec<(f64, f64, f64, f64)>) {
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
            return; // 셀 내부 비진입
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
    (tbs, txs)
}

/// (a) start-anchor 소형: 표+후행 텍스트 같은 줄(seg 1개) — 코퍼스 exam_science 동형.
/// end-anchor 자기 줄 규칙(officex_tac_end_anchor_own_line)이 start-anchor 로
/// 번지지 않아야 한다.
#[test]
fn start_anchor_small_table_shares_line_with_following_text() {
    let mut doc = make_start_anchor_doc("[7087,7087]");
    assert_eq!(
        seg_count(&doc),
        1,
        "start-anchor 소형은 표+텍스트 seg 1개 동거(코퍼스 exam_science s0#61)"
    );
    let (tbs, txs) = tables_and_texts(&mut doc);
    assert_eq!(tbs.len(), 1, "본문 표 1개");
    let (tx, _ty, tw, _th) = tbs[0];
    let (xx, _xy, _xw, _xh) = txs[0];
    // 같은 줄 순서: 텍스트는 표 오른쪽에서 시작(가로 인라인 동거).
    assert!(
        (xx - (tx + tw)).abs() <= 1.5,
        "후행 텍스트는 표 바로 오른쪽에서 시작해야 한다: 표 우변={} 텍스트 x={xx}",
        tx + tw
    );
    // 코퍼스 어긋남(보고 대상, 수리 별도): 렌더는 텍스트 상자를 표 아래(46px
    // 데싱크)에 그린다 — 같은 줄 baseline 공유가 아님. 여기서는 y 를 핀하지 않는다.
}

/// (b) 다중 소형 TAC 표 3개: 합폭≤줄폭 → 저장 조판 seg 1개 — 코퍼스 calendar_year 동형.
#[test]
fn three_small_tac_tables_stay_on_single_line_seg() {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    for off in 0..3u32 {
        let c: serde_json::Value = serde_json::from_str(
            &doc.create_table_ex(&format!(
                r#"{{"sectionIdx":0,"paraIdx":0,"charOffset":{off},"rowCount":1,"colCount":1,"treatAsChar":true}}"#,
            ))
            .unwrap(),
        )
        .unwrap();
        let (pi, ci) = (
            c["paraIdx"].as_u64().unwrap() as u32,
            c["controlIdx"].as_u64().unwrap() as u32,
        );
        doc.set_table_column_widths(0, pi, ci, "[8000]").unwrap();
    }
    assert_eq!(
        seg_count(&doc),
        1,
        "다중 소형 TAC(합폭≤줄폭)은 seg 1개(코퍼스 calendar_year s0#4 — 1줄 가로 병치)"
    );
    let (tbs, _) = tables_and_texts(&mut doc);
    assert_eq!(tbs.len(), 3, "표 3개 전부 렌더");
    // 코퍼스 어긋남(보고 대상, 수리 별도): 코퍼스는 1줄 가로 병치(같은 y, 다른 x)
    // 인데 현행 렌더는 같은 x(줄 시작)에 표 높이 간격 세로 스택으로 그린다.
    // 현행 characterization: 세 표 모두 줄 시작 x 에서 시작.
    let x0 = tbs[0].0;
    assert!(
        tbs.iter().all(|(x, ..)| (x - x0).abs() < 1.0),
        "현행 렌더 고정(어긋남 명시): 세 표가 같은 x 세로 스택 — 코퍼스는 가로 병치. {tbs:?}"
    );
    // 저장 왕복(HWPX): 저장 조판 seg 1개 보존.
    let bytes = doc.export_hwpx().unwrap();
    let reloaded = HwpDocument::from_bytes(&bytes).unwrap();
    assert_eq!(seg_count(&reloaded), 1, "HWPX 왕복 후에도 seg 1개");
}

/// (c) 대형(폭>줄폭) start-anchor — 현행 characterization.
/// 코퍼스(기부 양식 s0#25·aift s0#0)는 폭 초과 → seg 2개(표 자기 줄 + 후행 텍스트
/// 다음 줄)인데, 현행 엔진은 폭 기준 줄바꿈을 하지 않고 seg 1개를 유지한다.
/// **코퍼스 어긋남 — 고치지 않고 현행 고정, 수리는 별도 결정.**
#[test]
fn start_anchor_wide_table_currently_keeps_single_seg_diverges_from_corpus() {
    let mut doc = make_start_anchor_doc("[25000,25000]");
    // 현행: seg 1개 (코퍼스 기대: 2개 — 표 줄 lh=h+外마진, 텍스트 줄).
    assert_eq!(
        seg_count(&doc),
        1,
        "현행 고정(어긋남 명시): 대형 start-anchor 도 seg 1개 유지 — 코퍼스는 seg 2개"
    );
    let (tbs, txs) = tables_and_texts(&mut doc);
    assert_eq!(tbs.len(), 1);
    // 현행 렌더 characterization: 후행 텍스트가 줄 시작 x 로 내려가고 표는 텍스트
    // 폭만큼 밀려 줄폭을 넘긴다(우측 넘침) — 코퍼스는 표가 줄 시작, 텍스트가 다음 줄.
    let (tx, _ty, _tw, _th) = tbs[0];
    let (xx, ..) = txs[0];
    assert!(
        xx < tx,
        "현행 고정(어긋남 명시): 텍스트 x({xx}) < 표 x({tx}) — 코퍼스와 역순"
    );
}

/// (d) 저장 왕복(HWPX): start-anchor 소형·대형 모두 line_seg 수 보존.
#[test]
fn start_anchor_line_segs_survive_hwpx_roundtrip() {
    for widths in ["[7087,7087]", "[25000,25000]"] {
        let doc = make_start_anchor_doc(widths);
        let before = seg_count(&doc);
        let bytes = doc.export_hwpx().unwrap();
        let reloaded = HwpDocument::from_bytes(&bytes).unwrap();
        assert_eq!(
            seg_count(&reloaded),
            before,
            "HWPX 왕복 후 line_seg 수 보존 실패 (widths={widths})"
        );
    }
}
