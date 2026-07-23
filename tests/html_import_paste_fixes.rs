//! [paste-import] pasteHtml 표 서식 수리 검증 (sc- QA paste-import.qa.test.ts 대응).
//! 셀 인라인 스타일/굵기·%폭·applyInnerMargin·ragged 채움·caption·HTML4 속성·img 자리표시.

use rhwp::wasm_api::HwpDocument;
use serde_json::Value;

fn paste_table(html: &str) -> HwpDocument {
    let mut doc = HwpDocument::create_empty();
    doc.paste_html(0, 0, 0, html)
        .unwrap_or_else(|e| panic!("paste_html: {e:?}"));
    doc
}

fn props(doc: &HwpDocument, cell: u32) -> Value {
    let s = doc
        .get_cell_properties(0, 0, 0, cell)
        .unwrap_or_else(|e| panic!("get_cell_properties: {e:?}"));
    serde_json::from_str(&s).unwrap()
}

fn char_props(doc: &HwpDocument, cell: usize) -> Value {
    let s = doc
        .get_cell_char_properties_at(0, 0, 0, cell, 0, 0)
        .unwrap_or_else(|e| panic!("get_cell_char_properties_at: {e:?}"));
    serde_json::from_str(&s).unwrap()
}

fn para_props(doc: &HwpDocument, cell: usize) -> Value {
    let s = doc
        .get_cell_para_properties_at(0, 0, 0, cell, 0)
        .unwrap_or_else(|e| panic!("get_cell_para_properties_at: {e:?}"));
    serde_json::from_str(&s).unwrap()
}

fn dims(doc: &HwpDocument) -> Value {
    let s = doc
        .get_table_dimensions(0, 0, 0)
        .unwrap_or_else(|e| panic!("get_table_dimensions: {e:?}"));
    serde_json::from_str(&s).unwrap()
}

// 주: 셀 글자/문단 서식 보존(cell_inline_char_style·text_align)은 앱 결재 양식 워크어라운드
// (restoreApprovalCellFormat)와 충돌해 되돌렸다 — 해당 회귀 테스트 2건 제거. 구조 수정만 검증.

#[test]
fn percent_width_is_honored() {
    let doc = paste_table(
        r#"<table style="width:100%;table-layout:fixed"><tr><td style="width:14%">가</td><td style="width:86%">나</td></tr></table>"#,
    );
    let w0 = props(&doc, 0)["width"].as_u64().unwrap();
    let w1 = props(&doc, 1)["width"].as_u64().unwrap();
    assert_ne!(w0, w1, "% 폭이 균등분할되지 않아야 함: {w0},{w1}");
    // 14:86 비율 근사 (42520 기준 5952:36568)
    assert!(w0 > 5000 && w0 < 7000, "14% ≒ 5952, 실제 {w0}");
    assert!(w1 > 35000 && w1 < 38000, "86% ≒ 36568, 실제 {w1}");
}

#[test]
fn padding_sets_apply_inner_margin() {
    let doc = paste_table(r#"<table><tr><td style="padding:30px">P</td></tr></table>"#);
    let p = props(&doc, 0);
    assert_eq!(p["applyInnerMargin"], Value::Bool(true));
    assert!(p["paddingLeft"].as_i64().unwrap() > 2000, "padding 30px ≒ 2250");
}

#[test]
fn ragged_rows_filled_to_rectangle() {
    let doc = paste_table(r#"<table><tr><td>1</td><td>2</td><td>3</td></tr><tr><td>4</td></tr></table>"#);
    let d = dims(&doc);
    assert_eq!(d["rowCount"], serde_json::json!(2));
    assert_eq!(d["colCount"], serde_json::json!(3));
    assert_eq!(d["cellCount"], serde_json::json!(6), "구멍 없이 2x3=6셀");
}

#[test]
fn colspan_overflow_no_holes() {
    let doc = paste_table(r#"<table><tr><td colspan="9">C</td></tr><tr><td>a</td><td>b</td></tr></table>"#);
    let d = dims(&doc);
    // colCount는 브라우저처럼 9로 확장되되 둘째 행 구멍(7칸)이 빈 셀로 메워져야 한다.
    let cols = d["colCount"].as_u64().unwrap();
    let cells = d["cellCount"].as_u64().unwrap();
    assert_eq!(cols, 9);
    assert_eq!(cells, 1 + 9, "1(row0) + 9(row1: 2실+7채움) = 10");
}

#[test]
fn html4_presentational_attrs() {
    let doc = paste_table(
        r##"<table bgcolor="#ff0000" border="1" width="500"><tr><td bgcolor="#00ff00" align="center" width="100">셀</td><td>둘</td></tr></table>"##,
    );
    let p = props(&doc, 0);
    assert_eq!(p["fillType"], serde_json::json!("solid"), "bgcolor 반영");
    // 주: align="center"(문단 정렬)는 앱 복원 패스가 맡으므로 여기선 검증하지 않는다(셀 서식 보존 되돌림).
    let w = p["width"].as_u64().unwrap();
    assert!(w > 6000 && w < 9000, "width=100(px) ≒ 7500, 실제 {w}");
}

#[test]
fn caption_text_preserved() {
    let doc = paste_table(
        r#"<table><caption>표제목</caption><tr><td>a</td><td>b</td></tr></table>"#,
    );
    // 표는 para 0, 캡션은 뒤 문단으로 보존
    let pc = doc.get_paragraph_count(0).unwrap_or_else(|e| panic!("{e:?}"));
    let mut found = false;
    for p in 0..pc {
        if let Ok(t) = doc.get_text_range(0, p, 0, 40) {
            if t.contains("표제목") {
                found = true;
            }
        }
    }
    assert!(found, "caption 텍스트가 어딘가 남아야 함");
}

#[test]
fn img_in_paragraph_leaves_placeholder() {
    let mut doc = HwpDocument::create_empty();
    doc.paste_html(
        0,
        0,
        0,
        r#"<p>앞<img src="data:image/png;base64,iVBORw0KGgo=" width="10" height="10">뒤</p>"#,
    )
    .unwrap_or_else(|e| panic!("{e:?}"));
    let t = doc.get_text_range(0, 0, 0, 40).unwrap_or_default();
    assert_ne!(t, "앞뒤", "이미지가 조용히 버려지면 안 됨: {t}");
    assert!(t.contains("이미지"), "자리표시 텍스트 기대, 실제 {t}");
}
