//! 강조점이 **줄 상자 안**에 그려지는지 — 첫 줄에서 사라지거나 윗부분이 잘리던 결함의 회귀 방지.
//!
//! 실결함(2026-08-03): 점을 글꼴 글리프(●○ˇ˜･˸)로 찍었다. 잉크 크기를 알 수 없어
//!  (1) 줄 상자 위로 삐져나가 본문 clip 에 잘리고(첫 줄에서는 통째로 안 보임),
//!  (2) ● 말고는 잉크가 작아 화면에서 안 보였다.
//! 지금은 renderer::emphasis 가 좌표로 직접 그린다. 여기서는 SVG 실좌표로 확인한다.
//! (Canvas 렌더러도 같은 모듈을 쓴다 — 값이 갈라지면 두 경로가 어긋난 것이다.)

fn attr(line: &str, key: &str) -> f64 {
    let at = line
        .find(key)
        .unwrap_or_else(|| panic!("{key} 없음: {line}"))
        + key.len();
    let rest = &line[at..];
    rest[..rest.find('"').unwrap()].parse().unwrap()
}

/// 본문 clip 사각형의 윗변 — 여기보다 위는 화면에서 잘린다.
fn body_clip_top(svg: &str) -> f64 {
    let line = svg
        .lines()
        .find(|l| l.contains("<clipPath id=\"body-clip"))
        .expect("본문 clip 없음");
    attr(line, " y=\"")
}

fn text_baseline_and_size(svg: &str) -> (f64, f64) {
    let line = svg.lines().find(|l| l.contains(">강<")).expect("본문 글자 없음");
    (attr(line, " y=\""), attr(line, " font-size=\""))
}

fn render_with_dot(kind: u8) -> String {
    let mut doc = rhwp::wasm_api::HwpDocument::create_empty();
    doc.create_blank_document_native().unwrap();
    doc.insert_text_native(0, 0, 0, "강조").unwrap();
    doc.apply_char_format_native(0, 0, 0, 2, &format!(r#"{{"emphasisDot":{kind}}}"#))
        .unwrap();
    doc.render_page_svg_native(0).unwrap()
}

#[test]
fn every_emphasis_kind_is_drawn_inside_the_line_box() {
    for kind in 1..=6u8 {
        let svg = render_with_dot(kind);
        let (baseline, font_size) = text_baseline_and_size(&svg);
        let clip_top = body_clip_top(&svg);
        let glyph_top = baseline - font_size * 0.8; // 한글 글자 윗변 근사

        // 그려진 도형의 세로 범위를 모은다(원 + 꺾은선).
        let mut lo = f64::MAX;
        let mut hi = f64::MIN;
        let mut shapes = 0;
        for line in svg.lines() {
            if line.starts_with("<circle") {
                let cy = attr(line, " cy=\"");
                let r = attr(line, " r=\"");
                let sw = if line.contains("stroke-width") { attr(line, " stroke-width=\"") } else { 0.0 };
                lo = lo.min(cy - r - sw / 2.0);
                hi = hi.max(cy + r + sw / 2.0);
                shapes += 1;
            } else if line.starts_with("<polyline") {
                let sw = attr(line, " stroke-width=\"");
                let at = line.find(" points=\"").unwrap() + 9;
                let rest = &line[at..];
                for pt in rest[..rest.find('"').unwrap()].split(' ') {
                    let yv: f64 = pt.split(',').nth(1).unwrap().parse().unwrap();
                    lo = lo.min(yv - sw / 2.0);
                    hi = hi.max(yv + sw / 2.0);
                }
                shapes += 1;
            }
        }

        assert!(shapes >= 2, "강조점 {kind}: 글자 수만큼 그려져야 한다(그려진 도형 {shapes}개)");
        assert!(
            lo >= clip_top,
            "강조점 {kind} 이 본문 clip 위로 나간다(첫 줄에서 잘린다): {lo:.2} < {clip_top:.2}"
        );
        assert!(
            hi <= glyph_top,
            "강조점 {kind} 이 글자와 겹친다: {hi:.2} > {glyph_top:.2}"
        );
        // 안 보일 만큼 납작하면 실패 — ● 말고는 안 보이던 게 이 결함이다.
        assert!(
            hi - lo >= font_size * 0.08,
            "강조점 {kind} 이 너무 납작해 안 보인다: 높이 {:.2}px",
            hi - lo
        );
    }
}
