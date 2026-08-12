//! [2026-08-12 신고 "표 경계선이 또 이상해졌다"] 어긋내기(offset_cell_boundary)와
//! 표 높이 동기화 핀.
//!
//! 결함 ①: effective_row_heights 의 글줄 바닥(2026-08-11)이 어긋내기가 만든 **조각 행**
//! (걸침 span 셀이 있는 행)까지 1284HU 로 부풀려 common.height 가 실제(2852)보다 크게
//! (5136) 기록됐다. 조각 행은 저장 높이가 곧 실효 높이다 — 걸침 셀 존재 시 바닥 미적용.
//! 결함 ②: offset/restore 가 update_ctrl_dimensions 를 부르지 않아 common.height 가
//! 이전 값에 고착됐다(복원해도 부풀림이 남음).

use rhwp::wasm_api::HwpDocument;

fn table_h(doc: &HwpDocument) -> (u32, Vec<u32>, Vec<u32>) {
    for para in &doc.document().sections[0].paragraphs {
        for ctrl in &para.controls {
            if let rhwp::model::control::Control::Table(t) = ctrl {
                return (
                    t.common.height,
                    t.get_row_heights(),
                    t.effective_row_heights(),
                );
            }
        }
    }
    panic!("표 없음");
}

fn make_staggerable() -> (HwpDocument, u32, u32) {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    let c: serde_json::Value = serde_json::from_str(
        &doc.create_table_ex(
            r#"{"sectionIdx":0,"paraIdx":0,"charOffset":0,"rowCount":3,"colCount":3,"treatAsChar":false}"#,
        )
        .unwrap(),
    )
    .unwrap();
    let (pi, ci) = (
        c["paraIdx"].as_u64().unwrap() as u32,
        c["controlIdx"].as_u64().unwrap() as u32,
    );
    doc.resize_table_cells(
        0,
        pi,
        ci,
        r#"[{"cellIdx":0,"heightDelta":2000},{"cellIdx":1,"heightDelta":2000},{"cellIdx":2,"heightDelta":2000}]"#,
    )
    .unwrap();
    (doc, pi, ci)
}

/// 조각 행(걸침 셀이 있는 행)에는 글줄 바닥을 적용하지 않는다.
#[test]
fn stagger_fragment_rows_are_not_inflated() {
    let (mut doc, pi, ci) = make_staggerable();
    let (h_before, _raw0, _eff0) = table_h(&doc);
    doc.offset_cell_boundary(0, pi, ci, 1, "bottom", -800)
        .unwrap();
    let (h, raw, eff) = table_h(&doc);
    // 조각 행 = raw[1](분할 상반 잔여)·raw[2](분할 하반) — eff 가 raw 그대로여야 한다
    assert_eq!(
        eff[1], raw[1],
        "조각 행이 부풀었다: raw={raw:?} eff={eff:?}"
    );
    assert_eq!(
        eff[2], raw[2],
        "조각 행이 부풀었다: raw={raw:?} eff={eff:?}"
    );
    // common.height 는 eff 합과 일치(스테일 금지)
    let sum: u32 = eff.iter().sum();
    assert_eq!(h, sum, "어긋내기 후 common.height({h}) ≠ eff 합({sum})");
    // 종전 결함(부풀림 5136) 재발 방지 — 어긋내기는 자기 축만: 표 높이 불변.
    // (구 `h < 5000` 매직 상수는 raw 델타가 실효 밑절미를 얻은 2026-08-13 수치와 안 맞음)
    assert_eq!(h, h_before, "어긋내기가 표 높이를 바꿨다: {h_before} → {h}");
}

/// 어긋내기 → 복원 왕복 후 common.height 가 eff 합으로 갱신된다(스테일 금지).
#[test]
fn restore_resyncs_common_height() {
    let (mut doc, pi, ci) = make_staggerable();
    doc.offset_cell_boundary(0, pi, ci, 1, "bottom", -800)
        .unwrap();
    doc.restore_cell_boundary(0, pi, ci, 1, "bottom").unwrap();
    let (h, raw, eff) = table_h(&doc);
    assert_eq!(raw.len(), 3, "복원 후 행 수가 3이 아니다: {raw:?}");
    let sum: u32 = eff.iter().sum();
    assert_eq!(
        h, sum,
        "복원 후 common.height({h})가 eff 합({sum})으로 재동기화되지 않았다"
    );
}

/// 원 결함(2026-08-11 옆글자) 비회귀: 빈 규약 행(걸침 셀 없음)은 여전히 바닥 적용.
#[test]
fn plain_empty_rows_still_get_line_floor() {
    let (doc, _pi, _ci) = make_staggerable();
    let (_h, raw, eff) = table_h(&doc);
    assert_eq!(raw[1], 284, "빈 행 저장 규약(패딩만)");
    assert!(
        eff[1] >= 1284,
        "빈 규약 행 바닥이 사라졌다(옆글자 결함 재발): eff={eff:?}"
    );
}
