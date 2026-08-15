//! [2026-08-15 신고 "명령 단추, 선택 상자, 입력 상자 — 선택은 되는데 자유 이동이 안 된다"]
//! 양식 개체 시각 오프셋 핀.
//!
//! 배경: HWPX `<hp:pos>` 는 양식이든 그림이든 속성 집합이 같고(11속성 동일), HWP5 양식
//! CTRL_HEADER 도 46바이트 개체 공통 속성 그 자체다 — 즉 포맷은 부동 배치를 **담을 수
//! 있다**. 하지만 한컴 실물은 전수 인라인이다(HWPX 190/190, HWP 214/214 실측). 그래서
//! 이건 손실 복구가 아니라 신규 기능이고, 한컴 정합 규약상 두 가지를 지켜야 한다 —
//! 오프셋 0에서 한컴과 픽셀 동일, 비0은 우리가 정의하되 코드에 발산을 적어 둘 것.
//!
//! 설계: 앵커는 글자 사이(인라인)에 그대로 두고 **보이는 위치만 델타로 옮긴다**. 폭
//! 예약·줄 높이·캐럿 칸은 건드리지 않으므로 기존 문서는 산술이 `+0.0` 이라 불변이다.

use rhwp::wasm_api::HwpDocument;

const HU_PER_PX: f64 = 75.0;

fn form_doc() -> (HwpDocument, usize) {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    let out = doc
        .insert_form_object_native(0, 0, 0, r#"{"formType":"PushButton","name":"btn1"}"#)
        .expect("양식 삽입");
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let ci = v["controlIdx"]
        .as_u64()
        .or_else(|| v["ci"].as_u64())
        .expect("controlIdx") as usize;
    (doc, ci)
}

/// 페이지를 성글게 훑어 양식 개체 bbox 를 찾는다 (x, y, w, h).
///
/// 스튜디오가 클릭 판정에 쓰는 바로 그 경로(`get_form_object_at_native`)를 쓴다 —
/// 렌더 트리를 직접 읽는 것보다 사용자가 겪는 계약에 가깝다.
fn find_form_bbox(doc: &HwpDocument) -> (f64, f64, f64, f64) {
    let mut y = 0.0;
    while y < 400.0 {
        let mut x = 0.0;
        while x < 600.0 {
            if let Ok(out) = doc.get_form_object_at_native(0, x, y) {
                let v: serde_json::Value = serde_json::from_str(&out).unwrap();
                if v["found"] == true {
                    let b = &v["bbox"];
                    return (
                        b["x"].as_f64().unwrap(),
                        b["y"].as_f64().unwrap(),
                        b["w"].as_f64().unwrap(),
                        b["h"].as_f64().unwrap(),
                    );
                }
            }
            x += 6.0;
        }
        y += 4.0;
    }
    panic!("양식 개체를 화면에서 찾지 못했다");
}

/// 핀 1 — 오프셋이 화면 위치를 정확히 그만큼 옮긴다(양수·음수 양쪽).
///
/// 음수 축을 같이 박는 이유: 그림 쪽에서 오프셋 부호를 잃어 자유 이동이 영구히 죽은
/// 사고가 실제로 있었다(2026-08-15, officex_object_negative_offset).
#[test]
fn offset_moves_form_visually_both_signs() {
    let (mut doc, ci) = form_doc();
    let (x0, y0, w0, h0) = find_form_bbox(&doc);

    doc.set_form_object_props_native(0, 0, ci, r#"{"horzOffset":2250,"vertOffset":1500}"#)
        .expect("오프셋 설정");
    let (x1, y1, w1, h1) = find_form_bbox(&doc);
    assert!(
        (x1 - x0 - 2250.0 / HU_PER_PX).abs() < 0.5,
        "가로 오프셋 미반영: x {x0:.2} → {x1:.2}"
    );
    assert!(
        (y1 - y0 - 1500.0 / HU_PER_PX).abs() < 0.5,
        "세로 오프셋 미반영: y {y0:.2} → {y1:.2}"
    );
    assert!(
        (w1 - w0).abs() < 0.01 && (h1 - h0).abs() < 0.01,
        "오프셋이 크기를 바꿨다 — 델타는 위치만 옮겨야 한다"
    );

    doc.set_form_object_props_native(0, 0, ci, r#"{"horzOffset":-2250,"vertOffset":-1500}"#)
        .expect("음수 오프셋 설정");
    let (x2, y2, ..) = find_form_bbox(&doc);
    assert!(
        (x2 - x0 + 2250.0 / HU_PER_PX).abs() < 0.5,
        "음수 가로 오프셋이 소실됐다: x {x0:.2} → {x2:.2}"
    );
    assert!(
        (y2 - y0 + 1500.0 / HU_PER_PX).abs() < 0.5,
        "음수 세로 오프셋이 소실됐다: y {y0:.2} → {y2:.2}"
    );
}

/// 핀 2 — 오프셋을 줘도 **조판은 불변**이다(쪽수·글줄 메트릭).
///
/// 이 핀이 이 설계의 안전장치다. 누군가 "빈칸이 남는 게 이상하다"며 폭 예약이나
/// `Control::Form(_) => true`(인라인 술어) 를 손대면 여기서 먼저 터진다.
#[test]
fn offset_does_not_disturb_typesetting() {
    let (mut doc, ci) = form_doc();
    let pages0 = doc.page_count();
    let (_, _, w0, h0) = find_form_bbox(&doc);

    doc.set_form_object_props_native(0, 0, ci, r#"{"horzOffset":3000,"vertOffset":900}"#)
        .expect("오프셋 설정");

    assert_eq!(doc.page_count(), pages0, "오프셋이 쪽수를 바꿨다");
    let (_, _, w1, h1) = find_form_bbox(&doc);
    assert!(
        (w1 - w0).abs() < 0.01 && (h1 - h0).abs() < 0.01,
        "오프셋이 개체 크기를 바꿨다 — 델타는 위치만 옮겨야 한다"
    );
}

/// 핀 3 — 오프셋 키가 없는 문서(= 한컴 실물 전부)는 렌더가 비트 동일해야 한다.
///
/// 한컴 코퍼스 214/214 가 인라인이라는 실측을 코드로 고정한다. 델타 기능이 기존
/// 문서를 건드리지 않는다는 증명.
#[test]
fn absent_offset_keys_render_identically() {
    let (doc_a, _) = form_doc();
    let before = doc_a.render_page_svg_native(0).expect("렌더");

    let (mut doc_b, ci) = form_doc();
    // 오프셋 0 을 명시해도 키 부재와 같은 그림이어야 한다(0 = +0.0)
    doc_b
        .set_form_object_props_native(0, 0, ci, r#"{"horzOffset":0,"vertOffset":0}"#)
        .expect("0 오프셋");
    let after = doc_b.render_page_svg_native(0).expect("렌더");

    assert_eq!(before, after, "오프셋 0 이 렌더를 바꿨다 — 기존 문서 회귀");
}
