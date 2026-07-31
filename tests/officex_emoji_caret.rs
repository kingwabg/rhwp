//! 이모지가 든 줄의 캐럿 좌표 회귀(2026-07-31 실측 결함 2건).
//!
//! ① 마지막 줄의 끝을 `para.char_count`(글자 수)로 잡고 그 값을 UTF-16 오프셋으로
//!    해석해, 이모지처럼 UTF-16 에서 2칸인 글자가 있으면 줄이 그만큼 짧게 끊겼다.
//!    → "앞 😀😀😀 뒤" 에서 " 뒤" 가 사라지고 캐럿이 줄 머리로 되돌아갔다.
//! ② 폴백 폭 사다리가 두 곳에 복사돼 있어 이모지가 캐럿 경로에서만 반각으로 남아
//!    서로 겹쳤다(수리는 text_measurement::fallback_char_width 로 일원화).
//!
//! BMP 문자만 쓰면 두 결함 모두 드러나지 않는다 — 그래서 오래 남아 있었다.
use rhwp::wasm_api::HwpDocument;

fn caret_xs(text: &str) -> Vec<f64> {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    doc.insert_text(0, 0, 0, text).unwrap();
    let len = doc.get_paragraph_length(0, 0).unwrap();
    (0..=len)
        .map(|i| {
            let j = doc.get_cursor_rect(0, 0, i).unwrap();
            let v: serde_json::Value = serde_json::from_str(&j).unwrap();
            (v["x"].as_f64().unwrap() * 10.0).round() / 10.0
        })
        .collect()
}

#[test]
fn caret_advances_monotonically_across_emoji() {
    let xs = caret_xs("앞 😀😀😀 뒤");
    for w in xs.windows(2) {
        assert!(w[1] >= w[0], "캐럿이 뒤로 갔다(줄 시작으로 리셋): {xs:?}");
    }
}

/// 이모지 줄과 같은 자리에 전각 한글을 넣은 줄이 거의 같은 좌표여야 한다
/// (= 이모지를 전각으로 재고, 줄이 끝까지 살아 있다).
#[test]
fn emoji_line_matches_cjk_line() {
    let emoji = caret_xs("앞 😀😀😀 뒤");
    let cjk = caret_xs("앞 가나다 뒤");
    assert_eq!(emoji.len(), cjk.len(), "글자 수가 달라졌다: {emoji:?} vs {cjk:?}");
    for (i, (a, b)) in emoji.iter().zip(cjk.iter()).enumerate() {
        assert!(
            (a - b).abs() < 2.0,
            "{i}번째 캐럿이 어긋난다: 이모지 {a} vs 한글 {b}\n  {emoji:?}\n  {cjk:?}"
        );
    }
}

/// 이모지가 줄 끝에 있어도 마지막 글자까지 좌표가 나와야 한다(줄이 짧게 끊기지 않는다).
#[test]
fn line_survives_trailing_emoji() {
    let xs = caret_xs("보고 완료 🎉🎉");
    assert_eq!(xs.len(), 9, "글자 수만큼 캐럿 자리가 나와야 한다: {xs:?}");
    assert!(xs[8] > xs[0], "줄이 끊겨 캐럿이 되돌아갔다: {xs:?}");
}
