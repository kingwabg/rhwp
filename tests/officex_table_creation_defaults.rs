//! 표 만들기 기본값 핀 — 한컴 실측 정합 (2026-08-11 사용자 지정).
//!
//! · 테두리 굵기 0.12mm (BORDER_WIDTHS 인덱스 1)
//! · 바깥 여백 상하좌우 1.00mm (283 HWPUNIT)
//! · 안 여백 좌·우 1.80mm (510 HU), 위·아래 0.50mm (142 HU)
//!
//! 종전엔 ① 안 여백 상하가 141HU(0.497mm) ② 테두리가 "굵기 인덱스 ≥ 1 인 아무
//! BorderFill 재사용"이라 문서에 굵은 테두리가 있으면 기본 표가 두껍게 생성됐다.

use rhwp::wasm_api::HwpDocument;

/// HWPUNIT → mm (7200 HWPUNIT = 1 inch = 25.4mm)
fn hu_mm(hu: f64) -> f64 {
    hu * 25.4 / 7200.0
}

fn json_num(json: &str, key: &str) -> f64 {
    let pat = format!("\"{}\":", key);
    let s = json.find(&pat).unwrap_or_else(|| panic!("키 {key} 없음: {json}")) + pat.len();
    let rest = &json[s..];
    let e = rest
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
        .unwrap_or(rest.len());
    rest[..e].parse().expect("숫자 파싱")
}

fn blank_doc() -> HwpDocument {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("samples/officex_blank.hwpx");
    let bytes = std::fs::read(&path).expect("read blank fixture");
    HwpDocument::from_bytes(&bytes).expect("parse blank fixture")
}

#[test]
fn created_table_uses_hancom_default_margins_and_border() {
    let mut doc = blank_doc();
    let res = doc.create_table(0, 0, 0, 2, 2).expect("표 생성");
    let para = json_num(&res, "paraIdx") as u32;
    let ctrl = json_num(&res, "controlIdx") as u32;

    // 표 속성: 바깥 여백 1.00mm
    let tp = doc.get_table_properties(0, para, ctrl).expect("표 속성");
    for key in ["outerLeft", "outerRight", "outerTop", "outerBottom"] {
        let mm = hu_mm(json_num(&tp, key));
        assert!(
            (mm - 1.00).abs() < 0.01,
            "바깥 여백 {key} = {mm:.3}mm, 기대 1.00mm — {tp}"
        );
    }

    // 셀 속성: 안 여백 좌·우 1.80mm / 위·아래 0.50mm, 테두리 굵기 인덱스 1
    let cp = doc.get_cell_properties(0, para, ctrl, 0).expect("셀 속성");
    for key in ["paddingLeft", "paddingRight"] {
        let mm = hu_mm(json_num(&cp, key));
        assert!(
            (mm - 1.80).abs() < 0.01,
            "안 여백 {key} = {mm:.3}mm, 기대 1.80mm — {cp}"
        );
    }
    for key in ["paddingTop", "paddingBottom"] {
        let mm = hu_mm(json_num(&cp, key));
        assert!(
            (mm - 0.50).abs() < 0.01,
            "안 여백 {key} = {mm:.3}mm, 기대 0.50mm — {cp}"
        );
    }
}

/// 문서에 굵은 테두리가 이미 있어도 기본 표는 0.12mm 로 만들어져야 한다.
/// (종전엔 "인덱스 ≥ 1 인 아무 BorderFill 재사용"이라 굵은 값을 물려받았다.)
#[test]
fn created_table_border_width_is_012mm() {
    let mut doc = blank_doc();
    let res = doc.create_table(0, 0, 0, 2, 2).expect("표 생성");
    let para = json_num(&res, "paraIdx") as u32;
    let ctrl = json_num(&res, "controlIdx") as u32;
    let cp = doc.get_cell_properties(0, para, ctrl, 0).expect("셀 속성");

    // borderLeft/Right/Top/Bottom 의 width 는 굵기 **인덱스** — 1 = 0.12mm
    for side in ["borderLeft", "borderRight", "borderTop", "borderBottom"] {
        let pat = format!("\"{}\":", side);
        let s = cp.find(&pat).unwrap_or_else(|| panic!("{side} 없음: {cp}")) + pat.len();
        let obj = &cp[s..];
        let w = json_num(obj, "width");
        assert_eq!(
            w as i64, 1,
            "{side} 굵기 인덱스 = {w} (기대 1 = 0.12mm) — {cp}"
        );
    }
}
