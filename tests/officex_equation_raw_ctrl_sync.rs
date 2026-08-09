//! [officex 0d 감사] HWP5 파스 수식의 raw_ctrl_data 낡은 미러 저장 핀.
//!
//! 직렬화기(serialize_equation_control)는 raw_ctrl_data 가 있으면 그대로 기록한다.
//! 종전 수식 setter 는 물리(`eq.common`)만 바꾸고 사본을 안 따라오게 해서, HWP5 로드
//! 수식의 배치 편집(TAC 전환·오프셋)이 .hwp 저장에서 파스 시점 값으로 조용히 원복됐다 —
//! 표 저장 손상(2026-08-06 수리)과 같은 가족. 수리: `sync_raw_ctrl_data_from_common`.

use rhwp::document_core::DocumentCore;
use rhwp::model::control::Control;
use std::fs;
use std::path::Path;

/// 첫 본문 수식의 (section, para, control) 인덱스.
fn first_equation(doc: &rhwp::model::document::Document) -> (usize, usize, usize) {
    for (si, section) in doc.sections.iter().enumerate() {
        for (pi, para) in section.paragraphs.iter().enumerate() {
            for (ci, ctrl) in para.controls.iter().enumerate() {
                if matches!(ctrl, Control::Equation(_)) {
                    return (si, pi, ci);
                }
            }
        }
    }
    panic!("본문 수식 없음");
}

/// HWP5 로드 수식: 배치 편집 → .hwp 저장 → 재파스에서 편집값이 살아남아야 한다.
#[test]
fn equation_placement_edit_survives_hwp_save() {
    let bytes = fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("samples/math-001.hwp"))
        .expect("read samples/math-001.hwp");
    let mut core = DocumentCore::from_bytes(&bytes).expect("parse");
    let (si, pi, ci) = first_equation(core.document());

    // 파스본은 raw_ctrl_data 가 채워져 있어야 이 핀이 유효하다(직렬화기가 raw 우선).
    {
        let Control::Equation(eq) = &core.document().sections[si].paragraphs[pi].controls[ci]
        else {
            unreachable!()
        };
        // 36 = common_obj_offsets::MIN_LEN (pub(crate)). sync 는 이 미만이면 무동작이므로
        // is_empty 전제만으로는 핀이 헛돈다.
        assert!(
            eq.raw_ctrl_data.len() >= 36,
            "전제: HWP5 파스 수식은 raw_ctrl_data ≥ 36바이트 보유 (실제 {})",
            eq.raw_ctrl_data.len()
        );
        assert!(eq.common.treat_as_char, "전제: 표본 수식은 TAC");
    }

    core.set_equation_properties_native(
        si,
        pi,
        ci,
        None,
        None,
        r#"{"treatAsChar":false,"vertOffset":1234,"horzOffset":567}"#,
    )
    .expect("set_equation_properties_native");

    let saved = core.export_hwp_with_adapter().expect("export .hwp");
    let re = rhwp::parser::parse_document(&saved).expect("re-parse");
    let Control::Equation(eq) = &re.sections[si].paragraphs[pi].controls[ci] else {
        panic!("재파스에 수식 없음")
    };
    assert!(
        !eq.common.treat_as_char,
        "TAC 해제가 저장에서 원복됨 (raw_ctrl_data 낡은 미러)"
    );
    assert_eq!(eq.common.vertical_offset, 1234, "vertOffset 원복");
    assert_eq!(eq.common.horizontal_offset, 567, "horzOffset 원복");
}
