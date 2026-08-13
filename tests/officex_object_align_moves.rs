//! [2026-08-13 신고 "자유이동인데 위치 이동이 안 돼"] 정렬 변경이 개체를 옮기는 계약.
//!
//! 엔진 규약: 기준(rel_to)·정렬(align)을 **오프셋 없이** 바꾸면 rebase 가 개체를 제자리에
//! 유지한다("같은 자리를 다른 잣대로 다시 재라"). 그래서 우측 패널이 정렬만 보내면
//! 사용자가 「가운데」를 눌러도 개체가 안 움직였다.
//!
//! 해법은 엔진 규약을 뒤집는 게 아니라 이미 문서화된 **opt-out(명시 오프셋 동봉)**을 쓰는 것.
//! 패널은 정렬을 바꿀 때 현재 오프셋을 함께 보내 rebase 를 끄고, 개체가 새 정렬 기준으로
//! 이동하게 한다. 이 파일은 그 계약을 고정한다 — 깨지면 패널의 정렬 버튼이 다시 먹통이 된다.

use rhwp::wasm_api::HwpDocument;

const PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
    0x42, 0x60, 0x82,
];

fn setup() -> (HwpDocument, u32, u32) {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    let out = doc
        .insert_picture_native(0, 0, 0, &[], PNG, 20000, 15000, 1, 1, "png", "", None, None)
        .expect("그림 삽입");
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    (
        doc,
        v["paraIdx"].as_u64().unwrap() as u32,
        v["controlIdx"].as_u64().unwrap() as u32,
    )
}

fn props(doc: &HwpDocument, pi: u32, ci: u32) -> serde_json::Value {
    serde_json::from_str(&doc.get_picture_properties(0, pi, ci).expect("속성 조회")).unwrap()
}

fn base(doc: &mut HwpDocument, pi: u32, ci: u32) {
    doc.set_picture_properties(
        0,
        pi,
        ci,
        r#"{"treatAsChar":false,"horzRelTo":"Paper","horzAlign":"Left","horzOffset":4500,
            "vertRelTo":"Paper","vertAlign":"Top","vertOffset":3000}"#,
    )
    .unwrap();
}

/// 패널 방식(정렬 + 현재 오프셋 동봉): 오프셋이 그대로 남아 개체가 새 기준으로 이동한다.
#[test]
fn align_with_explicit_offset_moves_object() {
    let (mut doc, pi, ci) = setup();
    base(&mut doc, pi, ci);
    doc.set_picture_properties(0, pi, ci, r#"{"horzAlign":"Center","horzOffset":4500}"#)
        .unwrap();
    let after = props(&doc, pi, ci);
    assert_eq!(after["horzAlign"], "Center", "정렬 미반영");
    assert_eq!(
        after["horzOffset"], 4500,
        "명시 오프셋이 rebase 에 덮였다 — 패널 정렬 버튼이 먹통이 된다"
    );
}

/// 세로도 동형.
#[test]
fn vert_align_with_explicit_offset_moves_object() {
    let (mut doc, pi, ci) = setup();
    base(&mut doc, pi, ci);
    doc.set_picture_properties(0, pi, ci, r#"{"vertAlign":"Center","vertOffset":3000}"#)
        .unwrap();
    let after = props(&doc, pi, ci);
    assert_eq!(after["vertAlign"], "Center");
    assert_eq!(after["vertOffset"], 3000);
}

/// 반대 축: 오프셋 **없이** 정렬만 보내면 종전대로 제자리 유지(rebase) — 규약 보존.
#[test]
fn align_without_offset_still_rebases() {
    let (mut doc, pi, ci) = setup();
    base(&mut doc, pi, ci);
    doc.set_picture_properties(0, pi, ci, r#"{"horzAlign":"Center"}"#)
        .unwrap();
    let after = props(&doc, pi, ci);
    assert_eq!(after["horzAlign"], "Center");
    assert_ne!(
        after["horzOffset"], 4500,
        "오프셋 없는 정렬 전환은 rebase 로 제자리를 지켜야 한다(기존 규약)"
    );
}
