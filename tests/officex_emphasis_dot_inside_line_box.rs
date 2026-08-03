//! 강조점이 **줄 상자 안**에 그려지는지 — 첫 줄에서 통째로 사라지던 결함의 회귀 방지.
//!
//! 실결함(2026-08-03): 점을 baseline-1.05em 에 찍었다. 줄 상자 윗변이 baseline-1.0em 이라
//! 본문 clip 바깥이었고, 첫 줄에서는 아예 안 보이고 아랫줄에서는 한 줄 위 여백에 떠 보였다.
//! 여기서는 SVG 출력의 실제 좌표로 "점 잉크가 [줄 윗변, 글자 윗변] 사이"임을 확인한다.
//! (Canvas 렌더러는 같은 값을 쓰므로 이 테스트가 두 경로를 함께 지킨다 — 값이 갈라지면 고쳐라.)

/// `<text ... y="..." font-size="..."> ●` 에서 (y, font_size) 를 뽑는다.
fn dot_positions(svg: &str) -> Vec<(f64, f64)> {
    svg.lines()
        .filter(|l| l.contains(">●<"))
        .map(|l| {
            let pick = |key: &str| -> f64 {
                let at = l.find(key).unwrap_or_else(|| panic!("{key} 없음: {l}")) + key.len();
                let rest = &l[at..];
                let end = rest.find('"').unwrap();
                rest[..end].parse().unwrap()
            };
            (pick(" y=\""), pick(" font-size=\""))
        })
        .collect()
}

/// 본문 clip 사각형의 윗변 — 첫 줄 점이 여기 밖이면 화면에서 사라진다.
fn body_clip_top(svg: &str) -> f64 {
    let line = svg
        .lines()
        .find(|l| l.contains("<clipPath id=\"body-clip"))
        .expect("본문 clip 없음");
    let at = line.find(" y=\"").expect("clip y 없음") + 4;
    let rest = &line[at..];
    rest[..rest.find('"').unwrap()].parse().unwrap()
}

fn text_baseline_and_size(svg: &str) -> (f64, f64) {
    let line = svg
        .lines()
        .find(|l| l.contains(">강<"))
        .expect("본문 글자 없음");
    let pick = |key: &str| -> f64 {
        let at = line.find(key).unwrap() + key.len();
        let rest = &line[at..];
        rest[..rest.find('"').unwrap()].parse().unwrap()
    };
    (pick(" y=\""), pick(" font-size=\""))
}

#[test]
fn emphasis_dot_sits_between_line_top_and_glyph_top() {
    let mut doc = rhwp::wasm_api::HwpDocument::create_empty();
    doc.create_blank_document_native().unwrap();
    doc.insert_text_native(0, 0, 0, "강조").unwrap();
    doc.apply_char_format_native(0, 0, 0, 2, r#"{"emphasisDot":1}"#)
        .unwrap();

    let svg = doc.render_page_svg_native(0).unwrap();
    let dots = dot_positions(&svg);
    assert_eq!(dots.len(), 2, "글자 수만큼 점이 찍혀야 한다: {dots:?}");

    let (baseline, font_size) = text_baseline_and_size(&svg);
    let clip_top = body_clip_top(&svg);
    // '●' 잉크는 자기 baseline 위로 대략 0.55em 까지 올라간다(폰트 공통 근사).
    let ink_top = dots[0].0 - dots[0].1 * 0.55;
    // 한글 글자 윗변 근사 — 점은 이보다 위에 있어야 글자와 겹치지 않는다.
    let glyph_top = baseline - font_size * 0.8;

    assert!(
        ink_top >= clip_top,
        "점이 본문 clip 위로 삐져나가 첫 줄에서 사라진다: ink_top={ink_top:.2}, clip_top={clip_top:.2}"
    );
    assert!(
        dots[0].0 <= glyph_top,
        "점이 글자와 겹친다: dot_y={:.2}, glyph_top={glyph_top:.2}",
        dots[0].0
    );
}
