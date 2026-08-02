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

/// 이모지 줄과 전각 한글 줄의 **글자 자리 수**가 같아야 한다(= 줄이 끝까지 살아 있다).
///
/// 폭까지 같기를 요구하던 판정은 폐기했다: 컬러 이모지는 전각(1.0em)이 아니라
/// **1.28em** 이 정본이다(adb4cdc37, 오라클 = canvas measureText 10pt→17px).
/// 종전 단언은 그 수리 이전의 의도를 굳혀 둔 것이라 수리와 충돌했다.
#[test]
fn emoji_line_keeps_all_caret_slots() {
    let emoji = caret_xs("앞 😀😀😀 뒤");
    let cjk = caret_xs("앞 가나다 뒤");
    assert_eq!(emoji.len(), cjk.len(), "글자 수가 달라졌다: {emoji:?} vs {cjk:?}");
    // 앞머리("앞 ")는 이모지와 무관하니 좌표가 같아야 한다 — 줄 시작이 밀리면 다른 결함이다.
    for i in 0..3 {
        assert!(
            (emoji[i] - cjk[i]).abs() < 0.5,
            "이모지 앞 구간이 어긋난다: {i}번째 {} vs {}\n  {emoji:?}\n  {cjk:?}",
            emoji[i],
            cjk[i]
        );
    }
}

/// 컬러 이모지는 한글 글자처럼 **한 em 칸**을 쓴다(2026-08-02 결정).
///
/// 이력: 2026-08-01 에 잉크 폭 실측(1.28em)에 맞춰 진행폭을 키웠다가, 높이를 안 건드려
/// 이모지가 줄 아래로 처지는 결함이 드러났다. 이제 글리프를 글자 높이에 맞춰 줄여
/// 그리므로(EMOJI_GLYPH_SCALE) 진행폭도 한글과 같은 한 em 으로 되돌렸다.
#[test]
fn color_emoji_occupies_one_em_like_cjk() {
    let emoji = caret_xs("앞 😀😀😀 뒤");
    let cjk = caret_xs("앞 가나다 뒤");
    let emoji_adv = emoji[3] - emoji[2];
    let cjk_adv = cjk[3] - cjk[2];
    let ratio = emoji_adv / cjk_adv;
    assert!(
        (0.95..=1.10).contains(&ratio),
        "컬러 이모지 진행폭이 한글 한 칸과 같아야 한다 (실측 {ratio:.3}배: 이모지 {emoji_adv:.1}px vs 한글 {cjk_adv:.1}px)"
    );
}

/// 이모지가 줄 끝에 있어도 마지막 글자까지 좌표가 나와야 한다(줄이 짧게 끊기지 않는다).
#[test]
fn line_survives_trailing_emoji() {
    let xs = caret_xs("보고 완료 🎉🎉");
    assert_eq!(xs.len(), 9, "글자 수만큼 캐럿 자리가 나와야 한다: {xs:?}");
    assert!(xs[8] > xs[0], "줄이 끊겨 캐럿이 되돌아갔다: {xs:?}");
}
