//! 바탕쪽 편집 왕복 회귀 (2026-07-28)
//!
//! 결함 위험: 직렬화가 `raw_list_header`(원본 바이트)를 우선 쓰므로, 문단을 편집해도
//! 옛 헤더가 저장돼 **변경이 파일에 반영되지 않는다**. set_master_page_text 가 raw 를
//! 무효화하는지 왕복으로 확인한다.

use rhwp::model::document::{Document, Section};
use rhwp::model::header_footer::MasterPage;
use rhwp::model::paragraph::Paragraph;
use rhwp::parser::parse_hwp;
use rhwp::serializer::cfb_writer::serialize_hwp;
use rhwp::wasm_api::HwpDocument;

fn master_texts(doc: &Document) -> Vec<String> {
    doc.sections
        .iter()
        .flat_map(|s| &s.section_def.master_pages)
        .map(|mp| {
            mp.paragraphs
                .iter()
                .map(|p| p.text.clone())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .collect()
}

#[test]
fn master_page_text_edit_survives_roundtrip() {
    // 바탕쪽 1개를 가진 문서 구성 (raw_list_header 를 일부러 채워 '원본 우선' 함정 재현)
    let mut doc = Document::default();
    let mut section = Section::default();
    section.paragraphs.push(Paragraph::new_empty());
    let mut mp = MasterPage {
        text_width: 59528,
        text_height: 84188,
        ..Default::default()
    };
    let mut old = Paragraph::new_empty();
    old.text = "옛 바탕쪽".to_string();
    old.char_count = old.text.chars().count() as u32;
    mp.paragraphs.push(old);
    // 원본 바이트가 남아 있는 상태(파일에서 읽은 문서와 같은 조건)
    mp.raw_list_header = vec![1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    section.section_def.master_pages.push(mp);
    // ⚠ 바탕쪽은 SectionDef 컨트롤의 자식으로 직렬화된다(serializer/control.rs:323).
    //   합성 문서에도 그 컨트롤이 있어야 파일에 실린다(없으면 조용히 누락 — 실측).
    section.paragraphs[0]
        .controls
        .push(rhwp::model::control::Control::SectionDef(Box::new(
            section.section_def.clone(),
        )));
    doc.sections.push(section);

    let mut hdoc = HwpDocument::create_empty();
    hdoc.set_document(doc);
    let before = hdoc.get_master_pages(0);
    assert!(before.contains("옛 바탕쪽"), "구성 실패: {before}");

    hdoc.set_master_page_text(0, 0, "새 바탕쪽\n둘째 줄")
        .expect("편집");

    let bytes = serialize_hwp(hdoc.document()).expect("직렬화");
    let reparsed = parse_hwp(&bytes).expect("재파싱");
    let after = master_texts(&reparsed);

    assert_eq!(
        after,
        vec!["새 바탕쪽\n둘째 줄".to_string()],
        "바탕쪽 편집이 저장에 반영되지 않음 (raw_list_header 무효화 실패 의심): {after:?}"
    );
}
