//! [2026-08-15 신고 "자유이동 되지 않고"] 음수 오프셋이 getter 에서 부호를 잃던 결함 핀.
//!
//! 오프셋은 모델에 u32 비트캐스트로 저장된다. 공용 getter(common_props_json)가 이를
//! unsigned 그대로 내보내 앵커 위/왼쪽 드래그 한 번에 `4294967295` 가 됐고, 스튜디오의
//! 다음 드래그(기존값+델타)가 i32 범위를 벗어나 setter 에서 무시 — 이후 개체 이동이
//! 영구히 죽었다. 차트는 기본 배치가 Column/Para 기준이라 위로 끌면 즉시 음수가 되어
//! 가장 먼저 걸렸다. 그림·도형·차트 전부 이 getter 를 쓴다.

use rhwp::wasm_api::HwpDocument;

fn chart_doc() -> (HwpDocument, u32) {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    let spec = r#"{"style":"column","type":"column","title":"T","categories":["a"],"series":[{"name":"s","values":[1]}]}"#;
    let out = doc
        .insert_chart_native(0, 0, spec, 0, 0, false)
        .expect("차트 삽입");
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    (doc, v["controlIdx"].as_u64().unwrap() as u32)
}

/// 음수 오프셋 왕복: setter(-600) → getter(-600). unsigned 로 새면 드래그가 죽는다.
#[test]
fn negative_offset_roundtrips_signed() {
    let (mut doc, ci) = chart_doc();
    doc.set_shape_properties_native(0, 0, ci as usize, r#"{"horzOffset":-1,"vertOffset":-600}"#)
        .expect("음수 오프셋 설정");
    let props: serde_json::Value =
        serde_json::from_str(&doc.get_shape_properties(0, 0, ci).expect("속성 조회")).unwrap();
    assert_eq!(props["horzOffset"], -1, "horzOffset 이 unsigned 로 샜다");
    assert_eq!(props["vertOffset"], -600, "vertOffset 이 unsigned 로 샜다");
    // 드래그 시나리오 재개: 음수 상태에서 델타를 더한 값이 그대로 서야 한다
    doc.set_shape_properties_native(
        0,
        0,
        ci as usize,
        r#"{"horzOffset":2399,"vertOffset":1800}"#,
    )
    .expect("후속 드래그");
    let props: serde_json::Value =
        serde_json::from_str(&doc.get_shape_properties(0, 0, ci).expect("속성 조회")).unwrap();
    assert_eq!(props["horzOffset"], 2399);
    assert_eq!(props["vertOffset"], 1800);
}
