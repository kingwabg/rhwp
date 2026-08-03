//! 강조점(글자 위 점) 모양 — canvas·svg 가 **같은 도형**을 그리도록 한 곳에 둔다.
//!
//! 왜 글꼴 글리프(●○ˇ˜･˸)를 쓰지 않는가(2026-08-03 실측):
//!  - 글리프마다 잉크 크기가 제각각이라 한 벌의 font-size 로는 ● 만 보이고 ˇ ˜ ･ 는 안 보인다.
//!  - 잉크 높이를 모르니 줄 상자 밖으로 삐져나가 본문 clip 에 윗부분이 잘린다.
//!  - 대체 글꼴에 그 글리프가 없으면 두부(□)가 뜬다.
//!
//! 그래서 좌표로 직접 그린다. 쓸 수 있는 세로 공간은 **[줄 상자 윗변, 글자 윗변]** 뿐이다:
//! 줄 상자 윗변이 baseline-1.0em, 한글 글자 윗변이 대략 baseline-0.8em 이라 0.2em 이 전부다.
//! 세로가 좁은 만큼 가로로 넉넉히 그려 읽히게 한다(갈매기·물결).

/// 그리기 원시 도형 — 렌더러는 이 둘만 구현하면 된다.
pub(crate) enum EmphasisPrim {
    /// 원(채움 또는 테두리)
    Circle {
        cx: f64,
        cy: f64,
        r: f64,
        filled: bool,
        stroke_width: f64,
    },
    /// 이어 그리는 선
    Polyline {
        points: Vec<(f64, f64)>,
        width: f64,
    },
}

/// 글자 하나에 얹을 강조점 도형.
///
/// `center_x` 는 글자 가로 중앙, `baseline_y` 는 글자 baseline, `font_size` 는 글자 크기(px).
/// 종류는 한/글 「글자 모양」 강조점 순서를 따른다(1=● 2=○ 3=ˇ 4=˜ 5=･ 6=˸).
/// `line_top` 은 이 글자가 앉은 **줄 상자의 윗변**(본문 clip 도 여기서 잘린다).
/// 글꼴·줄간격에 따라 baseline 에서 얼마나 떨어져 있는지가 달라지므로 **추측하지 않고 받는다** —
/// 0.985em 로 어림잡았더니 ascent 가 그보다 짧은 글꼴에서 점 윗부분이 잘렸다(2026-08-03 신고).
pub(crate) fn emphasis_mark(
    kind: u8,
    center_x: f64,
    baseline_y: f64,
    font_size: f64,
    line_top: f64,
) -> Vec<EmphasisPrim> {
    let lw = (font_size * 0.045).max(0.5);
    // 선으로 그리는 모양은 굵기의 절반이 바깥으로 번진다 — 그만큼 안쪽으로 들여 잡는다.
    // 줄 상자 윗변보다 위로는 절대 올라가지 않는다.
    let top = (baseline_y - font_size * 0.985).max(line_top) + lw / 2.0;
    // 아래는 글자 윗변 살짝 위. 다만 띠가 너무 얇아지면(=줄 상자가 빠듯하면) 안 보이므로,
    // 글자에 조금 다가서더라도 최소 두께는 지킨다 — 안 보이는 것보다 붙는 게 낫다.
    let bottom = (baseline_y - font_size * 0.825 - lw / 2.0).max(top + font_size * 0.11);
    let h = bottom - top;
    let cy = (top + bottom) / 2.0;
    // 채움 원은 선 번짐이 없어 띠를 꽉 채워도 된다(선 모양보다 조금 크게 보이는 게 맞다).
    let fill_h = h + lw;
    // 가로는 넉넉히 — 세로 0.16em 로는 모양이 안 읽힌다.
    let half_w = font_size * 0.19;

    match kind {
        1 => vec![EmphasisPrim::Circle {
            cx: center_x,
            cy,
            r: fill_h / 2.0,
            filled: true,
            stroke_width: 0.0,
        }],
        2 => vec![EmphasisPrim::Circle {
            cx: center_x,
            cy,
            // 테두리 원은 선이 반지름 밖으로 lw/2 번진다 — 그만큼 줄여야 띠 안에 든다.
            r: h / 2.0,
            filled: false,
            stroke_width: lw,
        }],
        // 갈매기(∨) — 아래로 뾰족하게
        3 => vec![EmphasisPrim::Polyline {
            points: vec![
                (center_x - half_w, top),
                (center_x, bottom),
                (center_x + half_w, top),
            ],
            width: lw,
        }],
        // 물결(~) — 꺾은선 네 토막으로 근사(곡선 API 없이 두 렌더러가 같게 나온다)
        4 => vec![EmphasisPrim::Polyline {
            points: vec![
                (center_x - half_w, bottom),
                (center_x - half_w * 0.5, top),
                (center_x, cy),
                (center_x + half_w * 0.5, bottom),
                (center_x + half_w, top),
            ],
            width: lw,
        }],
        // 가운뎃점 — 작은 점 하나
        5 => vec![EmphasisPrim::Circle {
            cx: center_x,
            cy,
            r: fill_h * 0.32,
            filled: true,
            stroke_width: 0.0,
        }],
        // 쌍점 — 작은 점 둘(세로로 겹치기엔 띠가 너무 얕아 가로로 나란히 둔다)
        6 => {
            let dx = fill_h * 0.55;
            let r = fill_h * 0.32;
            vec![
                EmphasisPrim::Circle {
                    cx: center_x - dx,
                    cy,
                    r,
                    filled: true,
                    stroke_width: 0.0,
                },
                EmphasisPrim::Circle {
                    cx: center_x + dx,
                    cy,
                    r,
                    filled: true,
                    stroke_width: 0.0,
                },
            ]
        }
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 어떤 종류도 **띠 밖으로 나가지 않는다** — 나가면 본문 clip 에 잘려 첫 줄에서 사라진다.
    #[test]
    fn every_mark_stays_inside_the_band() {
        let (baseline, fs) = (100.0_f64, 13.333_f64);
        let band_top = baseline - fs; // 줄 상자 윗변
        let glyph_top = baseline - fs * 0.8; // 한글 글자 윗변
        for kind in 1..=6u8 {
            for prim in emphasis_mark(kind, 50.0, baseline, fs, band_top) {
                let (lo, hi) = match prim {
                    EmphasisPrim::Circle {
                        cy,
                        r,
                        stroke_width,
                        ..
                    } => (cy - r - stroke_width / 2.0, cy + r + stroke_width / 2.0),
                    EmphasisPrim::Polyline { points, width } => {
                        let min = points.iter().map(|p| p.1).fold(f64::MAX, f64::min);
                        let max = points.iter().map(|p| p.1).fold(f64::MIN, f64::max);
                        (min - width / 2.0, max + width / 2.0)
                    }
                };
                assert!(
                    lo >= band_top,
                    "강조점 {kind} 이 줄 상자 위로 나간다: {lo} < {band_top}"
                );
                assert!(
                    hi <= glyph_top,
                    "강조점 {kind} 이 글자와 겹친다: {hi} > {glyph_top}"
                );
            }
        }
    }
}
