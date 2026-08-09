//! [officex/어울림 배선 3/3] 판정식 핀 — 위로 올린 자리차지 표가 앞 본문을 민다.
//!
//! probe-flow.mjs(sc- scratchpad)의 네이티브 재현. createTable 은 문단을 분할해
//! 표를 빈 host 문단(pi=1)에 앵커하므로, 밀려야 할 "앞 글자"는 **앞 문단(pi=0)** —
//! 앵커 문단 도착 시점 등록으로는 불가능한 역방향 케이스다. 사전 밴드
//! (PendingFloatBand.flow_top) 의 단 시작 선등록 + para_start_y 사전 시드(표 동결)가
//! 이 계약의 구현이다. 이 테스트가 깨지면 그 배선이 끊긴 것이다.

use rhwp::wasm_api::HwpDocument;

fn front_char_y(doc: &HwpDocument) -> f64 {
    let rects = doc.get_selection_rects(0, 0, 5, 0, 15).expect("rects");
    let v: serde_json::Value = serde_json::from_str(&rects).expect("json");
    v[0]["y"].as_f64().expect("y")
}

fn table_box(doc: &HwpDocument, pi: u32, ci: u32) -> (f64, f64) {
    let bbox = doc.get_table_bbox(0, pi, ci).expect("bbox");
    let v: serde_json::Value = serde_json::from_str(&bbox).expect("json");
    let y = v["y"].as_f64().unwrap();
    (y, y + v["height"].as_f64().unwrap())
}

#[test]
fn upward_visible_float_pushes_preceding_text_below() {
    let mm = 283.46_f64;
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().expect("blank");
    doc.insert_text(0, 0, 0, &"가나다라마바사아자차카타파하 ".repeat(30))
        .expect("insert");
    let created = doc.create_table(0, 0, 200, 3, 3).expect("createTable");
    let v: serde_json::Value = serde_json::from_str(&created).expect("json");
    let pi = v["paraIdx"].as_u64().unwrap() as u32;
    let ci = v["controlIdx"].as_u64().unwrap() as u32;
    doc.set_table_properties(
        0,
        pi,
        ci,
        r#"{"treatAsChar":false,"textWrap":"TopAndBottom","vertRelTo":"Para","restrictInPage":false,"vertOffset":0}"#,
    )
    .expect("setProps");

    let front_before = front_char_y(&doc);
    let (top0, _) = table_box(&doc, pi, ci);
    // 전제: v_off=0 표는 앞 본문 아래(순차 흐름)에 있고 앞 글자는 문단 첫 줄에 있다.
    assert!(
        top0 > front_before + 50.0,
        "전제 붕괴: 초기 표 top {top0:.1} 이 앞 글자 {front_before:.1} 바로 아래가 아님"
    );

    // 위로 -25mm: 표가 앞 문단의 첫 줄 잉크를 덮는 위치로 올라간다.
    doc.move_table_offset(0, pi, ci, 0, -(25.0 * mm).round() as i32)
        .expect("move-25");
    let (top, bottom) = table_box(&doc, pi, ci);
    let front_after = front_char_y(&doc);

    // 계약 1 (판정식): 앞 글자는 표 아래로 내려간다 — 관통·겹침 금지.
    assert!(
        front_after >= bottom - 0.5,
        "앞 글자 y={front_after:.1} 가 표[{top:.1}..{bottom:.1}] 아래로 밀리지 않음"
    );
    // 계약 2 (동결): 표는 밀린 텍스트를 따라 내려가지 않는다 — 원래 앵커 기준
    // 위치(초기 top − 25mm)에 남는다. 순환(표가 텍스트를 밀고 텍스트를 쫓아감) 차단.
    let expected_top = top0 - 25.0 * mm / 7200.0 * 96.0;
    assert!(
        (top - expected_top).abs() < 2.0,
        "표 top {top:.1} 이 동결 위치 {expected_top:.1} 에서 이탈 (순환 추적 의심)"
    );
}
