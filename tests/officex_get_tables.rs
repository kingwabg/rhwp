//! getTables — 구역 안 표 열거(서식 규정 검사가 문서 전체를 훑는 데 필요).
//!
//! 왜 API 를 새로 만들었나: 형제 API 들은 표 위치를 이미 알 때 쓰는 것이라, 문서 전체를
//! 훑는 쪽은 문단×컨트롤을 무작정 찔러 예외로 판별해야 했다 — 느리고 "표 아님"과
//! "범위 초과"를 구분하지 못한다.
use rhwp::wasm_api::HwpDocument;

fn mk_table(doc: &mut HwpDocument, para_idx: usize, rows: u32, cols: u32) {
    doc.create_table_ex(&format!(
        r#"{{"sectionIdx":0,"paraIdx":{para_idx},"charOffset":0,"rowCount":{rows},"colCount":{cols},"treatAsChar":true}}"#
    ))
    .unwrap();
}

#[test]
fn get_tables_enumerates_every_table() {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    doc.insert_text(0, 0, 0, "첫 문단").unwrap();
    mk_table(&mut doc, 0, 2, 3);
    let len = doc.get_paragraph_length(0, 0).unwrap() as usize;
    doc.split_paragraph_native(0, 0, len).unwrap();
    doc.insert_text(0, 1, 0, "둘째").unwrap();
    mk_table(&mut doc, 1, 4, 5);

    let json = doc.get_tables_native(0).unwrap();
    assert_eq!(json.matches("\"controlIdx\"").count(), 2, "표 2개: {json}");
    assert!(
        json.contains("\"rowCount\":2") && json.contains("\"colCount\":3"),
        "첫 표 2×3: {json}"
    );
    assert!(
        json.contains("\"rowCount\":4") && json.contains("\"colCount\":5"),
        "둘째 표 4×5: {json}"
    );
}

#[test]
fn get_tables_is_empty_without_tables() {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    doc.insert_text(0, 0, 0, "표 없는 문서").unwrap();
    assert_eq!(doc.get_tables_native(0).unwrap(), "[]");
}

#[test]
fn get_tables_rejects_bad_section() {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    assert!(doc.get_tables_native(99).is_err(), "범위 밖 구역은 오류");
}
