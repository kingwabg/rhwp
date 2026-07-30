//! [표 폭 지정 2026-07-30] createTableEx 의 colWidths/rowHeights 가 **떠 있는 표**에서
//! 통째로 버려지던 결함의 회귀 잠금.
//!
//! 원인: create_table_ex_native 가 `treat_as_char=false` 이면 폭·높이 인자를 버리고
//! create_table_native 로 위임했고, 그 함수는 단 폭 균등 분할만 했다. 그래서 어울림·
//! 자리차지 표의 폭을 지정할 수단이 아예 없었다(테스트조차 "칸 지우기"로 폭을 흉내냄).
use rhwp::wasm_api::HwpDocument;

fn table_width(doc: &HwpDocument, pi: u32, ci: u32) -> u64 {
    let tp: serde_json::Value =
        serde_json::from_str(&doc.get_table_properties(0, pi, ci).unwrap()).unwrap();
    tp["tableWidth"].as_u64().unwrap()
}

fn make(json: &str) -> (HwpDocument, u32, u32) {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    let c: serde_json::Value =
        serde_json::from_str(&doc.create_table_ex(json).unwrap()).unwrap();
    let (pi, ci) = (
        c["paraIdx"].as_u64().unwrap() as u32,
        c["controlIdx"].as_u64().unwrap() as u32,
    );
    (doc, pi, ci)
}

/// 떠 있는 표(비-TAC)도 colWidths 를 그대로 반영한다.
#[test]
fn floating_table_honors_col_widths() {
    let (doc, pi, ci) = make(
        r#"{"sectionIdx":0,"paraIdx":0,"charOffset":0,"rowCount":2,"colCount":2,"treatAsChar":false,"colWidths":[3000,5000]}"#,
    );
    assert_eq!(table_width(&doc, pi, ci), 8000, "colWidths 합이 표 폭이어야 한다");
    // 열마다 다른 폭이 실제 셀에 반영됐는지 — 첫 두 셀 폭 비교
    let bb: serde_json::Value =
        serde_json::from_str(&doc.get_table_cell_bboxes(0, pi, ci, None).unwrap()).unwrap();
    let cells = bb.as_array().unwrap();
    let w0 = cells[0]["w"].as_f64().unwrap();
    let w1 = cells[1]["w"].as_f64().unwrap();
    assert!(
        w1 > w0 + 10.0,
        "열별 폭이 반영되지 않았다: w0={w0:.0} w1={w1:.0}"
    );
}

/// 글자처럼취급 표도 종전대로 반영한다(기존 동작 보존).
#[test]
fn tac_table_honors_col_widths() {
    let (doc, pi, ci) = make(
        r#"{"sectionIdx":0,"paraIdx":0,"charOffset":0,"rowCount":1,"colCount":2,"treatAsChar":true,"colWidths":[2000,4000]}"#,
    );
    assert_eq!(table_width(&doc, pi, ci), 6000);
}

/// colWidths 를 안 주면 종전 그대로 단 폭 균등 분할(전폭).
#[test]
fn omitted_col_widths_keeps_full_column_fit() {
    let (doc, pi, ci) = make(
        r#"{"sectionIdx":0,"paraIdx":0,"charOffset":0,"rowCount":2,"colCount":2,"treatAsChar":false}"#,
    );
    assert!(
        table_width(&doc, pi, ci) > 30000,
        "지정 없으면 단 폭에 맞춰야 한다"
    );
}

/// 방어: 열 수와 길이가 다르거나 최소폭 미만이면 거부(떠 있는 표에서도).
/// ⚠ wasm 래퍼는 Err(JsValue) 를 돌려주는데 JsValue 는 네이티브에서 다룰 수 없다 —
/// 네이티브 함수로 검증한다.
#[test]
fn floating_table_rejects_bad_col_widths() {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    assert!(
        doc.create_table_ex_native(0, 0, 0, 2, 3, false, Some(&[3000, 3000]), None)
            .is_err(),
        "열 수 불일치가 통과했다"
    );
    assert!(
        doc.create_table_ex_native(0, 0, 0, 2, 2, false, Some(&[100, 3000]), None)
            .is_err(),
        "최소폭 미만이 통과했다"
    );
}
