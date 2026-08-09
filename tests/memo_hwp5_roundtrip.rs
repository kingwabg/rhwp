//! HWP5 메모 본문 보존 회귀 (2026-07-28)
//!
//! 결함: 직렬화(serializer/body_text.rs)는 MEMO_LIST 꼬리를 완전히 기록하는데 파서는
//! memo_index 만 읽고 본문을 버려서, 한컴에서 메모를 단 .hwp 를 열었다 저장하면
//! **메모가 통째로 사라졌다**(HWPX 경로는 정상). 파서에 대칭 경로를 추가해 수리.
//!
//! 코퍼스에 메모 보유 .hwp 표본이 없어(2026-07-28 전수 스캔) 모델을 직접 구성해 왕복한다.

use rhwp::model::control::{Control, Field, FieldType};
use rhwp::model::document::{Document, Section};
use rhwp::model::paragraph::Paragraph;
use rhwp::parser::parse_hwp;
use rhwp::serializer::cfb_writer::serialize_hwp;

fn memo_bodies(doc: &Document) -> Vec<String> {
    let mut out = Vec::new();
    for sec in &doc.sections {
        for para in &sec.paragraphs {
            for ctrl in &para.controls {
                if let Control::Field(f) = ctrl {
                    if f.field_type == FieldType::Memo {
                        for mp in &f.memo_paragraphs {
                            out.push(mp.text.clone());
                        }
                    }
                }
            }
        }
    }
    out
}

#[test]
fn hwp5_memo_body_survives_roundtrip() {
    // 빈 문서 + 본문 1문단에 메모 필드(본문 2문단)
    let mut doc = Document::default();
    let mut section = Section::default();
    section.paragraphs.push(Paragraph::new_empty());
    doc.sections.push(section);
    let mut memo_body_1 = Paragraph::new_empty();
    memo_body_1.text = "검토 의견입니다".to_string();
    memo_body_1.char_count = memo_body_1.text.chars().count() as u32;
    let mut memo_body_2 = Paragraph::new_empty();
    memo_body_2.text = "둘째 줄".to_string();
    memo_body_2.char_count = memo_body_2.text.chars().count() as u32;

    let field = Field {
        field_type: FieldType::Memo,
        // 직렬화는 ctrl_id 를 원본 그대로 되쓴다 — 합성 필드는 이 값을 채워야 재파싱에서
        // 다시 Memo 로 인식된다(실측: 0 이면 Unknown 으로 돌아옴).
        ctrl_id: rhwp::parser::tags::FIELD_MEMO,
        memo_index: 0,
        memo_paragraphs: vec![memo_body_1, memo_body_2],
        ..Default::default()
    };

    let sec = doc.sections.get_mut(0).expect("빈 문서에 구역 1개");
    let para = sec.paragraphs.get_mut(0).expect("빈 문서에 문단 1개");
    para.text = "본문".to_string();
    para.char_count = para.text.chars().count() as u32;
    para.controls.push(Control::Field(field));

    let before = memo_bodies(&doc);
    assert_eq!(before.len(), 2, "구성 실패: {before:?}");

    let bytes = serialize_hwp(&doc).expect("HWP5 직렬화");
    let reparsed = parse_hwp(&bytes).expect("HWP5 재파싱");
    let after = memo_bodies(&reparsed);

    assert_eq!(
        after, before,
        "HWP5 왕복에서 메모 본문이 유실됨 (전={before:?} 후={after:?})"
    );
}
