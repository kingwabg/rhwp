//! 구역 나누기 회귀 (2026-07-30)
//!
//! 판정식(mydocs/eng/plans/section-break.md):
//! ① 나누기 후 구역 1→2, 본문 텍스트 총량 불변
//! ② 새 구역만 가로 전환 → 두 구역의 landscape 독립 (기능의 존재 이유)
//! ③ HWP5 저장→재로드 후에도 ①② 유지 — 새 구역 문단0의 SectionDef **컨트롤**이
//!    직렬화를 이겨야 한다(Section.section_def 만 갈라 두면 저장에서 사라지는 함정).

use rhwp::wasm_api::HwpDocument;

fn all_text(doc: &HwpDocument) -> String {
    doc.document()
        .sections
        .iter()
        .flat_map(|s| &s.paragraphs)
        .map(|p| p.text.as_str())
        .collect()
}

#[test]
fn section_break_splits_and_survives_roundtrip() {
    let mut doc = HwpDocument::create_empty();
    doc.insert_text(0, 0, 0, "첫 구역 내용").unwrap();
    doc.split_paragraph(0, 0, 6).unwrap();
    doc.insert_text(0, 1, 0, "둘째 구역이 될 내용").unwrap();
    let before_text = all_text(&doc);

    // ① 2문단 머리에서 구역 나누기
    let r = doc.insert_section_break(0, 1, 0).unwrap();
    assert!(r.contains("\"ok\":true"), "실행 실패: {r}");
    assert!(r.contains("\"sectionIdx\":1"), "새 구역 인덱스: {r}");
    assert_eq!(doc.get_section_count(), 2, "구역 1→2");
    // 커서에서 가르는 의미론(쪽/단 나누기와 동일): 문단 머리에서 나누면 원 구역
    // 끝에 빈 문단이 남는다 — 한컴의 Enter 계열 나누기와 같은 동작.
    assert_eq!(doc.get_paragraph_count(0).unwrap(), 2, "원 구역 문단 수(끝 빈 문단 포함)");
    assert_eq!(doc.get_paragraph_count(1).unwrap(), 1, "새 구역 문단 수");
    assert_eq!(all_text(&doc), before_text, "본문 텍스트 총량 불변");

    // ② 새 구역만 가로로 — 구역 독립성
    let pd1 = doc.get_page_def(1).unwrap();
    let landscaped = pd1.replace("\"landscape\":false", "\"landscape\":true");
    doc.set_page_def(1, &landscaped).unwrap();
    assert!(
        doc.get_page_def(0).unwrap().contains("\"landscape\":false"),
        "구역0은 세로 유지"
    );
    assert!(
        doc.get_page_def(1).unwrap().contains("\"landscape\":true"),
        "구역1은 가로"
    );

    // ③ HWP5 왕복
    let bytes = doc.export_hwp().unwrap();
    let re = HwpDocument::from_bytes(&bytes).unwrap();
    assert_eq!(re.get_section_count(), 2, "재로드 후 구역 수");
    assert_eq!(all_text(&re), before_text, "재로드 후 텍스트");
    assert!(
        re.get_page_def(0).unwrap().contains("\"landscape\":false"),
        "재로드 후 구역0 세로"
    );
    assert!(
        re.get_page_def(1).unwrap().contains("\"landscape\":true"),
        "재로드 후 구역1 가로 — SectionDef 컨트롤이 직렬화를 이겼는가"
    );
}
