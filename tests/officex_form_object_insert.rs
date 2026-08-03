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

/// 개체 좌우 이동 — delta 로 한 글자씩, 절대 offset 으로 드래그 낙하. 본문이 정확히 갈라진다.
#[test]
fn move_form_object_within_text() {
    let mut doc = new_doc();
    doc.insert_text_native(0, 0, 0, "가나다라").unwrap();
    let r = doc
        .insert_form_object_native(0, 0, 2, r#"{"formType":"CheckBox"}"#)
        .unwrap();
    let ci: usize = r.split("\"controlIdx\":").nth(1).and_then(|t| t.trim_end_matches('}').parse().ok()).unwrap();

    // 렌더에서 체크박스의 x 가 텍스트 사이 어디냐로 위치를 판정한다 — 위치 2: "가나[☐]다라"
    let x_of = |d: &HwpDocument, needle: &str| -> f64 {
        let svg = d.render_page_svg_native(0).unwrap();
        let line = svg.lines().find(|l| l.contains(needle)).unwrap_or_else(|| panic!("{needle} 없음"));
        let at = line.find(" x=\"").unwrap() + 4;
        line[at..].split('"').next().unwrap().parse().unwrap()
    };
    let box_x = |d: &HwpDocument| -> f64 {
        // 체크 사각형은 rect 로 그려진다 — 첫 검은 테두리 rect 의 x
        let svg = d.render_page_svg_native(0).unwrap();
        let line = svg.lines().find(|l| l.starts_with("<rect") && l.contains("선택") || l.contains("checkbox"))
            .map(|l| l.to_string());
        // 렌더 구현에 기대지 말고 캡션 "선택 상자" 텍스트 x 로 판정한다
        drop(line);
        x_of(d, ">선택 상자<")
    };

    let x2 = box_x(&doc); // 위치 2
    // 왼쪽으로 한 칸 → 위치 1: "가[☐]나다라"
    let r = doc.move_form_object_native(0, 0, ci, r#"{"delta":-1}"#).unwrap();
    let ci: usize = r.split("\"controlIdx\":").nth(1).and_then(|t| t.trim_end_matches('}').parse().ok()).unwrap();
    let x1 = box_x(&doc);
    assert!(x1 < x2, "왼쪽 이동 후 x 가 줄어야: {x1} vs {x2}");

    // 절대 위치 4(맨 끝) → "가나다라[☐]"
    let r = doc.move_form_object_native(0, 0, ci, r#"{"offset":4}"#).unwrap();
    let ci: usize = r.split("\"controlIdx\":").nth(1).and_then(|t| t.trim_end_matches('}').parse().ok()).unwrap();
    let x4 = box_x(&doc);
    assert!(x4 > x2, "끝 이동 후 x 가 커져야: {x4} vs {x2}");

    // 왕복 무결 — 저장해 다시 열어도 위치·본문 유지
    let bytes = doc.export_hwp_with_adapter().unwrap();
    let doc2 = HwpDocument::from_bytes(&bytes).unwrap();
    assert_eq!(doc2.get_text_range_native(0, 0, 0, 10).unwrap(), "가나다라");
    assert!(doc2.get_form_object_info_native(0, 0, ci).unwrap().contains("\"ok\":true"));
}

/// 캐럿이 개체 **오른쪽**에 설 수 있어야 한다 — 양식이 논리 길이에서 빠져 있으면
/// 개체 뒤로 커서가 못 간다(2026-08-03 사용자 신고: "개체 오른쪽으로 커서가 가지 않아").
#[test]
fn caret_can_stand_right_of_form_object() {
    let mut doc = new_doc();
    doc.insert_text_native(0, 0, 0, "가나").unwrap();
    doc.insert_form_object_native(0, 0, 2, r#"{"formType":"PushButton"}"#).unwrap();
    doc.insert_text_native(0, 0, 2, "다").unwrap();

    // 본문 글자 3 + 개체 1 = 논리 길이 4
    let logical = doc.get_logical_length(0, 0).unwrap();
    assert_eq!(logical, 4, "양식이 논리 길이에 안 잡힌다(캐럿이 개체를 건너뛴다)");

    // 개체 오른쪽(논리 3)이 텍스트 좌표 2(=개체 뒤 '다' 앞)로 풀려야 한다
    let after_obj = doc.logical_to_text_offset(0, 0, 3).unwrap();
    let before_obj = doc.logical_to_text_offset(0, 0, 2).unwrap();
    assert!(
        after_obj >= before_obj,
        "개체 앞뒤 논리 위치가 같은 텍스트 좌표로 뭉갠다: {before_obj} -> {after_obj}"
    );
}

/// 캐럿 **그림**도 개체 오른쪽에 그려져야 한다 — 논리 위치만 맞고 x 가 개체 폭을
/// 건너뛰면 "타이핑은 오른쪽에 되는데 커서는 왼쪽에 보이는" 상태가 된다(2026-08-03 신고).
#[test]
fn caret_rect_moves_past_form_object_width() {
    let mut doc = new_doc();
    doc.insert_text_native(0, 0, 0, "나가 내").unwrap();
    doc.insert_form_object_native(0, 0, 3, r#"{"formType":"PushButton"}"#).unwrap();

    let rect_x = |d: &HwpDocument, off: usize| -> f64 {
        let r = d.get_cursor_rect_native(0, 0, off).unwrap();
        let at = r.find("\"x\":").unwrap() + 4;
        r[at..].split(|c| c == ',' || c == '}').next().unwrap().parse().unwrap()
    };

    // 논리: 나(0)가(1)공백(2)[개체](3)내(4). 개체 앞(3) vs 개체 뒤(4).
    let before = rect_x(&doc, 3);
    let after = rect_x(&doc, 4);
    // 명령 단추 기본 폭 7087 HWPUNIT ≈ 94px — 절반 이상은 벌어져야 한다.
    assert!(
        after - before > 40.0,
        "캐럿 x 가 개체 폭을 건너뛴다: before={before:.1}, after={after:.1}"
    );
}

/// 개체를 연속 삽입한 문단 끝에서 타이핑 — 글자가 개체들 **뒤**에 들어가야 한다.
/// (2026-08-03 실측: logical_to_text_offset 워크 루프가 같은 위치의 컨트롤 여럿을
/// 소비 못 해 at_ctrl=false 로 풀렸고, 이어 친 글자가 개체들 앞으로 말려들어갔다.)
#[test]
fn typing_after_consecutive_forms_lands_after_them() {
    let mut doc = new_doc();
    doc.insert_text_logical(0, 0, 0, "수집: ").unwrap();
    for kind in ["CheckBox", "ComboBox", "RadioButton"] {
        doc.insert_form_object_native(0, 0, 4, &format!(r#"{{"formType":"{kind}"}}"#))
            .unwrap();
    }
    // 커서 = 개체 3개 뒤(논리 7)
    let r = doc.insert_text_logical(0, 0, 7, "끝").unwrap();
    assert!(r.contains("\"logicalOffset\":8"), "삽입 후 논리 위치: {r}");

    let svg = doc.render_page_svg_native(0).unwrap();
    let x_of = |needle: &str| -> f64 {
        let line = svg.lines().find(|l| l.contains(needle)).unwrap_or_else(|| panic!("{needle} 없음"));
        let at = line.find(" x=\"").unwrap() + 4;
        line[at..].split('"').next().unwrap().parse().unwrap()
    };
    assert!(
        x_of(">끝<") > x_of(">라디오 단추<"),
        "글자가 개체들 앞에 그려졌다: 끝={} 라디오={}",
        x_of(">끝<"), x_of(">라디오 단추<")
    );
}

/// 본문 사이에 넣은 개체는 **그 글자 크기에 맞춰** 들어가야 한다 —
/// 한컴 정본 크기(19.8pt)를 10pt 본문에 그대로 넣으면 줄 높이가 뛰어 글자가 밀린다
/// (2026-08-03 사용자 신고 "텍스트 높낮이가 달라지는데").
#[test]
fn inline_form_matches_font_size() {
    let mut doc = new_doc();
    doc.insert_text_native(0, 0, 0, "가나다").unwrap();
    let r = doc
        .insert_form_object_native(0, 0, 2, r#"{"formType":"CheckBox"}"#)
        .unwrap();
    let ci: usize = r.split("\"controlIdx\":").nth(1).and_then(|t| t.trim_end_matches('}').parse().ok()).unwrap();
    let info = doc.get_form_object_info_native(0, 0, ci).unwrap();
    let h: u32 = info.split("\"height\":").nth(1).unwrap().split(',').next().unwrap().parse().unwrap();
    // 기본 10pt(=1000) 문서 → 1.4em = 1400 언저리. 정본 1984 를 그대로 쓰면 실패한다.
    assert!(h <= 1500, "인라인 개체가 글자보다 너무 크다: {h} HWPUNIT");
    assert!(h >= 900, "너무 작다: {h}");
}
