//! [2026-08-15 신고 "그림을 옆으로 돌렸는데 테두리만 돌아가"] 차트(Ole RawSvg) 회전 핀.
//!
//! 결함: 렌더 트리에서 그림(Image)·도형(Rect/Path)은 노드에 ShapeTransform(회전/대칭)을
//! 싣는데, 차트가 타는 Ole → RawSvg 분기만 transform 을 버렸다. 그래서 회전각은
//! 저장되고 선택 테두리(스튜디오가 각도로 그림)는 돌아가는데 차트 내용은 그대로였다.
//!
//! 수리: RawSvgNode 에 transform 을 실어 3개 백엔드(svg 직삽입·web_canvas·skia)가
//! Image 와 동일 규약(bbox 중심 회전)으로 적용한다.

use rhwp::wasm_api::HwpDocument;

fn chart_doc() -> (HwpDocument, u32) {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    let spec = r#"{"style":"column","type":"column","title":"차트 제목","categories":["항목 1","항목 2"],"series":[{"name":"계열 1","values":[4.3,2.5]}]}"#;
    let out = doc
        .insert_chart_native(0, 0, spec, 0, 0, false)
        .expect("차트 삽입");
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    (doc, v["controlIdx"].as_u64().unwrap() as u32)
}

/// 회전각을 주면 차트가 소멸하지 않고 rotate transform 이 실려야 한다.
#[test]
fn rotated_chart_still_renders() {
    let (mut doc, ci) = chart_doc();

    let svg0 = doc.render_page_svg_native(0).expect("렌더0");
    assert!(svg0.contains("차트 제목"), "회전 전 차트 미표시");

    doc.set_shape_properties_native(0, 0, ci as usize, r#"{"rotationAngle":30}"#)
        .expect("회전 설정");
    let svg1 = doc.render_page_svg_native(0).expect("렌더1");
    assert!(svg1.contains("차트 제목"), "회전 30도에서 차트 소멸");
    assert!(
        svg1.contains("rotate(30"),
        "차트에 rotate(30) transform 미적용"
    );
}

/// [2026-08-15 신고 "이동하면 계속 깜빡"] 차트 조각은 원점 기준 + translate 배치 계약.
/// 절대 좌표 방출로 돌아가면 드래그(매 프레임 재조판)마다 조각 바이트가 바뀌어
/// web_canvas 디코드 캐시가 항상 빗나가고 이동 중 차트가 깜빡인다.
#[test]
fn chart_fragment_is_origin_relative_with_translate() {
    let (doc, _ci) = chart_doc();
    let svg = doc.render_page_svg_native(0).expect("렌더");
    assert!(
        svg.contains("<g transform=\"translate("),
        "차트 원점 조각의 translate 래퍼가 없다 — 절대 좌표 회귀(드래그 깜빡임)"
    );
}

/// 회전 0 복귀 시 transform 래퍼가 남지 않아야 한다(이중 래핑/잔존 방지).
#[test]
fn zero_rotation_leaves_no_wrapper() {
    let (mut doc, ci) = chart_doc();
    doc.set_shape_properties_native(0, 0, ci as usize, r#"{"rotationAngle":45}"#)
        .expect("회전 설정");
    doc.set_shape_properties_native(0, 0, ci as usize, r#"{"rotationAngle":0}"#)
        .expect("회전 해제");
    let svg = doc.render_page_svg_native(0).expect("렌더");
    assert!(svg.contains("차트 제목"), "회전 해제 후 차트 소멸");
    assert!(!svg.contains("rotate(45"), "해제한 회전이 렌더에 잔존");
}
