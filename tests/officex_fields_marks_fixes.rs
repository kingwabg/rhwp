//! officex fields-marks 웨이브 수리 회귀 가드.
//! QA 스윕(tests/rhwp-qa/fields-marks.qa.test.ts)의 시나리오를 Rust에서 재현한다.

use rhwp::document_core::DocumentCore;
use serde_json::Value;

fn blank_with_text(text: &str) -> DocumentCore {
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native()
        .expect("create blank document");
    if !text.is_empty() {
        core.insert_text_native(0, 0, 0, text).expect("insert text");
    }
    core
}

// ── 누름틀: 빈 이름 삽입 거부 ────────────────────────────────────────────
#[test]
fn empty_name_clickhere_rejected() {
    let mut core = blank_with_text("");
    let res: Value =
        serde_json::from_str(&core.insert_click_here_field_at(0, 0, 0, "", "", "", true).unwrap())
            .unwrap();
    assert_eq!(res["ok"], Value::Bool(false), "빈 이름은 거부되어야 한다");
    let list: Value = serde_json::from_str(&core.get_field_list_json()).unwrap();
    assert_eq!(list.as_array().unwrap().len(), 0, "유령 필드가 없어야 한다");
}

// ── 누름틀: 같은 이름 전부 갱신 (별도 문단, 비겹침) ──────────────────────
#[test]
fn set_field_value_by_name_updates_all() {
    let mut core = blank_with_text("");
    core.insert_click_here_field_at(0, 0, 0, "", "", "same", true)
        .unwrap();
    core.insert_paragraph_native(0, 1).unwrap();
    core.insert_click_here_field_at(0, 1, 0, "", "", "same", true)
        .unwrap();
    core.set_field_value_by_name("same", "ZZ").unwrap();
    let list: Value = serde_json::from_str(&core.get_field_list_json()).unwrap();
    let vals: Vec<String> = list
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["value"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(vals, vec!["ZZ".to_string(), "ZZ".to_string()], "둘 다 갱신");
}

// ── 책갈피: char_offset 반영 + 범위 검증 ────────────────────────────────
#[test]
fn bookmark_char_offset_reflected_and_bounds_checked() {
    let mut core = blank_with_text("0123456789"); // 길이 10
    core.add_bookmark_native(0, 0, 0, "at0").unwrap();
    core.add_bookmark_native(0, 0, 5, "at5").unwrap();
    let over: Value = serde_json::from_str(&core.add_bookmark_native(0, 0, 999, "over").unwrap())
        .unwrap();
    assert_eq!(over["ok"], Value::Bool(false), "범위 초과 거부");
    // u32 래핑(-1) — usize로는 huge
    let neg: Value =
        serde_json::from_str(&core.add_bookmark_native(0, 0, usize::MAX, "neg").unwrap()).unwrap();
    assert_eq!(neg["ok"], Value::Bool(false), "음수(래핑) 거부");

    let bms: Value = serde_json::from_str(&core.get_bookmarks_native().unwrap()).unwrap();
    let mut pos: Vec<(String, u64)> = bms
        .as_array()
        .unwrap()
        .iter()
        .map(|b| {
            (
                b["name"].as_str().unwrap().to_string(),
                b["charPos"].as_u64().unwrap(),
            )
        })
        .collect();
    pos.sort_by_key(|(_, p)| *p);
    assert_eq!(pos, vec![("at0".to_string(), 0), ("at5".to_string(), 5)]);
}

// ── 책갈피: 새 책갈피가 기존 핸들을 밀지 않는다 ─────────────────────────
#[test]
fn bookmark_handle_stable_after_insert() {
    let mut core = blank_with_text("ABCDEFGHIJ");
    core.add_bookmark_native(0, 0, 1, "A").unwrap();
    let bms: Value = serde_json::from_str(&core.get_bookmarks_native().unwrap()).unwrap();
    let handle_a = bms[0]["ctrlIdx"].as_u64().unwrap() as usize;
    core.add_bookmark_native(0, 0, 2, "B").unwrap();
    core.rename_bookmark_native(0, 0, handle_a, "RENAMED").unwrap();
    let bms: Value = serde_json::from_str(&core.get_bookmarks_native().unwrap()).unwrap();
    // 위치순 정렬: 1번(원래 A→RENAMED), 2번(B)
    let mut named: Vec<(u64, String)> = bms
        .as_array()
        .unwrap()
        .iter()
        .map(|b| {
            (
                b["charPos"].as_u64().unwrap(),
                b["name"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    named.sort_by_key(|(p, _)| *p);
    let names: Vec<String> = named.into_iter().map(|(_, n)| n).collect();
    assert_eq!(
        names,
        vec!["RENAMED".to_string(), "B".to_string()],
        "핸들이 원래 A를 가리켜야 한다"
    );
}

// ── 책갈피: 삭제 후 본문 무손상 ─────────────────────────────────────────
#[test]
fn bookmark_delete_keeps_text() {
    let mut core = blank_with_text("ABCDEFG");
    core.add_bookmark_native(0, 0, 3, "BM").unwrap();
    let bms: Value = serde_json::from_str(&core.get_bookmarks_native().unwrap()).unwrap();
    let handle = bms[0]["ctrlIdx"].as_u64().unwrap() as usize;
    core.delete_bookmark_native(0, 0, handle).unwrap();
    let bms: Value = serde_json::from_str(&core.get_bookmarks_native().unwrap()).unwrap();
    assert_eq!(bms.as_array().unwrap().len(), 0, "삭제됨");
    assert_eq!(core.get_text_range_native(0, 0, 0, 20).unwrap(), "ABCDEFG");
}

// ── 각주: 빈 각주 0자, 채운 뒤 실제 길이 ────────────────────────────────
#[test]
fn footnote_info_excludes_marker_placeholder() {
    let mut core = blank_with_text("본문");
    let made: Value = serde_json::from_str(&core.insert_footnote_native(0, 0, 2).unwrap()).unwrap();
    let cidx = made["controlIdx"].as_u64().unwrap() as usize;

    let empty: Value = serde_json::from_str(&core.get_footnote_info_native(0, 0, cidx).unwrap())
        .unwrap();
    assert_eq!(empty["totalTextLen"].as_u64().unwrap(), 0, "빈 각주는 0자");
    assert_eq!(empty["texts"][0].as_str().unwrap(), "");

    core.insert_text_in_footnote_native(0, 0, cidx, 0, 0, "각주내용")
        .unwrap();
    let filled: Value = serde_json::from_str(&core.get_footnote_info_native(0, 0, cidx).unwrap())
        .unwrap();
    assert_eq!(filled["totalTextLen"].as_u64().unwrap(), 4);
    assert_eq!(filled["texts"][0].as_str().unwrap(), "각주내용");
}
