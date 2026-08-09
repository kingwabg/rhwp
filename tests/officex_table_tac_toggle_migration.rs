//! [officex] 표 '글자처럼취급' 토글 마이그레이션 핀.
//!
//! 그림(picture.rs)·도형(shape.rs) setter 는 TAC 토글 시 rel_to=Para·offset=0 리셋을
//! 수행하는데 **표 setter 만 물리 비트만 바꾸고 끝났다** (table_ops.rs treatAsChar).
//! 시각 편입은 물리 기반 라우팅 + setter 꼬리 recompose/refresh 가 이미 처리했지만
//! (2026-08-09 실측으로 확인), 드래그로 옮겨 둔 표를 TAC 로 켜면 rel_to·offset 이
//! 남아 — 다시 끄는 순간 옛 오프셋으로 점프하고 저장 파일에도 남았다.
//! 토글 경로는 무핀이었다: 생성 시 TAC 핀(officex_tac_start_anchor_and_multi)이
//! 못 잡는 갭. 아래 첫 테스트의 오프셋 리셋 단언이 판별 핀(수리 전 red 확인).

use rhwp::renderer::render_tree::{RenderNode, RenderNodeType};
use rhwp::wasm_api::HwpDocument;

/// 렌더 트리의 (본문 표 bbox, 본문 TextRun bbox 목록). 셀 내부 비진입.
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
    (tbs, txs)
}

/// 편집기 실사용 흐름: 자리차지 표 생성 → 속성에서 글자처럼취급 ON → 표 뒤에 타이핑.
/// 생성 시 TAC 표 핀(officex_tac_start_anchor_and_multi)과 동일하게 텍스트가
/// 표 오른쪽 같은 줄에 앉아야 한다.
#[test]
fn tac_toggle_on_puts_following_text_right_of_table() {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    let c: serde_json::Value = serde_json::from_str(
        &doc.create_table_ex(
            r#"{"sectionIdx":0,"paraIdx":0,"charOffset":0,"rowCount":2,"colCount":2,"treatAsChar":false}"#,
        )
        .unwrap(),
    )
    .unwrap();
    let (pi, ci) = (
        c["paraIdx"].as_u64().unwrap() as u32,
        c["controlIdx"].as_u64().unwrap() as u32,
    );
    doc.set_table_column_widths(0, pi, ci, "[7087,7087]")
        .unwrap();
    // 자리차지 표는 인라인 슬롯을 차지하지 않으므로 같은 문단 offset 0 에 텍스트 입력
    // — 토글 시점에 호스트 문단에 텍스트가 이미 있는 실사용 상태를 만든다.
    doc.insert_text(0, pi, 0, "다음글").unwrap();
    // 드래그로 옮겨 둔 자리차지 표를 흉내: 오프셋이 남아 있는 상태에서 토글해야
    // 리셋 누락이 드러난다 (기본값 0 이면 리셋 부재가 가려짐).
    doc.set_table_properties(0, pi, ci, r#"{"vertOffset":5000,"horzOffset":3000}"#)
        .unwrap();

    // 편집기 경로: 속성 대화상자에서 글자처럼취급 ON.
    doc.set_table_properties(0, pi, ci, r#"{"treatAsChar":true}"#)
        .unwrap();

    // 한컴 산출물(samples/tac-verify Scenario A~D)과 동일: rel_to=Para·offset=0 리셋.
    let props: serde_json::Value =
        serde_json::from_str(&doc.get_table_properties(0, pi, ci).unwrap()).unwrap();
    assert_eq!(props["treatAsChar"], true);
    assert_eq!(
        props["vertRelTo"], "Para",
        "TAC 전환 시 vert_rel_to=Para 리셋"
    );
    assert_eq!(
        props["horzRelTo"], "Para",
        "TAC 전환 시 horz_rel_to=Para 리셋"
    );
    assert_eq!(props["vertOffset"], 0, "TAC 전환 시 세로 오프셋 0 리셋");
    assert_eq!(props["horzOffset"], 0, "TAC 전환 시 가로 오프셋 0 리셋");

    // 렌더: 표 앵커가 텍스트 뒤(end-anchor 형)이므로 [텍스트][표] 같은 줄 인라인 —
    // officex_tac_end_anchor_own_line 의 소형 표 기하와 동형.
    let (tbs, txs) = tables_and_texts(&mut doc);
    assert_eq!(tbs.len(), 1, "본문 표 1개");
    assert!(!txs.is_empty(), "텍스트 렌더됨");
    let (tx, ty, _tw, th) = tbs[0];
    let (xx, xy, xw, xh) = txs[0];
    assert!(
        (tx - (xx + xw)).abs() <= 1.5,
        "토글 TAC: 표는 텍스트 바로 오른쪽에서 시작해야 한다(같은 줄 인라인): 텍스트 우변={} 표 x={tx}",
        xx + xw
    );
    let overlap = (ty + th).min(xy + xh) - ty.max(xy);
    assert!(
        overlap > 0.0,
        "토글 TAC: 표와 텍스트가 같은 줄이어야 한다(세로 겹침): 표 y=[{ty},{}] 텍스트 y=[{xy},{}]",
        ty + th,
        xy + xh
    );
}

/// 실파일 회귀 핀: 한컴 저장본의 TAC 표를 글자처럼취급 OFF 로 토글하면 호스트 문단이
/// 재조판되어 표가 float 배치로 이동하고, 저장 왕복이 화면과 일치해야 한다.
/// (현행 정상 동작의 잠금 — 물리 라우팅·refresh 를 미러 게이트로 되돌리는 회귀 방지.)
#[test]
fn tac_toggle_off_on_loaded_file_rebreaks_host_paragraph() {
    let bytes = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("samples/tac-case-001.hwp"),
    )
    .expect("read samples/tac-case-001.hwp");
    let mut doc = HwpDocument::from_bytes(&bytes).expect("load");

    // 본문에서 TAC 표 문단을 찾는다 (Deref → DocumentCore::document()).
    let (pi, ci) = {
        let mut found = None;
        'outer: for (pi, para) in doc.document().sections[0].paragraphs.iter().enumerate() {
            for (ci, ctrl) in para.controls.iter().enumerate() {
                if matches!(ctrl, rhwp::model::control::Control::Table(t) if t.common.treat_as_char)
                {
                    found = Some((pi as u32, ci as u32));
                    break 'outer;
                }
            }
        }
        found.expect("본문 TAC 표 없음")
    };
    let before = tables_and_texts(&mut doc);

    doc.set_table_properties(0, pi, ci, r#"{"treatAsChar":false}"#)
        .unwrap();

    let after = tables_and_texts(&mut doc);
    // 인라인(줄 안 잉크 top) → float(문단 상단 앵커)로 표 위치가 이동하고 텍스트가
    // 재조판되어야 한다. 실측: 표 y 157.4→132.3, 텍스트 런 5→9개.
    assert_ne!(
        before.0, after.0,
        "TAC OFF 토글이 호스트 문단을 재조판해 표 배치가 바뀌어야 한다 — 그대로면 \
         저장 line_segs 가 권위로 남아 편집기 토글이 화면에 반영되지 않는 것"
    );

    // 저장 왕복: 토글 후 재조판 없이는 낡은 저장 line_segs(토글 전 인라인 줄 구조)가
    // .hwp 로 그대로 나가, 재열람 시 화면과 파일이 어긋난다. 재파스 렌더가
    // 토글 후 화면과 일치해야 한다.
    let saved = doc.export_hwp_with_adapter().expect("export .hwp");
    let mut reloaded = HwpDocument::from_bytes(&saved).expect("re-parse");
    let round = tables_and_texts(&mut reloaded);
    let (ax, ay, ..) = after.0[0];
    let (rx, ry, ..) = round.0[0];
    assert!(
        (ax - rx).abs() <= 1.5 && (ay - ry).abs() <= 1.5,
        "저장 왕복 후 표 배치가 토글 후 화면과 일치해야 한다: 화면=({ax},{ay}) 재파스=({rx},{ry}) \
         — 어긋나면 낡은 line_segs 가 저장된 것"
    );
}
