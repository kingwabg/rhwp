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
//! 유지)의 **저장 조판**은 소형·다중에서 코퍼스와 정합(seg 1개)이다.
//! - (a) 소형 start-anchor 렌더 세로 데싱크 — **수리 완료**. 원인은 조판 라우터가
//!   아니라 편집 생성 표의 `Table.attr`(=CommonObjAttr FLAGS 미러) 오설정이었다:
//!   bit0(글자처럼취급)이 0 이라 `paragraph_has_table` 이 인라인 TAC 표를 블록 표로
//!   오판 → 표가 `PageItem::Table` 로 따로 나가 흐름 y 를 전진시키고, 같은 줄의
//!   후행 텍스트가 그 아래에 그려졌다.
//! - (b) 다중 소형 렌더 세로 스택 — **수리 완료**(같은 원인, 같은 수리).
//! - (c) 대형 start-anchor: 코퍼스는 seg 2개(표 줄+텍스트 줄)인데 현행은 seg 1개
//!   유지 + 표가 줄폭을 넘겨 우측으로 넘침 — **어긋남 명시 유지**(폭 기준 줄바꿈
//!   생산이 별도 과제).
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
    let (tx, ty, tw, th) = tbs[0];
    let (xx, xy, _xw, xh) = txs[0];
    // 같은 줄 순서: 텍스트는 표 오른쪽에서 시작(가로 인라인 동거).
    assert!(
        (xx - (tx + tw)).abs() <= 1.5,
        "후행 텍스트는 표 바로 오른쪽에서 시작해야 한다: 표 우변={} 텍스트 x={xx}",
        tx + tw
    );
    // 한 줄 공유(코퍼스): 텍스트 상자가 표와 세로로 겹친다 — 표 아래로 내려가면 안 된다.
    let overlap = (ty + th).min(xy + xh) - ty.max(xy);
    assert!(
        overlap > 0.0,
        "후행 텍스트가 표와 같은 줄에 있어야 한다(세로 겹침): 표 y=[{ty},{}] 텍스트 y=[{xy},{}]",
        ty + th,
        xy + xh
    );
    // 코퍼스 횡단법칙: 뒤 텍스트 baseline = 표 바닥. TextRun bbox 높이가 baseline
    // 거리이므로 baseline = xy + xh 이고, 표 세로 구간 안에 있어야 한다.
    // (정확한 일치는 저장 line_height 가 표 바깥여백 상하를 포함해야 성립 — 현행
    //  줄높이는 표 본체 높이라 baseline 이 표 바닥에서 바깥여백만큼 위에 clamp 된다.
    //  이 잔차는 line_seg 생산(줄높이) 과제 몫이라 여기선 구간으로만 핀한다.)
    let baseline = xy + xh;
    assert!(
        baseline > ty && baseline <= ty + th + 0.5,
        "텍스트 baseline({baseline})은 표 세로 구간[{ty},{}] 안이어야 한다",
        ty + th
    );
}

/// (a-2) 같은 줄 공유의 캐럿·클릭 정합. 표 뒤 오프셋의 캐럿과 표 우측 클릭이
/// 가리키는 위치가 같아야 한다 — 렌더가 세로로 갈리면 둘이 45px 어긋났다.
#[test]
fn start_anchor_caret_and_click_agree_on_shared_line() {
    let mut doc = make_start_anchor_doc("[7087,7087]");
    let (tbs, _) = tables_and_texts(&mut doc);
    let (tx, ty, tw, th) = tbs[0];

    let caret: serde_json::Value =
        serde_json::from_str(&doc.get_cursor_rect(0, 0, 1).unwrap()).unwrap();
    let (cx, cy, ch) = (
        caret["x"].as_f64().unwrap(),
        caret["y"].as_f64().unwrap(),
        caret["height"].as_f64().unwrap(),
    );
    assert!(
        (cx - (tx + tw)).abs() <= 1.5,
        "표 뒤(offset 1) 캐럿은 표 우변에 서야 한다: 표 우변={} 캐럿 x={cx}",
        tx + tw
    );
    assert!(
        cy >= ty - 0.5 && cy + ch <= ty + th + 0.5,
        "표 뒤 캐럿은 표와 같은 줄 안에 있어야 한다: 표 y=[{ty},{}] 캐럿 y=[{cy},{}]",
        ty + th,
        cy + ch
    );
    assert!(
        ch < th - 1.0,
        "표 뒤 캐럿 높이는 표 높이가 아니라 텍스트 줄 높이여야 한다: 캐럿 h={ch} 표 h={th}"
    );

    // 표 우측 클릭 → 표 뒤 오프셋, 그리고 그 커서 위치가 캐럿 API 와 같은 줄.
    let click = doc
        .hit_test_native(0, tx + tw + 5.0, ty + th / 2.0)
        .unwrap();
    let hit: serde_json::Value = serde_json::from_str(&click).unwrap();
    assert_eq!(
        hit["charOffset"].as_u64().unwrap(),
        1,
        "표 우측 클릭은 표 뒤 오프셋이어야 한다: {hit}"
    );
    let hy = hit["cursorRect"]["y"].as_f64().unwrap();
    assert!(
        (hy - cy).abs() <= 2.0,
        "클릭 커서 y({hy})와 캐럿 API y({cy})가 어긋나면 안 된다"
    );
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
    // 코퍼스(calendar_year s0#4 · request s0#14): 1줄 가로 병치 — 같은 y, x 는
    // 앞 표 폭만큼 전진.
    let y0 = tbs[0].1;
    assert!(
        tbs.iter().all(|(_, y, ..)| (y - y0).abs() < 1.0),
        "세 표가 한 줄(같은 y)에 나란히 서야 한다: {tbs:?}"
    );
    for w in tbs.windows(2) {
        let (px, _, pw, _) = w[0];
        let (nx, ..) = w[1];
        assert!(
            nx >= px + pw - 0.5,
            "다음 표는 앞 표 오른쪽에서 시작해야 한다(겹침 금지): 앞 우변={} 다음 x={nx}",
            px + pw
        );
    }
    let (lx, _, lw, _) = tbs[2];
    assert!(
        lx + lw <= 113.4 + 566.9 + 0.5,
        "합폭이 줄폭 안이므로 세 표가 본문 폭을 넘지 않아야 한다: 마지막 우변={}",
        lx + lw
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
