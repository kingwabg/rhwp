//! [officex] 빈 host 어울림 가족(Square|Tight|Through)+Para 의 vertOffset 소비 계약 핀.
//!
//! 결함(2026-07-27 실기 재현): 빈 host 어울림 표는 어느 배치 경로에도 못 들어
//! vertOffset 이 렌더에 반영되지 않았다 — 드래그하면 모델 오프셋만 축적되고 화면은
//! 부동(사용자 "표 이동이 안 된다"의 본체). 수리 = 빈 host lane 분류를 가족까지 확장.

use rhwp::wasm_api::HwpDocument;

fn moved_px(wrap: &str, restrict: bool, mm: f64) -> (f64, f64) {
    let hu_per_mm = 283.46_f64;
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().expect("blank");
    doc.insert_text(0, 0, 0, &"가나다라마바사아자차카타파하 ".repeat(20))
        .expect("t");
    let c: serde_json::Value =
        serde_json::from_str(&doc.create_table(0, 0, 100, 3, 3).unwrap()).unwrap();
    let (pi, ci) = (
        c["paraIdx"].as_u64().unwrap() as u32,
        c["controlIdx"].as_u64().unwrap() as u32,
    );
    doc.set_table_properties(0, pi, ci, &format!(
        r#"{{"treatAsChar":false,"textWrap":"{wrap}","vertRelTo":"Para","restrictInPage":{restrict},"vertOffset":0}}"#
    )).unwrap();
    let y = |d: &HwpDocument| -> f64 {
        serde_json::from_str::<serde_json::Value>(&d.get_table_bbox(0, pi, ci).unwrap()).unwrap()
            ["y"]
            .as_f64()
            .unwrap()
    };
    let y0 = y(&doc);
    doc.move_table_offset(0, pi, ci, 0, (mm * hu_per_mm).round() as i32)
        .unwrap();
    let b: serde_json::Value =
        serde_json::from_str(&doc.get_table_bbox(0, pi, ci).unwrap()).unwrap();
    (
        b["y"].as_f64().unwrap() - y0,
        b["y"].as_f64().unwrap() + b["height"].as_f64().unwrap(),
    )
}

#[test]
fn square_family_consumes_vert_offset_like_topbottom() {
    let (tb, _) = moved_px("TopAndBottom", true, 100.0);
    for wrap in ["Square", "Tight", "Through"] {
        let (moved, _) = moved_px(wrap, true, 100.0);
        assert!(
            (moved - tb).abs() < 0.5 && moved > 300.0,
            "{wrap}: vertOffset 소비가 자리차지와 어긋남 (moved={moved:.1}, 기준={tb:.1})"
        );
    }
}

#[test]
fn square_family_respects_restrict_clamp() {
    // 쪽영역 제한 ON — 과잉 이동(+500mm)도 본문 아래끝(≈1009.1)을 넘지 못한다.
    for wrap in ["Square", "Tight"] {
        let (_, bottom) = moved_px(wrap, true, 500.0);
        assert!(
            bottom <= 1009.6,
            "{wrap}: 제한 ON 인데 본문 밖 (bottom={bottom:.1})"
        );
    }
}
