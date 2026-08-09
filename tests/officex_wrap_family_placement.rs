//! [officex] 어울림 가족 배치 계약 핀 — Tight/Through 는 Square 와 같은 위치 규칙을 따른다.
//!
//! 실측(2026-07-26, diag_square_jump): 빈 host + Page/Paper 절대배치 게이트가 Square
//! 전용이라 Tight/Through 가 vertRelTo/vertAlign 을 무시하고 흐름 위치에 남았다
//! (Page+Center 에서 Square=중앙 545 vs Tight/Through=238.9). 수리 = layout.rs 의
//! paper_page_square_empty_top 게이트를 가족(Square|Tight|Through)으로 확장.

use rhwp::wasm_api::HwpDocument;

fn placed_y(wrap: &str) -> f64 {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().expect("blank");
    doc.insert_text(0, 0, 0, &"가나다라마바사아자차카타파하 ".repeat(30))
        .expect("text");
    let created = doc.create_table(0, 0, 200, 3, 3).expect("table");
    let v: serde_json::Value = serde_json::from_str(&created).unwrap();
    let (pi, ci) = (
        v["paraIdx"].as_u64().unwrap() as u32,
        v["controlIdx"].as_u64().unwrap() as u32,
    );
    doc.set_table_properties(
        0,
        pi,
        ci,
        &format!(
            r#"{{"treatAsChar":false,"textWrap":"{wrap}","vertRelTo":"Page","vertAlign":"Center","vertOffset":0}}"#
        ),
    )
    .expect("props");
    let b = doc.get_table_bbox(0, pi, ci).expect("bbox");
    serde_json::from_str::<serde_json::Value>(&b).unwrap()["y"]
        .as_f64()
        .unwrap()
}

#[test]
fn tight_and_through_follow_square_absolute_placement() {
    let square = placed_y("Square");
    let tight = placed_y("Tight");
    let through = placed_y("Through");
    // 계약: 가족 3종은 같은 위치 규칙 — Page+Center 면 셋 다 중앙(흐름 위치가 아님).
    assert!(
        (tight - square).abs() < 0.5 && (through - square).abs() < 0.5,
        "어울림 가족 배치 갈라짐: square={square:.1} tight={tight:.1} through={through:.1}"
    );
    // 중앙 판정: 본문 상단(≈132) 흐름 위치보다 충분히 아래 — 배치 기준 무시 회귀 차단.
    assert!(
        square > 400.0,
        "Page+Center 인데 중앙이 아님: y={square:.1} (흐름 위치 잔류 의심)"
    );
}
