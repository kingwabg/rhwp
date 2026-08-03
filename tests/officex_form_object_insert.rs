//! 양식 개체 삽입 v1 — 5종 삽입·렌더·HWP 왕복(저장→다시 열기) 회귀.
//!
//! 골격은 수식 삽입과 같은 확장 컨트롤 규약(본문 8 WCHAR)이다. 여기서 지키는 것:
//!  1. 삽입 직후 렌더 트리에 양식 개체가 나온다(조판 배선).
//!  2. HWP 로 저장해 다시 열어도 종류·캡션·크기가 산다(직렬화 왕복).
//!  3. 삭제하면 본문 길이가 원래대로 돌아온다(8 WCHAR 반환).

use rhwp::wasm_api::HwpDocument;

const KINDS: [(&str, &str); 5] = [
    ("PushButton", "명령 단추"),
    ("CheckBox", "선택 상자"),
    ("ComboBox", ""),
    ("RadioButton", "라디오 단추"),
    ("Edit", ""),
];

fn new_doc() -> HwpDocument {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document_native().unwrap();
    doc
}

#[test]
fn insert_each_kind_and_roundtrip_hwp() {
    let mut doc = new_doc();
    doc.insert_text_native(0, 0, 0, "양식:").unwrap();
    for (kind, _) in KINDS {
        let r = doc
            .insert_form_object_native(0, 0, 3, &format!(r#"{{"formType":"{kind}"}}"#))
            .unwrap_or_else(|e| panic!("{kind} 삽입 실패: {e:?}"));
        assert!(r.contains("\"ok\":true"), "{kind}: {r}");
    }

    // 1. 조판/렌더에 나온다
    let svg = doc.render_page_svg_native(0).unwrap();
    for needle in ["명령 단추", "선택 상자", "라디오 단추"] {
        assert!(svg.contains(needle), "SVG 에 {needle} 캡션이 없다");
    }

    // 2. HWP 왕복
    let bytes = doc.export_hwp_with_adapter().unwrap();
    let doc2 = HwpDocument::from_bytes(&bytes).unwrap();
    let mut seen = Vec::new();
    for ci in 0..10usize {
        if let Ok(info) = doc2.get_form_object_info_native(0, 0, ci) {
            if info.contains("\"ok\":true") {
                seen.push(info);
            }
        }
    }
    assert_eq!(seen.len(), 5, "왕복 후 양식 개체 5개여야: {}개", seen.len());
    for (kind, caption) in KINDS {
        let found = seen.iter().find(|i| i.contains(&format!("\"formType\":\"{kind}\"")));
        let info = found.unwrap_or_else(|| panic!("왕복 후 {kind} 가 없다"));
        if !caption.is_empty() {
            assert!(info.contains(caption), "{kind} 캡션 소실: {info}");
        }
    }
}

/// 삭제 후에도 스트림 장부(char_count/char_offsets)가 맞아야 한다 —
/// 틀리면 저장이 깨지거나 다시 연 문서가 어긋난다. 그래서 "삭제 → 저장 → 재열기"로 검사한다.
#[test]
fn delete_then_roundtrip_stays_consistent() {
    let mut doc = new_doc();
    doc.insert_text_native(0, 0, 0, "가나다").unwrap();
    let r = doc
        .insert_form_object_native(0, 0, 1, r#"{"formType":"CheckBox"}"#)
        .unwrap();
    // 빈 문서에도 숨은 컨트롤(구역/단 정의)이 있어 인덱스는 반환값에서 읽는다.
    let ci: usize = r
        .split("\"controlIdx\":")
        .nth(1)
        .and_then(|t| t.trim_end_matches('}').parse().ok())
        .expect("controlIdx");
    assert!(doc
        .get_form_object_info_native(0, 0, ci)
        .unwrap()
        .contains("\"ok\":true"));

    doc.delete_form_object_native(0, 0, ci).unwrap();
    assert!(
        doc.get_form_object_info_native(0, 0, ci).is_err()
            || !doc.get_form_object_info_native(0, 0, ci).unwrap().contains("\"ok\":true"),
        "삭제 후에도 양식 개체가 남아 있다"
    );

    let bytes = doc.export_hwp_with_adapter().unwrap();
    let doc2 = HwpDocument::from_bytes(&bytes).unwrap();
    let text = doc2.get_text_range_native(0, 0, 0, 10).unwrap_or_default();
    assert_eq!(text, "가나다", "삭제 후 왕복 본문이 어긋났다: {text:?}");
}

/// 텍스트를 전부 지워 양식만 남은 문단 — 컨트롤 총폭이 줄 폭을 넘어 두 줄로 감길 때,
/// 같은 char_start 의 빈 줄들이 전체 TAC 를 겹쳐 받아 **모든 양식이 줄마다 복제**돼 보였다
/// (2026-08-03 실측: 브라우저에서 Ctrl+Z 로 텍스트를 되돌리자 양식 줄이 두 벌로).
#[test]
fn textless_paragraph_with_wrapping_forms_renders_each_form_once() {
    let mut doc = new_doc();
    doc.insert_text_native(0, 0, 0, "동의: ").unwrap();
    doc.insert_form_object_native(0, 0, 4, r#"{"formType":"CheckBox"}"#).unwrap();
    for kind in ["PushButton", "ComboBox", "RadioButton", "RadioButton", "Edit"] {
        doc.insert_form_object_native(0, 0, 5, &format!(r#"{{"formType":"{kind}"}}"#))
            .unwrap();
    }
    // 총폭 > 줄 폭 → 두 줄로 감긴다. 텍스트 삭제 후에도 각 양식은 정확히 한 번.
    doc.delete_text_native(0, 0, 0, 4).unwrap();
    let svg = doc.render_page_svg_native(0).unwrap();
    assert_eq!(svg.matches("선택 상자").count(), 1, "선택 상자 복제");
    assert_eq!(svg.matches("명령 단추").count(), 1, "명령 단추 복제");
    assert_eq!(svg.matches("라디오 단추").count(), 2, "라디오 단추 복제");
}

/// 콤보 항목 편집 v1 — setFormObjectProps({items}) 가
///  1) info.items 에 즉시 반영되고 (properties listItem{N} 정본)
///  2) HWP 저장 → 재열기에도 살아남아야 한다(스크립트 스트림 왕복).
#[test]
fn combobox_items_roundtrip_hwp() {
    let mut doc = new_doc();
    let r = doc
        .insert_form_object_native(0, 0, 0, r#"{"formType":"ComboBox","name":"계절"}"#)
        .unwrap();
    let ci: usize = r
        .split("\"controlIdx\":")
        .nth(1)
        .and_then(|t| t.trim_end_matches('}').parse().ok())
        .expect("controlIdx");

    doc.set_form_object_props_native(
        0, 0, ci,
        r#"{"text":"계절 선택","items":["봄","여름","가을","겨울"]}"#,
    )
    .unwrap();

    let info = doc.get_form_object_info_native(0, 0, ci).unwrap();
    assert!(info.contains(r#"["봄","여름","가을","겨울"]"#), "즉시 반영 실패: {info}");

    let bytes = doc.export_hwp_with_adapter().unwrap();
    let doc2 = HwpDocument::from_bytes(&bytes).unwrap();
    let info2 = doc2.get_form_object_info_native(0, 0, ci).unwrap();
    assert!(
        info2.contains(r#"["봄","여름","가을","겨울"]"#),
        "HWP 왕복 후 항목 소실: {info2}"
    );
    assert!(info2.contains(r#""text":"계절 선택""#), "텍스트 소실: {info2}");
}

/// 항목을 두 번 갈아끼워도 스크립트에 옛 줄이 안 쌓여야 한다(중복 InsertString 방지).
#[test]
fn combobox_items_replace_not_append() {
    let mut doc = new_doc();
    let r = doc
        .insert_form_object_native(0, 0, 0, r#"{"formType":"ComboBox","name":"급식"}"#)
        .unwrap();
    let ci: usize = r
        .split("\"controlIdx\":")
        .nth(1)
        .and_then(|t| t.trim_end_matches('}').parse().ok())
        .unwrap();
    doc.set_form_object_props_native(0, 0, ci, r#"{"items":["밥","빵"]}"#).unwrap();
    doc.set_form_object_props_native(0, 0, ci, r#"{"items":["김치","라면","떡"]}"#).unwrap();

    // 재열기 후에도 마지막 항목만 (스크립트에 옛 줄이 쌓였으면 옛 항목이 섞인다)
    let bytes = doc.export_hwp_with_adapter().unwrap();
    let doc2 = HwpDocument::from_bytes(&bytes).unwrap();
    let info2 = doc2.get_form_object_info_native(0, 0, ci).unwrap();
    assert!(info2.contains(r#"["김치","라면","떡"]"#), "교체 실패: {info2}");
    assert!(!info2.contains("밥"), "옛 항목 잔존: {info2}");
}
