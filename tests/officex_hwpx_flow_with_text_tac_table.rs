//! [법칙 2 / 이중 진실 해소] HWPX `flowWithText="0"` 글자취급 표의 조판 라우팅.
//!
//! `Table.attr` 은 `CommonObjAttr` FLAGS 의 미러이고, 물리는
//! `Table.common.treat_as_char` 다. HWPX 파서(`materialize_hwpx_table_attrs`)는
//! 미러를 `treat_as_char && flow_with_text` 로 게이트해 **불완전하게** 만들었다
//! (Task #1100 `571101050`). `flowWithText="0"` 인 글자취급 표는 미러 bit0=0 →
//! 조판 라우터(`paragraph_has_table`)와 TAC 판정이 미러를 읽던 시절엔 이 표를
//! **블록 표로 오판**해 표가 흐름 y 를 전진시키고, 같은 줄을 공유해야 할 후행
//! 텍스트가 표 아래로 쌓였다(96d24f4b3 이 편집 경로에서 고친 (a) 증상과 동형).
//!
//! `flowWithText`(= HWP5 attr bit13 = 한컴 "쪽 영역 안으로 제한")은 비-TAC
//! floating 개체의 y 클램프·밀어내기 스위치이고, 글자취급 표의 인라인 여부와는
//! 무관하다. HWP5·HWP3 파서는 bit0 을 게이트 없이 미러하므로 같은 문서를
//! .hwp 로 저장하면 인라인, .hwpx 로 저장하면 블록이 되는 컨테이너 의존 분기였다.
//!
//! 정적 코퍼스 254개 HWPX 전수 스캔 결과 `flowWithText="0"` 글자취급 표는 108건
//! 있으나 **같은 문단에 후행 텍스트가 있는 표본은 0건**이라, 편집 API 로 합성해
//! HWPX 저장·재로드로 재현한다.
use rhwp::renderer::render_tree::{RenderNode, RenderNodeType};
use rhwp::wasm_api::HwpDocument;

/// 빈 문단 offset 0 에 소형 TAC 표(start-anchor) + offset 1 에 후행 텍스트를 만들고
/// `restrictInPage:false`(= HWPX `pos@flowWithText="0"`)로 저장한 HWPX 바이트.
fn flow_with_text_zero_hwpx() -> Vec<u8> {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    let created: serde_json::Value = serde_json::from_str(
        &doc.create_table_ex(
            r#"{"sectionIdx":0,"paraIdx":0,"charOffset":0,"rowCount":2,"colCount":2,"treatAsChar":true}"#,
        )
        .unwrap(),
    )
    .unwrap();
    let (pi, ci) = (
        created["paraIdx"].as_u64().unwrap() as u32,
        created["controlIdx"].as_u64().unwrap() as u32,
    );
    // 줄폭 안에 넉넉히 들어가는 소형 표 — 폭 기준 줄바꿈이면 후행 텍스트와 한 줄.
    doc.set_table_column_widths(0, pi, ci, "[7087,7087]")
        .unwrap();
    doc.insert_text(0, 0, 1, "다음글").unwrap();
    doc.set_table_properties(0, pi, ci, r#"{"restrictInPage":false}"#)
        .unwrap();
    doc.export_hwpx().unwrap()
}

/// 본문 표 bbox 목록과 본문 TextRun bbox 목록 (셀 내부 비진입).
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

/// 표와 텍스트가 **같은 줄**인가 = 세로 대역이 겹치는가.
/// (인라인 TAC 는 텍스트 baseline 이 표 바닥이라 텍스트 상자가 표 top 보다 살짝
///  위에서 시작할 수 있으므로 포함 관계가 아니라 겹침으로 판정한다.)
fn shares_line(table: (f64, f64, f64, f64), text: (f64, f64, f64, f64)) -> bool {
    text.1 < table.1 + table.3 - 1.0 && text.1 + text.3 > table.1 + 1.0
}

/// 저장 조판: `flowWithText=0` 이어도 표는 "큰 글자"이므로 후행 텍스트와 한 줄(seg 1개).
#[test]
fn hwpx_flow_with_text_zero_tac_table_keeps_single_line_seg() {
    let bytes = flow_with_text_zero_hwpx();
    let reloaded = HwpDocument::from_bytes(&bytes).unwrap();
    let segs: serde_json::Value =
        serde_json::from_str(&reloaded.debug_line_seg_tags(0, 0).unwrap()).unwrap();
    assert_eq!(
        segs.as_array().unwrap().len(),
        1,
        "flowWithText=0 글자취급 표도 후행 텍스트와 한 줄이어야 한다: {segs}"
    );
}

/// 렌더: 표와 후행 텍스트가 같은 줄 — 텍스트가 표 오른쪽에, 세로 대역을 공유.
#[test]
fn hwpx_flow_with_text_zero_tac_table_renders_text_beside_table() {
    let bytes = flow_with_text_zero_hwpx();
    let mut reloaded = HwpDocument::from_bytes(&bytes).unwrap();
    let (tbs, txs) = tables_and_texts(&mut reloaded);
    assert_eq!(tbs.len(), 1, "본문 표 1개여야 한다: {tbs:?}");
    assert_eq!(txs.len(), 1, "본문 텍스트 1개여야 한다: {txs:?}");
    let (tx, ty, tw, th) = tbs[0];
    let (xx, xy, _xw, xh) = txs[0];
    assert!(
        xx >= tx + tw - 1.0,
        "후행 텍스트는 표 오른쪽에 놓여야 한다: 표 right={} 텍스트 x={xx}",
        tx + tw
    );
    assert!(
        shares_line(tbs[0], txs[0]),
        "후행 텍스트는 표와 세로 대역을 공유(같은 줄)해야 한다: \
         표 y=[{ty}..{}] 텍스트 y=[{xy}..{}]",
        ty + th,
        xy + xh
    );
}

/// 컨테이너 무관: 같은 문서를 HWP5 로 저장·재로드해도 같은 조판이어야 한다.
/// (미러 게이트 시절엔 HWPX 만 블록으로 갈라졌다.)
#[test]
fn hwp5_and_hwpx_roundtrips_agree_on_flow_with_text_zero_tac_table() {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    let created: serde_json::Value = serde_json::from_str(
        &doc.create_table_ex(
            r#"{"sectionIdx":0,"paraIdx":0,"charOffset":0,"rowCount":2,"colCount":2,"treatAsChar":true}"#,
        )
        .unwrap(),
    )
    .unwrap();
    let (pi, ci) = (
        created["paraIdx"].as_u64().unwrap() as u32,
        created["controlIdx"].as_u64().unwrap() as u32,
    );
    doc.set_table_column_widths(0, pi, ci, "[7087,7087]")
        .unwrap();
    doc.insert_text(0, 0, 1, "다음글").unwrap();
    doc.set_table_properties(0, pi, ci, r#"{"restrictInPage":false}"#)
        .unwrap();

    let mut geometry = Vec::new();
    for bytes in [doc.export_hwpx().unwrap(), doc.export_hwp().unwrap()] {
        let mut reloaded = HwpDocument::from_bytes(&bytes).unwrap();
        let (tbs, txs) = tables_and_texts(&mut reloaded);
        assert_eq!(tbs.len(), 1);
        assert_eq!(txs.len(), 1);
        // 표 대비 텍스트의 상대 배치만 비교 (포맷별 절대 좌표 미세차 무시).
        geometry.push((
            txs[0].0 >= tbs[0].0 + tbs[0].2 - 1.0,
            shares_line(tbs[0], txs[0]),
        ));
    }
    assert_eq!(
        geometry[0], geometry[1],
        "HWPX·HWP5 재로드 조판이 갈라지면 안 된다: hwpx={:?} hwp={:?}",
        geometry[0], geometry[1]
    );
    assert_eq!(
        geometry[0],
        (true, true),
        "두 포맷 모두 후행 텍스트가 표 오른쪽·같은 줄이어야 한다"
    );
}

/// 미러 정합: HWPX 파스가 만든 `Table.attr` bit0 은 물리와 일치해야 한다.
/// `set_table_properties` 가 이 값을 raw_ctrl_data FLAGS 로 되쓰므로
/// (table_ops.rs:2875) 어긋나 있으면 표 속성을 한 번 만지는 순간 저장 파일에서
/// 글자처럼취급 비트가 유실된다 — 게이트 시절 실측 `attr=0x0` / `tac=true`.
#[test]
fn hwpx_mirror_bit0_matches_physics() {
    use rhwp::model::control::Control;
    let bytes = flow_with_text_zero_hwpx();
    let doc = HwpDocument::from_bytes(&bytes).unwrap();
    let tables: Vec<_> = doc.document().sections[0]
        .paragraphs
        .iter()
        .flat_map(|p| p.controls.iter())
        .filter_map(|c| match c {
            Control::Table(t) => Some(t),
            _ => None,
        })
        .collect();
    assert_eq!(tables.len(), 1, "표 1개를 찾아야 한다");
    let t = tables[0];
    assert!(
        t.common.treat_as_char,
        "물리(treat_as_char)가 저장 왕복에서 유실됐다"
    );
    assert!(!t.common.flow_with_text, "flowWithText=0 이 유실됐다");
    assert_eq!(
        t.attr & 0x01,
        0x01,
        "미러 bit0 이 물리와 어긋난다 (attr={:#x})",
        t.attr
    );
}
