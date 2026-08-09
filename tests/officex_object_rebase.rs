//! [개선 트랙1] 배치 기준계/정렬 전환 오프셋 rebase 핀.
//!
//! 계약: rel_to/align 키가 있고 값이 실제 변하며 **오프셋 키가 없으면**, 개체의 시각
//! 위치(렌더트리 bbox)를 보존하도록 새 기준계 오프셋을 역산해 기록한다(위치 보존).
//! 명시 오프셋 동봉 = opt-out(숫자 보존). 표는 common + raw_ctrl_data 이중 기록.

use rhwp::wasm_api::HwpDocument;

fn table_doc(props: &str) -> (HwpDocument, u32, u32) {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    doc.insert_text(0, 0, 0, &"가나다라마바사 아자차카타파하 ".repeat(10))
        .unwrap();
    let c: serde_json::Value =
        serde_json::from_str(&doc.create_table(0, 0, 100, 2, 2).unwrap()).unwrap();
    let (pi, ci) = (
        c["paraIdx"].as_u64().unwrap() as u32,
        c["controlIdx"].as_u64().unwrap() as u32,
    );
    doc.set_table_properties(0, pi, ci, props).unwrap();
    (doc, pi, ci)
}

fn table_bbox(doc: &HwpDocument, pi: u32, ci: u32) -> (f64, f64, f64, f64) {
    let b: serde_json::Value =
        serde_json::from_str(&doc.get_table_bbox(0, pi, ci).unwrap()).unwrap();
    (
        b["x"].as_f64().unwrap(),
        b["y"].as_f64().unwrap(),
        b["width"].as_f64().unwrap(),
        b["height"].as_f64().unwrap(),
    )
}

fn table_h_offset(doc: &HwpDocument, pi: u32, ci: u32) -> i64 {
    // get_table_properties 의 horzOffset 은 raw_ctrl_data H_OFFSET 슬라이스에서 읽는다
    // — 이 값의 정합 검증이 곧 이중 기록(raw==common) 검증이다.
    let p: serde_json::Value =
        serde_json::from_str(&doc.get_table_properties(0, pi, ci).unwrap()).unwrap();
    p["horzOffset"].as_i64().unwrap()
}

const HU_PER_PX: f64 = 75.0; // 96dpi: 7200/96

/// 핀 1 — float 표 Page/Left 배치 후 horzRelTo→Paper 전환: 렌더트리 bbox.x 불변(±0.5px)
/// AND raw_ctrl_data H_OFFSET(get_table_properties 경유) == 렌더 위치(common 소비)와 정합.
#[test]
fn pin_frame_switch_preserves_bbox() {
    let (mut doc, pi, ci) = table_doc(
        r#"{"treatAsChar":false,"textWrap":"TopAndBottom","vertRelTo":"Para","vertOffset":0,"horzRelTo":"Page","horzAlign":"Left","horzOffset":3000}"#,
    );
    let (x0, ..) = table_bbox(&doc, pi, ci);
    doc.set_table_properties(0, pi, ci, r#"{"horzRelTo":"Paper"}"#)
        .unwrap();
    let (x1, ..) = table_bbox(&doc, pi, ci);
    assert!(
        (x1 - x0).abs() <= 0.5,
        "기준계 전환 후 표가 점프: x {x0:.2} → {x1:.2}"
    );
    // Paper/Left 의 forward: x = 0 + off. raw 슬라이스 오프셋이 렌더 위치와 일치해야
    // common(렌더 소비)과 raw(저장 소비)가 같은 값이라는 뜻이다.
    let off_px = table_h_offset(&doc, pi, ci) as f64 / HU_PER_PX;
    assert!(
        (off_px - x1).abs() <= 0.5,
        "raw H_OFFSET({off_px:.2}px)과 렌더 x({x1:.2}px) 불일치 — 이중 기록 깨짐"
    );
}

/// 핀 2 — horzAlign Right(offset 3000) → Left 전환: bbox.x 불변(부호 반전 역산 검증).
#[test]
fn pin_align_flip_right_left() {
    let (mut doc, pi, ci) = table_doc(
        r#"{"treatAsChar":false,"textWrap":"TopAndBottom","vertRelTo":"Para","vertOffset":0,"horzRelTo":"Page","horzAlign":"Right","horzOffset":3000}"#,
    );
    let (x0, ..) = table_bbox(&doc, pi, ci);
    doc.set_table_properties(0, pi, ci, r#"{"horzAlign":"Left"}"#)
        .unwrap();
    let (x1, ..) = table_bbox(&doc, pi, ci);
    assert!(
        (x1 - x0).abs() <= 0.5,
        "정렬 반전 후 표가 점프: x {x0:.2} → {x1:.2}"
    );
}

/// 핀 3 — 명시 오프셋 동봉 = opt-out: rebase 미개입, 숫자 그대로.
#[test]
fn pin_explicit_offset_suppresses_rebase() {
    let (mut doc, pi, ci) = table_doc(
        r#"{"treatAsChar":false,"textWrap":"TopAndBottom","vertRelTo":"Para","vertOffset":0,"horzRelTo":"Page","horzAlign":"Left","horzOffset":3000}"#,
    );
    doc.set_table_properties(0, pi, ci, r#"{"horzRelTo":"Paper","horzOffset":5000}"#)
        .unwrap();
    assert_eq!(
        table_h_offset(&doc, pi, ci),
        5000,
        "명시 오프셋이 rebase 에 덮였다"
    );
}

/// 핀 4 — Shape 공용 setter 의 음수 오프셋(json_i32 통일): u32 비트캐스트 왕복.
#[test]
fn pin_negative_offset_via_common_setter() {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    let c: serde_json::Value = serde_json::from_str(
        &doc.create_shape_control(
            r#"{"sectionIdx":0,"paraIdx":0,"charOffset":0,"width":6000,"height":6000,"treatAsChar":false,"shapeType":"rectangle"}"#,
        )
        .unwrap(),
    )
    .unwrap();
    let (pi, ci) = (
        c["paraIdx"].as_u64().unwrap() as u32,
        c["controlIdx"].as_u64().unwrap() as u32,
    );
    doc.set_shape_properties(0, pi, ci, r#"{"horzOffset":-1200}"#)
        .unwrap();
    let p: serde_json::Value =
        serde_json::from_str(&doc.get_shape_properties(0, pi, ci).unwrap()).unwrap();
    // common_obj_attr_to_json 은 u32 로 출력 — 음수는 비트캐스트로 왕복한다.
    assert_eq!(
        p["horzOffset"].as_u64().unwrap(),
        (-1200i32 as u32) as u64,
        "음수 오프셋이 소실됨 (json_u32 회귀)"
    );
}

/// 핀 5 — 페이지 중간 문단의 float 도형 vertRelTo Para→Page 전환 후 bbox.y 불변(±0.5px).
/// (도형과 그림은 compute_object_position 의 동일 세로 forward 를 쓴다.)
#[test]
fn pin_vert_para_to_page() {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    let filler = |n: usize| format!("채움 {n:02} 가나다라마바사 아자차카타파하 강물이 흐르고");
    doc.insert_text(0, 0, 0, &filler(6)).unwrap();
    for n in (1..6).rev() {
        doc.insert_paragraph(0, 0).unwrap();
        doc.insert_text(0, 0, 0, &filler(n)).unwrap();
    }
    let c: serde_json::Value = serde_json::from_str(
        &doc.create_shape_control(
            r#"{"sectionIdx":0,"paraIdx":4,"charOffset":0,"width":6000,"height":6000,"treatAsChar":false,"shapeType":"rectangle"}"#,
        )
        .unwrap(),
    )
    .unwrap();
    let (pi, ci) = (
        c["paraIdx"].as_u64().unwrap() as u32,
        c["controlIdx"].as_u64().unwrap() as u32,
    );
    // 명시 오프셋 동봉 → rebase 미개입으로 초기 배치 확정 (Para 기준, 문단 중간 y).
    doc.set_shape_properties(
        0,
        pi,
        ci,
        r#"{"vertRelTo":"Para","vertAlign":"Top","vertOffset":4500,"horzRelTo":"Column","horzAlign":"Left","horzOffset":1500}"#,
    )
    .unwrap();
    let bbox = |doc: &HwpDocument| -> (f64, f64) {
        let b: serde_json::Value =
            serde_json::from_str(&doc.get_shape_bbox(0, pi, ci).unwrap()).unwrap();
        (b["x"].as_f64().unwrap(), b["y"].as_f64().unwrap())
    };
    let (x0, y0) = bbox(&doc);
    doc.set_shape_properties(0, pi, ci, r#"{"vertRelTo":"Page"}"#)
        .unwrap();
    let (x1, y1) = bbox(&doc);
    assert!(
        (y1 - y0).abs() <= 0.5,
        "세로 기준계 전환 후 도형이 점프: y {y0:.2} → {y1:.2}"
    );
    assert!(
        (x1 - x0).abs() <= 0.5,
        "세로 전환이 가로를 건드림: x {x0:.2} → {x1:.2}"
    );
}
