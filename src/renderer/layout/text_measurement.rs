//! 텍스트 폭 측정, 문자 클러스터 분할, CJK 판별 관련 함수

use super::super::font_metrics_data;
use super::super::style_resolver::ResolvedStyleSet;
use super::super::{hwpunit_to_px, TabLeaderInfo, TabStop, TextStyle};
use crate::model::style::UnderlineType;

// ── TextMeasurer trait ──────────────────────────────────────────────

/// 텍스트 폭 측정 추상화 트레이트
///
/// 플랫폼별 텍스트 측정 구현체를 추상화한다.
/// - EmbeddedTextMeasurer: 내장 폰트 메트릭 기반 (모든 플랫폼)
/// - WasmTextMeasurer: JS Canvas 브릿지 + 내장 메트릭 (WASM 전용)
pub trait TextMeasurer {
    /// 텍스트 전체 폭 추정 (px)
    fn estimate_text_width(&self, text: &str, style: &TextStyle) -> f64;
    /// 글자별 X 위치 경계값 계산 (N글자 → N+1개 경계)
    fn compute_char_positions(&self, text: &str, style: &TextStyle) -> Vec<f64>;
}

// ── 공통 헬퍼 ───────────────────────────────────────────────────────

/// 자모 클러스터 길이 매핑 계산
///
/// 한글 자모 조합(초+중+종)을 1개 클러스터로 묶는다.
/// cluster_len[i] > 0: 클러스터 시작 (길이), 0: 클러스터 내부 (이전 문자와 동일 위치)
/// 글꼴에 글리프가 없을 때의 **폴백 폭 사다리** — 폭 계산과 캐럿 위치가 같은 답을 내야 한다.
///
/// ⚠ 이 사다리는 한때 `estimate_text_width_unrounded` 와 `compute_char_positions` 에
/// **따로 복사돼 있었다**. 그래서 이모지 폭을 한쪽만 고쳤더니 줄바꿈은 바뀌고 캐럿은
/// 그대로여서 글자가 겹쳤다(2026-07-31 실측). 한 곳에서만 고치게 여기로 모은다.
pub(crate) fn fallback_char_width(
    font_family: &str,
    c: char,
    cluster_wide: bool,
    font_size: f64,
) -> f64 {
    if is_emoji_presentation(c) {
        // 컬러 이모지는 시스템 글꼴이 그린다 — 1em 으로 잡으면 뒤 글자를 덮는다
        font_size * EMOJI_ADVANCE_EM
    } else if cluster_wide || is_cjk_char(c) || is_fullwidth_symbol(c) || is_emoji_wide(c) {
        font_size
    } else if is_narrow_punctuation(c) || is_narrow_paren_for_font(font_family, c) {
        // Task #257: 콤마·중점 등 narrow glyph 폴백 폭 (0.5 → 0.3).
        font_size * 0.3
    } else {
        font_size * 0.5
    }
}

/// 이모지는 **전각(1em)** 으로 잰다.
///
/// 왜: 문서 글꼴에 이모지 글리프가 없으면 브라우저/시스템 컬러 이모지가 대신 그려지는데,
/// 그 글리프는 거의 항상 1em 이다. 그런데 폴백 폭이 0.5em 이라 연속으로 놓으면 글자가
/// 서로 겹치고 **뒤 글자를 덮어버렸다**(2026-07-31 실측: "앞 😀😀😀 뒤" → " 뒤" 사라짐).
/// 사이에 다른 글자가 끼면 티가 안 나서 오래 남아 있던 결함이다.
///
/// ⚠ 이 판정은 `measure_char_width_embedded` 가 **실패한 뒤에만** 쓰인다 — 문서 글꼴이
/// 그 글자를 실제로 가지고 있으면 그 폭이 이긴다. 그래서 ★·✓ 같은 기호를 좁게 가진
/// 한글 글꼴을 망치지 않는다.
fn is_emoji_wide(c: char) -> bool {
    matches!(c as u32,
        0x1F300..=0x1FAFF   // 그림문자·사람·동물·음식·기호 본진
        | 0x1F000..=0x1F02F // 마작
        | 0x1F0A0..=0x1F0FF // 트럼프
        | 0x1F100..=0x1F1FF // 괄호 문자·지역 표시(국기)
        | 0x2600..=0x27BF   // 기타 기호·딩벳(☀ ✅ ✈ …)
        | 0x2B00..=0x2BFF   // 화살표·별
        | 0x3030 | 0x303D | 0x3297 | 0x3299
    )
}

/// **컬러 이모지로 그려지는** 글자인가 (유니코드 Emoji_Presentation=Yes).
///
/// 왜 `is_emoji_wide` 와 따로 두나(2026-08-01 실측):
/// 같은 U+2600–27BF 블록 안에서도 ✅(U+2705)는 컬러 이모지로 **1.28em** 이고
/// ✈(U+2708)·★·☀ 는 문서 글꼴의 흑백 기호로 **0.97em** 이다. 블록으로는 못 가른다 —
/// 가르는 건 Emoji_Presentation 속성이다(변이 선택자 FE0F 없이도 컬러로 뜨는 글자).
/// 아래 목록은 그 속성의 실제 범위다(추측 아님).
///
/// ⚠ 이 판정이 틀리면 두 방향으로 깨진다: 컬러인데 좁게 잡으면 **뒤 글자를 덮고**,
///   흑백인데 넓게 잡으면 **글자 사이가 벌어진다**. 둘 다 실측으로 확인할 것.
pub(crate) fn is_emoji_presentation(c: char) -> bool {
    matches!(c as u32,
        0x231A..=0x231B | 0x23E9..=0x23EC | 0x23F0 | 0x23F3
        | 0x25FD..=0x25FE | 0x2614..=0x2615 | 0x2648..=0x2653 | 0x267F
        | 0x2693 | 0x26A1 | 0x26AA..=0x26AB | 0x26BD..=0x26BE
        | 0x26C4..=0x26C5 | 0x26CE | 0x26D4 | 0x26EA | 0x26F2..=0x26F3
        | 0x26F5 | 0x26FA | 0x26FD | 0x2705 | 0x270A..=0x270B | 0x2728
        | 0x274C | 0x274E | 0x2753..=0x2755 | 0x2757 | 0x2795..=0x2797
        | 0x27B0 | 0x27BF | 0x2B1B..=0x2B1C | 0x2B50 | 0x2B55
        | 0x1F004 | 0x1F0CF | 0x1F18E | 0x1F191..=0x1F19A
        | 0x1F1E6..=0x1F1FF // 지역 표시(국기)
        | 0x1F201 | 0x1F21A | 0x1F22F | 0x1F232..=0x1F236 | 0x1F238..=0x1F23A
        | 0x1F250..=0x1F251 | 0x1F300..=0x1F320 | 0x1F32D..=0x1F335
        | 0x1F337..=0x1F37C | 0x1F37E..=0x1F393 | 0x1F3A0..=0x1F3CA
        | 0x1F3CF..=0x1F3D3 | 0x1F3E0..=0x1F3F0 | 0x1F3F4 | 0x1F3F8..=0x1F43E
        | 0x1F440 | 0x1F442..=0x1F4FC | 0x1F4FF..=0x1F53D | 0x1F54B..=0x1F54E
        | 0x1F550..=0x1F567 | 0x1F57A | 0x1F595..=0x1F596 | 0x1F5A4
        | 0x1F5FB..=0x1F64F | 0x1F680..=0x1F6C5 | 0x1F6CC | 0x1F6D0..=0x1F6D2
        | 0x1F6D5..=0x1F6D7 | 0x1F6EB..=0x1F6EC | 0x1F6F4..=0x1F6FC
        | 0x1F7E0..=0x1F7EB | 0x1F90C..=0x1F93A | 0x1F93C..=0x1F945
        | 0x1F947..=0x1F9FF | 0x1FA70..=0x1FAFF
    )
}

/// 컬러 이모지가 차지하는 진행폭 — 한글 글자처럼 **한 em 칸**을 쓴다.
///
/// 이력: 2026-08-01 에 1.0 → 1.28 로 키웠다(잉크 실측 17px = 1.28em). 그런데 폭만
/// 키우고 높이를 안 건드려 이모지가 줄 아래로 처졌다 — 잉크 높이 20px(asc 15 + desc 5)
/// 대 한글 12.53px(asc 10.52 + desc 2.01), **baseline 아래로 2.5배** 깊었다(2026-08-02
/// 실측, 사용자 지적 "이모지 근처가 아래로 내려간다").
/// 지금은 글리프를 EMOJI_GLYPH_SCALE 로 줄여 한글과 같은 높이로 그리므로, 진행폭도
/// 한글과 같은 한 em 으로 되돌린다(사용자 선택: "글자 높이에 맞추기").
/// ⚠ 값이 플랫폼마다 다르다 — WASM 에서는 아래 measure_char_width_hwp 가 브라우저에
///   직접 물어 이 상수를 덮는다. 이 상수는 native(브라우저 없음) 폴백이다.
pub(crate) const EMOJI_ADVANCE_EM: f64 = 1.0;

/// 컬러 이모지 글리프 배율 — **1.0(원래 크기)**.
///
/// 2026-08-04 사용자 판정: 구글 독스는 이모지를 글자 크기 그대로 그려 선택 상자에
/// 꽉 차는데 우리는 줄여 그려 "칸 안에서 붕 뜬" 모습이었다(10pt 비교 스크린샷).
/// 축소(0.63→0.76→…)와 들어올림을 손으로 맞추는 시도가 두 번 회귀를 냈다 — 브라우저·
/// 독스와 같은 무보정(1.0em, 기준선 그대로)으로 되돌린다. 어드밴스도 1.0em 이라
/// 글리프가 자기 칸을 정확히 채운다.
pub(crate) const EMOJI_GLYPH_SCALE: f64 = 1.0;

/// baseline 올림(em) — **0(무보정)**.
///
/// 잉크 실측(100px): 😀 top −0.88em·bottom +0.11em, 한글 '가' −0.80/+0.15. 이모지는
/// 원래 기준선 글리프라 브라우저가 놓는 자리가 정답이다 — 손으로 올리면(옛 0.135, 뒤에
/// 0.032) 글자와 어긋난다. 구글 독스도 무보정이다.
pub(crate) const EMOJI_BASELINE_LIFT_EM: f64 = 0.0;

/// 그려지는 크기 기준 이모지 잉크 폭 배율 — 축소 글리프를 진행폭 안에 가운데 둘 때 쓴다.
/// 실측: 😀 잉크 폭 = 어드밴스와 같은 1.0em(좌측 베어링 0). 옛 값 1.28 은 어드밴스를
/// 원본 em 으로 나눈 수치를 잉크로 오인한 것이라 중앙 보정이 9배 부족했다(왼쪽 쏠림).
pub(crate) const EMOJI_INK_EM: f64 = 1.0;

/// 이모지 클러스터의 그리기 오프셋 — (가로 중앙 보정, baseline 올림) 픽셀.
/// web_canvas(화면)·svg(인쇄) 두 경로가 **반드시 같은 값**을 쓰도록 한 곳에 둔다.
pub(crate) fn emoji_draw_offsets(font_size: f64, advance: f64) -> (f64, f64) {
    let scaled = font_size * EMOJI_GLYPH_SCALE;
    let ink_w = scaled * EMOJI_INK_EM;
    let dx = ((advance - ink_w) / 2.0).max(0.0);
    (dx, font_size * EMOJI_BASELINE_LIFT_EM)
}

/// 이모지 결합용 문자 — 앞 글자에 붙어 그려지므로 폭 0.
/// (변이 선택자·ZWJ·피부색 수정자·keycap 결합자)
fn is_emoji_zero_width(c: char) -> bool {
    matches!(
        c as u32,
        0xFE0E | 0xFE0F       // 변이 선택자 15/16
        | 0x200D              // ZWJ (가족·직업 조합)
        | 0x1F3FB
            ..=0x1F3FF   // 피부색 수정자
        | 0x20E3 // keycap 결합자
    )
}

fn build_cluster_len(chars: &[char]) -> Vec<u8> {
    let char_count = chars.len();
    let mut cluster_len = vec![0u8; char_count];
    let mut ci = 0;
    while ci < char_count {
        if is_hangul_choseong(chars[ci]) {
            let start = ci;
            ci += 1;
            if ci < char_count && is_hangul_jungseong(chars[ci]) {
                ci += 1;
                if ci < char_count && is_hangul_jongseong(chars[ci]) {
                    ci += 1;
                }
            }
            cluster_len[start] = (ci - start) as u8;
        } else {
            cluster_len[ci] = 1;
            ci += 1;
        }
    }
    cluster_len
}

/// 스타일에서 공통 파라미터 추출 (font_size, ratio, tab_w)
fn style_params(style: &TextStyle) -> (f64, f64, f64) {
    let font_size = if style.font_size > 0.0 {
        style.font_size
    } else {
        12.0
    };
    let ratio = if style.ratio > 0.0 { style.ratio } else { 1.0 };
    let tab_w = if style.default_tab_width > 0.0 {
        style.default_tab_width
    } else {
        font_size * 4.0
    };
    (font_size, ratio, tab_w)
}

/// inline_tabs ext[2] 에서 탭 종류를 추출.
///
/// HWP `tab_extended` 포맷 (PR #292 / Task #290 실증):
/// - high byte = 탭 종류 enum+1 (1=LEFT, 2=RIGHT, 3=CENTER, 4=DECIMAL)
/// - low  byte = fill_type (TabDef.fill 과 동일)
///
/// 기존 코드는 `ext[2]` 전체 u16 을 탭 종류로 해석하여 실제 HWP 값(최소 256)과
/// 매칭 실패. 이 헬퍼로 고바이트만 추출해 0~4 값으로 정규화.
#[inline]
pub(super) fn inline_tab_type(ext: &[u16; 7]) -> u8 {
    ((ext[2] >> 8) & 0xFF) as u8
}

/// 현재 절대 위치에서 다음 탭 정지를 찾는다.
///
/// Returns (position, tab_type, fill_type).
/// 커스텀 탭이 없으면 기본 등간격 탭을 사용한다.
pub(crate) fn find_next_tab_stop(
    abs_x: f64,
    tab_stops: &[TabStop],
    default_tab_width: f64,
    auto_tab_right: bool,
    available_width: f64,
) -> (f64, u8, u8) {
    // 커스텀 탭 정지에서 현재 위치 뒤의 첫 번째 검색
    for ts in tab_stops {
        // type=1(오른쪽) 탭은 단 기준 절대 위치이므로 available_width 클램핑 제외.
        // 들여쓰기(left_margin)가 있는 문단에서도 오른쪽 탭이 동일 위치에 정렬되도록 한다.
        // type=0(왼쪽)/2(가운데) 탭은 종전대로 클램핑하여 텍스트 영역 밖으로 넘어가지 않게 한다.
        let pos = if ts.tab_type != 1 && ts.position > available_width && available_width > 0.0 {
            available_width
        } else {
            ts.position
        };
        if pos > abs_x + 0.5 {
            return (pos, ts.tab_type, ts.fill_type);
        }
    }
    // auto_tab_right: 커스텀 탭이 모두 지나갔으면 오른쪽 끝을 right 탭으로
    if auto_tab_right && available_width > abs_x + 0.5 {
        return (available_width, 1, 0); // type=1(오른쪽), fill=0(없음)
    }
    // 기본 등간격 탭
    let tab_w = if default_tab_width > 0.0 {
        default_tab_width
    } else {
        48.0
    };
    let next = ((abs_x / tab_w).floor() + 1.0) * tab_w;
    (next, 0, 0) // type=0(왼쪽), fill=0(없음)
}

/// 지정 인덱스부터 다음 탭(또는 문자열 끝)까지의 세그먼트 폭을 측정한다.
fn measure_segment_from(
    chars: &[char],
    cluster_len: &[u8],
    start: usize,
    char_width: &dyn Fn(usize) -> f64,
) -> f64 {
    let mut w = 0.0;
    for i in start..chars.len() {
        if chars[i] == '\t' {
            break;
        }
        if cluster_len[i] == 0 {
            continue;
        }
        w += char_width(i);
    }
    w
}

fn tab_suffix_is_ascii_page_number(chars: &[char], start: usize) -> bool {
    let mut seen_digit = false;
    for ch in chars.iter().skip(start) {
        if *ch == '\t' {
            return false;
        }
        if ch.is_whitespace() {
            continue;
        }
        if ch.is_ascii_digit() {
            seen_digit = true;
            continue;
        }
        return false;
    }
    seen_digit
}

fn right_leader_tab_target_rel(style: &TextStyle, font_size: f64) -> Option<f64> {
    style
        .tab_stops
        .iter()
        .rev()
        .find(|tab| tab.tab_type == 1 && tab.fill_type != 0)
        .map(|tab| tab.position - font_size * 0.25 - style.line_x_offset)
        .filter(|target| target.is_finite())
}

fn right_leader_tab_fill(style: &TextStyle) -> Option<u8> {
    style
        .tab_stops
        .iter()
        .rev()
        .find(|tab| tab.tab_type == 1 && tab.fill_type != 0)
        .map(|tab| tab.fill_type)
}

fn right_leader_body_target_rel(style: &TextStyle) -> Option<f64> {
    if style.available_width <= 0.0 || right_leader_tab_fill(style).is_none() {
        return None;
    }
    let target = style.text_start_offset + style.available_width - style.line_x_offset;
    if target.is_finite() {
        Some(target)
    } else {
        None
    }
}

/// 탭 리더 추출 (tab_extended 지원)
/// tab_extended: HWPX 인라인 탭 또는 HWP 탭 확장 데이터 (ext[1] = leader/fill_type)
pub fn extract_tab_leaders_with_extended(
    text: &str,
    positions: &[f64],
    style: &TextStyle,
    tab_extended: &[[u16; 7]],
) -> Vec<TabLeaderInfo> {
    let chars: Vec<char> = text.chars().collect();
    let tab_w = if style.default_tab_width > 0.0 {
        style.default_tab_width
    } else {
        48.0
    };
    let mut leaders = Vec::new();
    let mut tab_idx = 0usize; // tab_extended 인덱스
    for (i, c) in text.chars().enumerate() {
        if c == '\t' && i + 1 < positions.len() {
            let before_x = positions[i];
            let after_x = positions[i + 1];
            let has_more_tabs_after = chars.iter().skip(i + 1).any(|ch| *ch == '\t');
            let tabdef_page_number_fill = if tab_extended.is_empty()
                && !has_more_tabs_after
                && tab_suffix_is_ascii_page_number(&chars, i + 1)
            {
                right_leader_tab_fill(style)
            } else {
                None
            };

            // 1. tab_extended에서 leader 가져오기 (HWPX 인라인 탭)
            let ext_fill = if tab_idx < tab_extended.len() {
                tab_extended[tab_idx][1] as u8 // ext[1] = leader/fill_type
            } else {
                0
            };

            // 2. TabDef에서 fill_type 가져오기 (HWP TabDef)
            let tabdef_fill = if let Some(fill) = tabdef_page_number_fill {
                fill
            } else if !style.tab_stops.is_empty() || style.auto_tab_right {
                let abs_before = style.line_x_offset + before_x;
                let (_, _, ft) = find_next_tab_stop(
                    abs_before,
                    &style.tab_stops,
                    tab_w,
                    style.auto_tab_right,
                    style.available_width,
                );
                ft
            } else {
                0
            };

            // 둘 중 하나라도 fill이 있으면 리더 추가
            // 오른쪽 정렬 텍스트 앞에 공백 1개 간격 확보
            let fill_type = if ext_fill > 0 { ext_fill } else { tabdef_fill };
            if fill_type > 0 && after_x > before_x + 1.0 {
                let space_gap = style.font_size * 0.25;
                let content_x = text.chars().enumerate().skip(i + 1).find_map(|(j, ch)| {
                    if ch != '\t' && !ch.is_whitespace() && j < positions.len() {
                        Some(positions[j])
                    } else {
                        None
                    }
                });
                let end_x = content_x
                    .map(|x| x - space_gap)
                    .unwrap_or(after_x - space_gap)
                    .min(after_x - space_gap);
                leaders.push(TabLeaderInfo {
                    start_x: before_x,
                    end_x: end_x.max(before_x),
                    fill_type,
                });
            }
            tab_idx += 1;
        }
    }
    if leaders.len() > 1 {
        let mut min_following_end = f64::INFINITY;
        for leader in leaders.iter_mut().rev() {
            if min_following_end.is_finite() && leader.end_x > min_following_end {
                leader.end_x = min_following_end.max(leader.start_x);
            }
            min_following_end = min_following_end.min(leader.end_x);
        }
    }
    leaders
}

// ── EmbeddedTextMeasurer ────────────────────────────────────────────

/// 내장 폰트 메트릭 기반 텍스트 측정기
///
/// font_metrics_data의 582개 폰트 메트릭을 사용하여 문자 폭을 측정한다.
/// 메트릭이 없는 폰트는 CJK=font_size, Latin=font_size×0.5 휴리스틱을 사용한다.
/// 모든 플랫폼에서 동일하게 동작한다 (WASM 포함).
/// [#2132] 공용 글자-워크 — Embedded/Wasm measurer 의 compute_char_positions 중복 소거.
/// 폭 산출원(char_px_raw)과 인라인 탭 divergent 경로(inline_tab_x)만 measurer 별 훅.
/// 나머지(특수문자, dash leader, 자간 클램프, 공백, 커스텀/기본 탭)는 1벌.
fn compute_char_positions_walk(
    text: &str,
    style: &TextStyle,
    char_px_raw: &dyn Fn(usize, char, &[char], &[u8]) -> f64,
    inline_tab_x: &dyn Fn(usize, f64, &[u16; 7], &[char], &[u8], &dyn Fn(usize) -> f64) -> f64,
) -> Vec<f64> {
    let (font_size, ratio, tab_w) = style_params(style);
    let chars: Vec<char> = text.chars().collect();
    let char_count = chars.len();
    let mut positions = Vec::with_capacity(char_count + 1);
    let mut x = 0.0;
    positions.push(x);

    let cluster_len = build_cluster_len(&chars);
    let has_custom_tabs = !style.tab_stops.is_empty() || style.auto_tab_right;

    let char_width = |i: usize| -> f64 {
        let c = chars[i];
        if c == '\u{2007}' {
            return font_size * 0.5 * ratio + style.letter_spacing + style.extra_char_spacing;
        }
        // 인라인 객체 placeholder 는 실제 control node 가 따로 그리므로 텍스트 폭은 0.
        if c == '\u{FFFC}' {
            return 0.0;
        }
        // [Issue #677] HWP PUA 채움 문자 (U+F081C) — 시각 폭 0 (한컴 PDF 정합).
        if c == '\u{F081C}' {
            return 0.0;
        }
        // 이모지 결합 문자(변이 선택자·ZWJ·피부색)는 앞 글자에 붙어 그려진다 — 폭 0.
        if is_emoji_zero_width(c) {
            return 0.0;
        }
        let char_px_raw = char_px_raw(i, c, &chars, &cluster_len);
        // Task #352: dash leader 좁은 base 0.3 em + extra_dash_advance.
        let is_leader = is_dash_leader_run(&chars, i);
        let char_px = if is_leader {
            char_px_raw.min(font_size * 0.3)
        } else {
            char_px_raw
        };
        let mut w = char_px * ratio + style.letter_spacing + style.extra_char_spacing;
        if c == ' ' {
            w += style.extra_word_spacing;
        }
        if is_leader {
            w += style.extra_dash_advance;
        }
        // 음수 자간(letter_spacing + extra_char_spacing < 0) 시
        // per-char 최소 advance 클램프로 narrow glyph 역진 방지.
        if style.letter_spacing + style.extra_char_spacing < 0.0 {
            let min_w = char_px * ratio * 0.5;
            w = w.max(min_w);
        }
        w
    };

    let mut tab_char_idx = 0usize; // inline_tabs 인덱스
    for i in 0..char_count {
        let c = chars[i];
        if cluster_len[i] == 0 {
            positions.push(x);
            continue;
        }
        if c == '\t' {
            if tab_char_idx < style.inline_tabs.len() {
                let ext = &style.inline_tabs[tab_char_idx];
                x = inline_tab_x(i, x, ext, &chars, &cluster_len, &char_width);
                tab_char_idx += 1;
            } else if has_custom_tabs {
                let has_more_tabs_after = chars[i + 1..].contains(&'\t');
                if !has_more_tabs_after && tab_suffix_is_ascii_page_number(&chars, i + 1) {
                    if let Some(target_rel) = right_leader_body_target_rel(style) {
                        let seg_w = measure_segment_from(&chars, &cluster_len, i + 1, &char_width);
                        x = (target_rel - seg_w).max(x);
                        tab_char_idx += 1;
                        positions.push(x);
                        continue;
                    }
                }
                let abs_x = style.line_x_offset + x;
                let (tab_pos, tab_type, fill_type) = find_next_tab_stop(
                    abs_x,
                    &style.tab_stops,
                    tab_w,
                    style.auto_tab_right,
                    style.available_width,
                );
                let rel_tab = tab_pos - style.line_x_offset;
                // [Task #874] auto_tab_right / leader RIGHT 탭은 col-relative 우측 끝
                // (= text_start_offset + available_width) 까지 정렬.
                let effective_rel_tab = if tab_type == 1
                    && style.available_width > 0.0
                    && (fill_type != 0 || style.auto_tab_right)
                {
                    style.text_start_offset + style.available_width - style.line_x_offset
                } else {
                    rel_tab
                };
                match tab_type {
                    1 => {
                        // 오른쪽
                        let seg_start = if fill_type != 0 {
                            i + 1
                        } else {
                            let mut s = i + 1;
                            while s < chars.len() && chars[s] == ' ' && cluster_len[s] != 0 {
                                s += 1;
                            }
                            s
                        };
                        let seg_w =
                            measure_segment_from(&chars, &cluster_len, seg_start, &char_width);
                        x = (effective_rel_tab - seg_w).max(x);
                    }
                    2 => {
                        // 가운데
                        let seg_w = measure_segment_from(&chars, &cluster_len, i + 1, &char_width);
                        x = (rel_tab - seg_w / 2.0).max(x);
                    }
                    _ => {
                        // 왼쪽(0), 소수점(3)
                        x = rel_tab.max(x);
                    }
                }
                tab_char_idx += 1;
            } else {
                // 기본 등간격 탭: 라인 절대 위치(line_x_offset + x) 기준으로 계산.
                let abs_x = style.line_x_offset + x;
                let next_abs = ((abs_x / tab_w).floor() + 1.0) * tab_w;
                x = (next_abs - style.line_x_offset).max(x);
                tab_char_idx += 1;
            }
            positions.push(x);
            continue;
        }
        x += char_width(i);
        positions.push(x);
    }

    positions
}

pub struct EmbeddedTextMeasurer;

impl TextMeasurer for EmbeddedTextMeasurer {
    fn estimate_text_width(&self, text: &str, style: &TextStyle) -> f64 {
        let (font_size, ratio, tab_w) = style_params(style);
        let chars: Vec<char> = text.chars().collect();
        let cluster_len = build_cluster_len(&chars);
        let char_count = chars.len();
        let has_custom_tabs = !style.tab_stops.is_empty() || style.auto_tab_right;

        let char_width = |i: usize| -> f64 {
            let c = chars[i];
            if c == '\u{2007}' {
                return font_size * 0.5 * ratio + style.letter_spacing + style.extra_char_spacing;
            }
            // 인라인 객체 placeholder 는 실제 control node 가 따로 그리므로 텍스트 폭은 0.
            if c == '\u{FFFC}' {
                return 0.0;
            }
            // [Issue #677] HWP PUA 채움 문자 (U+F081C) — 시각 폭 0
            // 한컴이 인라인 TAC 표/도형 앞에 삽입하는 placeholder 채움 문자.
            // 한컴 PDF 정합 — 폭 0 으로 라인 inline x 에 영향 없음. fillers 가
            // 표 너비만큼 (≈97 chars × 1 char width = table width) 채워져
            // 표가 fillers 영역 위에 시각적으로 겹쳐 column-left 출력 패턴.
            if c == '\u{F081C}' {
                return 0.0;
            }
            let base_w_raw = if let Some(w) = (c == '\u{318D}')
                .then(|| area_dot_fallback_width(&style.font_family, font_size))
                .flatten()
            {
                w
            } else if let Some(w) = measure_char_width_embedded(
                &style.font_family,
                style.bold,
                style.italic,
                c,
                font_size,
            ) {
                w
            } else {
                // Task #257(narrow punct 0.3em) 포함 — 사다리는 fallback_char_width 한 곳뿐이다.
                fallback_char_width(&style.font_family, c, cluster_len[i] > 1, font_size)
            };
            // Task #352: 3+ 연속 dash 시퀀스(빈칸/leader) 는 좁은 폭으로 재산출.
            // HY신명조 등 한글 폰트 메트릭의 ASCII '-' 폭(0.83 em) 부풀림 회피.
            // 좁은 base 0.3 em 위에 paragraph_layout 가 라인 슬랙을 분배한
            // extra_dash_advance 를 추가하여 PDF 의 elastic leader 동작 모방.
            let is_leader = is_dash_leader_run(&chars, i);
            let base_w = if is_leader {
                base_w_raw.min(font_size * 0.3)
            } else {
                base_w_raw
            };
            let mut w = base_w * ratio + style.letter_spacing + style.extra_char_spacing;
            if c == ' ' {
                w += style.extra_word_spacing;
            }
            if is_leader {
                w += style.extra_dash_advance;
            }
            // 음수 자간(letter_spacing + extra_char_spacing < 0) 시
            // per-char 최소 advance = base*ratio*0.5 로 클램프하여 narrow
            // glyph(콤마/마침표 등) 이 뒷 글자와 역진 겹침되는 것을 방지한다.
            // 문서 CharShape 의 음수 자간 및 paragraph_layout 의 압축 모두 포함.
            if style.letter_spacing + style.extra_char_spacing < 0.0 {
                let min_w = base_w * ratio * 0.5;
                w = w.max(min_w);
            }
            w
        };

        let mut total = 0.0;
        let mut tab_char_idx = 0usize;
        for i in 0..char_count {
            let c = chars[i];
            if cluster_len[i] == 0 {
                continue;
            }
            if c == '\t' {
                // 인라인 탭 (HWP tab_extended / HWPX 인라인 탭)
                // NOTE: 네이티브 경로는 `tab_type = ext[2]` 전체 u16 해석을 유지.
                // 기존 golden SVG (issue-147, issue-267) 가 이 "우연한 LEFT 폴백" 동작에
                // 의존하고 있어, 이를 바꾸면 회귀 발생. WASM 경로만 inline_tab_type 사용.
                // [Issue #630 Stage 4 검증] HWP5 의 `ext[0]` 가 이미 right-tab 결과 위치
                // (= 우측 끝 - 한컴_seg_w) 로 저장되어 있어 LEFT fallback 이 인코딩 의도와
                // 정합. RIGHT 정확 매치 시 seg_w 이중 차감 → ≈seg_w (≈112px) 좌측 이탈
                // (aift p4 1-1 등 23/24 라인 모두 영향). 본 LEFT fallback 동작 유지.
                if tab_char_idx < style.inline_tabs.len() {
                    let ext = &style.inline_tabs[tab_char_idx];
                    let tab_width_px = ext[0] as f64 * 96.0 / 7200.0;
                    let tab_type = ext[2];
                    let tab_target = total + tab_width_px;
                    // [Task #874] auto_tab_right 가 활성된 paragraph 에서 단일 tab 의
                    // 인라인 tab_extended 는 Hancom 의 right-tab 결과 위치(= 우측 끝 -
                    // 한컴_seg_w) 를 ext[0] 로 저장. 우리 폰트의 seg_w 와 다르면 좌측
                    // 이탈 발생 (shortcut.hwp pi=144 `Alt+Shift+C` 27 px 부족). auto_right
                    // 일 때는 우리 metric 기준 right-edge - our_seg_w 로 override.
                    let has_more_tabs_after = chars[i + 1..].contains(&'\t');
                    // [Task #874 #10] ext[2] high-byte 가 명시적 LEFT(1)/DECIMAL(4) 면
                    // auto_tab_right paragraph 라도 override 금지 — exam_math.hwp p7
                    // item 18 (Task #290) 의 inline LEFT tab 회귀 차단.
                    let inline_type_hi = ((tab_type >> 8) & 0xFF) as u8;
                    let inline_is_explicit_left = inline_type_hi == 1 || inline_type_hi == 4;
                    let override_to_right = style.auto_tab_right
                        && !has_more_tabs_after
                        && style.available_width > 0.0
                        && !inline_is_explicit_left;
                    if override_to_right {
                        // [Task #874 #2] lang split 로 post-tab 콘텐츠가 후속 run 으로
                        // 쪼개진 경우 (예: "F3→Alt+I" → "F3"/"→"/"Alt+I"), 현재 run 내부
                        // 측정만으로는 seg_w 가 부족. paragraph_layout 이 미리 합산한
                        // block_w override 가 있으면 그것을 사용.
                        let seg_w = style.right_tab_block_width_override.unwrap_or_else(|| {
                            measure_segment_from(&chars, &cluster_len, i + 1, &char_width)
                        });
                        let right_edge_rel =
                            style.text_start_offset + style.available_width - style.line_x_offset;
                        total = (right_edge_rel - seg_w).max(total);
                    } else if inline_type_hi == 0
                        && !has_more_tabs_after
                        && tab_suffix_is_ascii_page_number(&chars, i + 1)
                    {
                        if let Some(target_rel) = right_leader_tab_target_rel(style, font_size) {
                            let seg_w =
                                measure_segment_from(&chars, &cluster_len, i + 1, &char_width);
                            total = (target_rel - seg_w).max(total);
                        } else {
                            total = tab_target.max(total);
                        }
                    } else {
                        match tab_type {
                            1 => {
                                let seg_w =
                                    measure_segment_from(&chars, &cluster_len, i + 1, &char_width);
                                total = (tab_target - seg_w).max(total);
                            }
                            2 => {
                                let seg_w =
                                    measure_segment_from(&chars, &cluster_len, i + 1, &char_width);
                                total = (tab_target - seg_w / 2.0).max(total);
                            }
                            _ => {
                                total = tab_target.max(total);
                            }
                        }
                    }
                    tab_char_idx += 1;
                } else if has_custom_tabs {
                    let has_more_tabs_after = chars[i + 1..].contains(&'\t');
                    if !has_more_tabs_after && tab_suffix_is_ascii_page_number(&chars, i + 1) {
                        if let Some(target_rel) = right_leader_body_target_rel(style) {
                            let seg_w =
                                measure_segment_from(&chars, &cluster_len, i + 1, &char_width);
                            total = (target_rel - seg_w).max(total);
                            tab_char_idx += 1;
                            continue;
                        }
                    }
                    let abs_x = style.line_x_offset + total;
                    let (tab_pos, tab_type, fill_type) = find_next_tab_stop(
                        abs_x,
                        &style.tab_stops,
                        tab_w,
                        style.auto_tab_right,
                        style.available_width,
                    );
                    let rel_tab = tab_pos - style.line_x_offset;
                    // [Task #874] auto_tab_right 의 tab_pos = available_width 는 텍스트
                    // 영역 시작 기준 상대값. col-relative 우측 끝 = text_start_offset +
                    // available_width. line_x_offset 도 col-relative 이므로 변환.
                    let effective_rel_tab = if tab_type == 1
                        && style.available_width > 0.0
                        && (fill_type != 0 || style.auto_tab_right)
                    {
                        style.text_start_offset + style.available_width - style.line_x_offset
                    } else {
                        rel_tab
                    };
                    match tab_type {
                        1 => {
                            // 오른쪽
                            let seg_w =
                                measure_segment_from(&chars, &cluster_len, i + 1, &char_width);
                            total = (effective_rel_tab - seg_w).max(total);
                        }
                        2 => {
                            // 가운데
                            let seg_w =
                                measure_segment_from(&chars, &cluster_len, i + 1, &char_width);
                            total = (rel_tab - seg_w / 2.0).max(total);
                        }
                        _ => {
                            // 왼쪽(0), 소수점(3) → 왼쪽과 동일 처리
                            total = rel_tab.max(total);
                        }
                    }
                    tab_char_idx += 1;
                } else {
                    // 기본 등간격 탭: 라인 절대 위치(line_x_offset + total) 기준으로 계산
                    let abs_x = style.line_x_offset + total;
                    let next_abs = ((abs_x / tab_w).floor() + 1.0) * tab_w;
                    total = (next_abs - style.line_x_offset).max(total);
                    tab_char_idx += 1;
                }
                continue;
            }
            if cluster_len[i] == 0 {
                continue;
            }
            total += char_width(i);
        }
        total.round()
    }

    fn compute_char_positions(&self, text: &str, style: &TextStyle) -> Vec<f64> {
        let (font_size, _ratio, _tab_w) = style_params(style);
        // [#2132] 폭 산출원 훅 — embedded 메트릭 lookup + 폴백 사다리 (Task #257 포함).
        let char_px_raw = |_i: usize, c: char, _chars: &[char], cluster_len: &[u8]| -> f64 {
            let i = _i;
            if let Some(w) = (c == '\u{318D}')
                .then(|| area_dot_fallback_width(&style.font_family, font_size))
                .flatten()
            {
                w
            } else if let Some(w) = measure_char_width_embedded(
                &style.font_family,
                style.bold,
                style.italic,
                c,
                font_size,
            ) {
                w
            } else {
                fallback_char_width(&style.font_family, c, cluster_len[i] > 1, font_size)
            }
        };
        // [#2132] 인라인 탭 divergent 경로 훅 — HWP5 raw ext 인코딩 legacy 해석 유지
        // (Issue #630 Stage 4/6, Task #874 계열 — 원본 무변경 이동).
        let inline_tab_x = |i: usize,
                            x_in: f64,
                            ext: &[u16; 7],
                            chars: &[char],
                            cluster_len: &[u8],
                            char_width: &dyn Fn(usize) -> f64|
         -> f64 {
            let mut x = x_in;
            let tab_width_px = ext[0] as f64 * 96.0 / 7200.0;
            let tab_type_raw = ext[2];
            let tab_target = x + tab_width_px;
            // [Task #874] auto_tab_right paragraph + 단일 tab: ext[0] = Hancom의
            // right-tab 결과 위치 (= 우측 끝 - 한컴_seg_w). 우리 폰트의 seg_w 와 차이
            // 가 있으면 좌측 이탈. col-relative right edge - our_seg_w 로 override.
            let has_more_tabs_after = chars[i + 1..].contains(&'\t');
            // [Task #874 #10] ext[2] high-byte 가 명시적 LEFT(1)/DECIMAL(4) 면
            // auto_tab_right paragraph 라도 override 금지 — exam_math.hwp p7
            // item 18 (Task #290) 의 inline LEFT tab 회귀 차단.
            let inline_type_hi = ((tab_type_raw >> 8) & 0xFF) as u8;
            let inline_is_explicit_left = inline_type_hi == 1 || inline_type_hi == 4;
            let override_to_right = style.auto_tab_right
                && !has_more_tabs_after
                && style.available_width > 0.0
                && !inline_is_explicit_left;
            // [Issue #630 Stage 6] HWP5 inline tab `ext[2]` 인코딩 = `(enum+1)<<8 | fill`
            // 이므로 high-byte 추출이 정확. 단, RIGHT(high-byte=2) + leader(fill≠0)
            // 의 경우 한컴 ext[0] 가 이미 "(우측 끝 - 한컴_seg_w)" 까지의 거리로
            // 저장 (Stage 4 검증).
            let body_right_text_rel = if style.available_width > 0.0 {
                style.text_start_offset + style.available_width - style.line_x_offset
            } else {
                f64::INFINITY
            };
            let body_right_legacy = if style.available_width > 0.0 {
                style.available_width - style.line_x_offset
            } else {
                f64::INFINITY
            };
            if override_to_right {
                // [Task #874 #2] lang split 후속 run 합산 override.
                let seg_w = if let Some(w) = style.right_tab_block_width_override {
                    w
                } else {
                    let seg_start = {
                        let mut s = i + 1;
                        while s < chars.len() && chars[s] == ' ' && cluster_len[s] != 0 {
                            s += 1;
                        }
                        s
                    };
                    measure_segment_from(&chars, &cluster_len, seg_start, &char_width)
                };
                x = (body_right_text_rel - seg_w).max(x);
            } else if inline_type_hi == 0
                && !has_more_tabs_after
                && tab_suffix_is_ascii_page_number(&chars, i + 1)
            {
                if let Some(target_rel) = right_leader_tab_target_rel(style, font_size) {
                    let seg_w = measure_segment_from(&chars, &cluster_len, i + 1, &char_width);
                    x = (target_rel - seg_w).max(x);
                } else {
                    x = tab_target.max(x);
                }
            } else {
                let high_byte = (tab_type_raw >> 8) & 0xFF;
                let fill_low = tab_type_raw & 0xFF;
                match (high_byte, tab_type_raw) {
                    (_, 1) => {
                        // 기존 raw 1 (LEFT 또는 잘못된 RIGHT 1) — 호환 유지
                        let seg_start = {
                            let mut s = i + 1;
                            while s < chars.len() && chars[s] == ' ' && cluster_len[s] != 0 {
                                s += 1;
                            }
                            s
                        };
                        let seg_w =
                            measure_segment_from(&chars, &cluster_len, seg_start, &char_width);
                        x = (tab_target - seg_w).max(x);
                    }
                    (_, 2) => {
                        // 기존 raw 2 — 호환 유지
                        let seg_w = measure_segment_from(&chars, &cluster_len, i + 1, &char_width);
                        x = (tab_target - seg_w / 2.0).max(x);
                    }
                    (2, _) if fill_low != 0 => {
                        // [Task #874 후속] 단일-run RIGHT + leader (목차 페이지번호) —
                        // Task #874 는 cross-run RIGHT+leader 의 text_start_offset
                        // 미포함 본질을 fix (body_right_text_rel +
                        // right_tab_block_width_override). 단일-run 케이스는
                        // 여전히 body_right_legacy (= available_width - line_x_offset)
                        // 사용 → text_start_offset 미포함 으로 cell right inner
                        // (= text_start_offset + available_width) 미달. 또한 leading
                        // space skip 으로 seg_w 가 space 폭만큼 과소 → digit right
                        // edge 가 cell right inner 보다 좌측에 위치 (정렬 미달).
                        //
                        // Fix: \t 뒤 content 가 있는 단일-run 은 cell_right_run_rel
                        // (= text_start_offset + available_width - line_x_offset) 정렬
                        // + seg_w_full (i+1 부터, leading space 포함). content 없는
                        // trailing space / 끝 케이스 (= cross-run 직전) 는 원본 path
                        // 유지 (다음 run 의 pending_right_tab 분기가 처리).
                        let seg_start_skipped = {
                            let mut s = i + 1;
                            while s < chars.len() && chars[s] == ' ' && cluster_len[s] != 0 {
                                s += 1;
                            }
                            s
                        };
                        let has_content_after = seg_start_skipped < chars.len();
                        if has_content_after {
                            let seg_w_full =
                                measure_segment_from(&chars, &cluster_len, i + 1, &char_width);
                            let cell_right_run_rel = style.text_start_offset
                                + style.available_width
                                - style.line_x_offset;
                            x = (cell_right_run_rel - seg_w_full).max(x);
                        } else {
                            let seg_w = measure_segment_from(
                                &chars,
                                &cluster_len,
                                seg_start_skipped,
                                &char_width,
                            );
                            x = (body_right_legacy - seg_w).max(x);
                        }
                    }
                    (2, _) => {
                        // RIGHT 인라인 탭 (no leader): 한컴 metrics 차이 흡수.
                        let seg_start = {
                            let mut s = i + 1;
                            while s < chars.len() && chars[s] == ' ' && cluster_len[s] != 0 {
                                s += 1;
                            }
                            s
                        };
                        let seg_w =
                            measure_segment_from(&chars, &cluster_len, seg_start, &char_width);
                        x = (body_right_legacy - seg_w).max(x);
                    }
                    _ => {
                        x = tab_target.max(x);
                    }
                }
            }
            x
        };
        compute_char_positions_walk(text, style, &char_px_raw, &inline_tab_x)
    }
}

// ── WASM 전용 내부 코드 ─────────────────────────────────────────────
//
// JS Canvas measureText 브릿지, LRU 캐시, HWP 단위 양자화 등
// WASM 빌드에서만 컴파일된다.

#[cfg(target_arch = "wasm32")]
mod wasm_internals {
    use crate::renderer::TextStyle;

    /// 1000pt 측정용 CSS font 문자열 생성
    pub(super) fn build_1000pt_font_string(style: &TextStyle) -> String {
        let font_weight = style
            .css_font_weight()
            .map(|weight| format!("{} ", weight))
            .unwrap_or_default();
        let font_style = if style.italic { "italic " } else { "" };
        let font_family = if style.font_family.is_empty() {
            "sans-serif".to_string()
        } else {
            let fallback = crate::renderer::generic_fallback(&style.font_family);
            format!("\"{}\", {}", style.font_family, fallback)
        };
        format!("{}{}1000px {}", font_style, font_weight, font_family)
    }

    /// 한컴 webhwp 방식 문자 폭 측정 (HWP 단위 양자화)
    ///
    /// 파이프라인: 내장 메트릭 → JS 1000px 측정 → font_size/1000 스케일링 → HWP 단위(×75) → 정수 반올림 → px
    pub(super) fn measure_char_width_hwp(
        measure_font: &str,
        font_family: &str,
        bold: bool,
        italic: bool,
        c: char,
        hangul_width_hwp: i32,
        font_size: f64,
    ) -> f64 {
        // 1차: 내장 메트릭 (JS 브릿지 호출 불필요)
        if let Some(w) = super::measure_char_width_embedded(font_family, bold, italic, c, font_size)
        {
            return w;
        }

        // 2차: 한글 음절 → '가' 대리 측정값 재사용 (이미 HWP 단위)
        if c >= '\u{AC00}' && c <= '\u{D7A3}' {
            return hangul_width_hwp as f64 / 75.0;
        }

        // 좁은 구두점 폴백 — native EmbeddedTextMeasurer 와 동기화.
        // measure_char_width_embedded 의 is_narrow_punctuation 분기 (0.3 em) 가
        // 적용되지 못한 미등록 폰트 케이스 (예: 휴먼명조 U+2027) 에서 JS Canvas
        // 측정값 (~0.5 em) 이 그대로 들어가지 않도록 동일 폴백 적용.
        if super::is_narrow_punctuation(c) || super::is_narrow_paren_for_font(font_family, c) {
            return font_size * 0.3;
        }

        // [Task #977] 미등록 폰트 폴백을 native EmbeddedTextMeasurer 와 동기화한다.
        // 종전(PR #1026 이전)은 JS Canvas `measureText` 실측값을 사용했으나, 미등록
        // 폰트는 브라우저 fallback 폰트로 측정되어 폰트별로 폭이 달라(예: 나눔바른
        // 고딕 ≠ 맑은 고딕) 목차 페이지의 선두 공백 CharShape 가 인접 문단과 다를 때
        // 개요번호 시작 x 가 ~9~10px 어긋났다. native compute_char_positions 와 동일한
        // ⚠ 컬러 이모지는 JS 브릿지로 재면 **안 된다**. 브릿지는 1000px 로 재서 배율을
        //   얻는데, Chrome 은 큰 크기에서 컬러 이모지 글꼴을 쓰지 못하고 .notdef(1em)을
        //   돌려준다. 실측(2026-08-01, 함초롬바탕 😀):
        //     13.3px → 1.278em (진짜)   50·100·200·400·1000px → 1.0em (거짓)
        //   그래서 아래 fallback_char_width 의 상수(EMOJI_ADVANCE_EM)로 간다.
        // 휴리스틱(공백·일반 0.5em, CJK·fullwidth em, narrow_punct 0.3em)으로 폰트 무관
        // 통일한다. PR #1026 의 narrow_punct 분기는 위에서 이미 처리(보존).
        // ⚠ 여기서도 같은 사다리를 쓴다 — 브라우저가 실제로 타는 경로가 이쪽이라,
        //   native 만 고치면 화면은 그대로다(이모지 겹침 실사고 2026-07-31).
        super::fallback_char_width(font_family, c, false, font_size)
    }

    /// 한글 '가' 대리 측정값 (HWP 단위, 정수)
    /// 내장 메트릭이 있으면 JS 호출 없이 반환.
    ///
    /// [Task #977 v3] 미등록 폰트의 한글 폭은 native `EmbeddedTextMeasurer`
    /// 폴백(`font_size`, 1.0 em CJK 휴리스틱)과 동기화한다. 종전 JS `cached_js_measure('가')`
    /// 폴백은 브라우저의 폰트 대체 결과(폰트별 ≠ 한컴 metrics)를 폭으로 채택해
    /// 한컴 저장값(tab_extended[0] = "tab_pos - 한컴_선행텍스트폭")과 합산 시 오차가
    /// 누적, 목차 페이지번호의 디지트 x 좌표가 행별로 어긋났다.
    /// 미등록 한글 폰트(나눔바른고딕 등)에서도 native 와 일관된 폭으로 폴백한다.
    pub(super) fn measure_hangul_width_hwp(
        _measure_font: &str,
        font_family: &str,
        bold: bool,
        italic: bool,
        font_size: f64,
    ) -> i32 {
        if let Some(w) =
            super::measure_char_width_embedded(font_family, bold, italic, '\u{AC00}', font_size)
        {
            return (w * 75.0).round() as i32;
        }
        // native EmbeddedTextMeasurer 동기화: 미등록 폰트의 한글(CJK)은 font_size (1.0 em).
        (font_size * 75.0).round() as i32
    }
}

// ── WasmTextMeasurer ────────────────────────────────────────────────

/// JS Canvas 브릿지 기반 텍스트 측정기 (WASM 전용)
///
/// 1000pt 측정 + HWP 단위 양자화로 한컴과 동일한 정밀도를 확보한다.
/// 내장 메트릭 우선, 미등록 폰트만 JS 브릿지 사용 (LRU 캐시 256 엔트리).
#[cfg(target_arch = "wasm32")]
pub struct WasmTextMeasurer;

#[cfg(target_arch = "wasm32")]
impl TextMeasurer for WasmTextMeasurer {
    fn estimate_text_width(&self, text: &str, style: &TextStyle) -> f64 {
        let (font_size, ratio, tab_w) = style_params(style);
        let measure_font = wasm_internals::build_1000pt_font_string(style);
        let hangul_hwp = wasm_internals::measure_hangul_width_hwp(
            &measure_font,
            &style.font_family,
            style.bold,
            style.italic,
            font_size,
        );

        let chars: Vec<char> = text.chars().collect();
        let cluster_len = build_cluster_len(&chars);
        let char_count = chars.len();
        let has_custom_tabs = !style.tab_stops.is_empty() || style.auto_tab_right;

        let char_width = |i: usize| -> f64 {
            let c = chars[i];
            if c == '\u{2007}' {
                return font_size * 0.5 * ratio + style.letter_spacing + style.extra_char_spacing;
            }
            // 인라인 객체 placeholder 는 실제 control node 가 따로 그리므로 텍스트 폭은 0.
            if c == '\u{FFFC}' {
                return 0.0;
            }
            // [Issue #677] HWP PUA 채움 문자 (U+F081C) — 시각 폭 0
            // 한컴이 인라인 TAC 표/도형 앞에 삽입하는 placeholder 채움 문자.
            // 한컴 PDF 정합 — 폭 0 으로 라인 inline x 에 영향 없음. fillers 가
            // 표 너비만큼 (≈97 chars × 1 char width = table width) 채워져
            // 표가 fillers 영역 위에 시각적으로 겹쳐 column-left 출력 패턴.
            if c == '\u{F081C}' {
                return 0.0;
            }
            let char_px_raw = if cluster_len[i] > 1 {
                hangul_hwp as f64 / 75.0
            } else {
                wasm_internals::measure_char_width_hwp(
                    &measure_font,
                    &style.font_family,
                    style.bold,
                    style.italic,
                    c,
                    hangul_hwp,
                    font_size,
                )
            };
            // Task #352: dash leader 좁은 base 0.3 em + extra_dash_advance.
            let is_leader = is_dash_leader_run(&chars, i);
            let char_px = if is_leader {
                char_px_raw.min(font_size * 0.3)
            } else {
                char_px_raw
            };
            let mut w = char_px * ratio + style.letter_spacing + style.extra_char_spacing;
            if c == ' ' {
                w += style.extra_word_spacing;
            }
            if is_leader {
                w += style.extra_dash_advance;
            }
            // 음수 자간(letter_spacing + extra_char_spacing < 0) 시
            // per-char 최소 advance 클램프로 narrow glyph 역진 방지.
            if style.letter_spacing + style.extra_char_spacing < 0.0 {
                let min_w = char_px * ratio * 0.5;
                w = w.max(min_w);
            }
            w
        };

        let mut total = 0.0;
        let mut tab_char_idx = 0usize; // [Task #296] inline_tabs 인덱스
        for i in 0..char_count {
            let c = chars[i];
            if cluster_len[i] == 0 {
                continue;
            }
            if c == '\t' {
                // [Task #296] 인라인 탭 (HWP tab_extended / HWPX 인라인 탭) 을
                // WASM Canvas 경로에서도 존중. 네이티브 EmbeddedTextMeasurer 와 동일 구조.
                if tab_char_idx < style.inline_tabs.len() {
                    let ext = &style.inline_tabs[tab_char_idx];
                    let tab_width_px = ext[0] as f64 * 96.0 / 7200.0;
                    let tab_type = inline_tab_type(ext);
                    let tab_target = total + tab_width_px;
                    // [Task #874] auto_tab_right paragraph + 단일 tab: native 와 동일.
                    let has_more_tabs_after = chars[i + 1..].iter().any(|c| *c == '\t');
                    // [Issue #900] Task #874 #10 와 동일 — ext[2] high-byte 가 명시적
                    // LEFT(1)/DECIMAL(4) 면 auto_tab_right paragraph 라도 override 금지.
                    // exam_math.hwp pi=0 ("1.\t의 값은? [2점]") 의 inline LEFT tab 이
                    // WASM 에서 right-align 되어 equation/text 가 column 우측으로 밀리는
                    // 회귀 차단. EmbeddedTextMeasurer (native) 는 이미 가드 적용.
                    let inline_is_explicit_left = tab_type == 1 || tab_type == 4;
                    let override_to_right = style.auto_tab_right
                        && !has_more_tabs_after
                        && style.available_width > 0.0
                        && !inline_is_explicit_left;
                    if override_to_right {
                        // [Task #874 #2] lang split 후속 run 합산 override (native 와 동일).
                        let seg_w = style.right_tab_block_width_override.unwrap_or_else(|| {
                            measure_segment_from(&chars, &cluster_len, i + 1, &char_width)
                        });
                        let right_edge_rel =
                            style.text_start_offset + style.available_width - style.line_x_offset;
                        total = (right_edge_rel - seg_w).max(total);
                    } else if tab_type == 0
                        && !has_more_tabs_after
                        && tab_suffix_is_ascii_page_number(&chars, i + 1)
                    {
                        if let Some(target_rel) = right_leader_tab_target_rel(style, font_size) {
                            let seg_w =
                                measure_segment_from(&chars, &cluster_len, i + 1, &char_width);
                            total = (target_rel - seg_w).max(total);
                        } else {
                            total = tab_target.max(total);
                        }
                    } else {
                        match tab_type {
                            2 => {
                                // RIGHT
                                let seg_w =
                                    measure_segment_from(&chars, &cluster_len, i + 1, &char_width);
                                total = (tab_target - seg_w).max(total);
                            }
                            3 => {
                                // CENTER
                                let seg_w =
                                    measure_segment_from(&chars, &cluster_len, i + 1, &char_width);
                                total = (tab_target - seg_w / 2.0).max(total);
                            }
                            _ => {
                                // LEFT(0/1), DECIMAL(4), 기타
                                total = tab_target.max(total);
                            }
                        }
                    }
                    tab_char_idx += 1;
                } else if has_custom_tabs {
                    let has_more_tabs_after = chars[i + 1..].iter().any(|c| *c == '\t');
                    if !has_more_tabs_after && tab_suffix_is_ascii_page_number(&chars, i + 1) {
                        if let Some(target_rel) = right_leader_body_target_rel(style) {
                            let seg_w =
                                measure_segment_from(&chars, &cluster_len, i + 1, &char_width);
                            total = (target_rel - seg_w).max(total);
                            tab_char_idx += 1;
                            continue;
                        }
                    }
                    let abs_x = style.line_x_offset + total;
                    let (tab_pos, tab_type, fill_type) = find_next_tab_stop(
                        abs_x,
                        &style.tab_stops,
                        tab_w,
                        style.auto_tab_right,
                        style.available_width,
                    );
                    let rel_tab = tab_pos - style.line_x_offset;
                    // [Task #874] auto_tab_right / leader RIGHT 탭은 col-relative 우측 끝
                    // (= text_start_offset + available_width) 까지 정렬.
                    let effective_rel_tab = if tab_type == 1
                        && style.available_width > 0.0
                        && (fill_type != 0 || style.auto_tab_right)
                    {
                        style.text_start_offset + style.available_width - style.line_x_offset
                    } else {
                        rel_tab
                    };
                    match tab_type {
                        1 => {
                            let seg_w =
                                measure_segment_from(&chars, &cluster_len, i + 1, &char_width);
                            total = (effective_rel_tab - seg_w).max(total);
                        }
                        2 => {
                            let seg_w =
                                measure_segment_from(&chars, &cluster_len, i + 1, &char_width);
                            total = (rel_tab - seg_w / 2.0).max(total);
                        }
                        _ => {
                            total = rel_tab.max(total);
                        }
                    }
                    tab_char_idx += 1;
                } else {
                    // 기본 등간격 탭: 라인 절대 위치(line_x_offset + total) 기준으로 계산
                    let abs_x = style.line_x_offset + total;
                    let next_abs = ((abs_x / tab_w).floor() + 1.0) * tab_w;
                    total = (next_abs - style.line_x_offset).max(total);
                    tab_char_idx += 1;
                }
                continue;
            }
            total += char_width(i);
        }
        total
    }

    fn compute_char_positions(&self, text: &str, style: &TextStyle) -> Vec<f64> {
        let (font_size, _ratio, _tab_w) = style_params(style);
        let measure_font = wasm_internals::build_1000pt_font_string(style);
        let hangul_hwp = wasm_internals::measure_hangul_width_hwp(
            &measure_font,
            &style.font_family,
            style.bold,
            style.italic,
            font_size,
        );
        // [#2132] 폭 산출원 훅 — wasm canvas 측정.
        let char_px_raw = |i: usize, c: char, _chars: &[char], cluster_len: &[u8]| -> f64 {
            if cluster_len[i] > 1 {
                hangul_hwp as f64 / 75.0
            } else {
                wasm_internals::measure_char_width_hwp(
                    &measure_font,
                    &style.font_family,
                    style.bold,
                    style.italic,
                    c,
                    hangul_hwp,
                    font_size,
                )
            }
        };
        // [#2132] 인라인 탭 divergent 경로 훅 — inline_tab_type 헬퍼 해석 (Task #296).
        let inline_tab_x = |i: usize,
                            x_in: f64,
                            ext: &[u16; 7],
                            chars: &[char],
                            cluster_len: &[u8],
                            char_width: &dyn Fn(usize) -> f64|
         -> f64 {
            let mut x = x_in;
            let tab_width_px = ext[0] as f64 * 96.0 / 7200.0;
            let tab_type = inline_tab_type(ext);
            let fill_low = (ext[2] & 0xFF) as u8;
            let tab_target = x + tab_width_px;
            // [Task #874] auto_tab_right paragraph + 단일 tab: native 와 동일.
            let has_more_tabs_after = chars[i + 1..].iter().any(|c| *c == '\t');
            // [Issue #900] Task #874 #10 와 동일 가드 — 인라인 LEFT(1)/DECIMAL(4)
            // 탭은 auto_tab_right 라도 right-align 금지. estimate_text_width 와
            // 동일 처리 — pi=0 의 tab 위치 정합 (equation/text 가 column 우측으로
            // 밀리는 회귀 차단).
            let inline_is_explicit_left = tab_type == 1 || tab_type == 4;
            let override_to_right = style.auto_tab_right
                && !has_more_tabs_after
                && style.available_width > 0.0
                && !inline_is_explicit_left;
            // [Issue #630 Stage 6] RIGHT + leader (fill ≠ 0): ')' 끝이 본문
            // 우측 끝까지 정렬.
            let body_right_text_rel = if style.available_width > 0.0 {
                style.text_start_offset + style.available_width - style.line_x_offset
            } else {
                f64::INFINITY
            };
            let body_right_legacy = if style.available_width > 0.0 {
                style.available_width - style.line_x_offset
            } else {
                f64::INFINITY
            };
            if override_to_right {
                // [Task #874 #2] lang split 후속 run 합산 override (native 와 동일).
                let seg_w = if let Some(w) = style.right_tab_block_width_override {
                    w
                } else {
                    let seg_start = {
                        let mut s = i + 1;
                        while s < chars.len() && chars[s] == ' ' && cluster_len[s] != 0 {
                            s += 1;
                        }
                        s
                    };
                    measure_segment_from(&chars, &cluster_len, seg_start, &char_width)
                };
                x = (body_right_text_rel - seg_w).max(x);
            } else if tab_type == 0
                && !has_more_tabs_after
                && tab_suffix_is_ascii_page_number(&chars, i + 1)
            {
                if let Some(target_rel) = right_leader_tab_target_rel(style, font_size) {
                    let seg_w = measure_segment_from(&chars, &cluster_len, i + 1, &char_width);
                    x = (target_rel - seg_w).max(x);
                } else {
                    x = tab_target.max(x);
                }
            } else {
                match tab_type {
                    2 if fill_low != 0 => {
                        // [Task #874 후속] 단일-run RIGHT + leader (목차 페이지번호).
                        // EmbeddedTextMeasurer 영역 정합 (text_measurement.rs 위쪽 동일
                        // 분기 본문 참조). \t 뒤 content 가 있는 단일-run 은
                        // cell_right_run_rel (= text_start_offset + available_width -
                        // line_x_offset) 정렬 + seg_w_full (leading space 포함).
                        // content 없는 trailing space / 끝 케이스는 원본 path 유지.
                        let seg_start_skipped = {
                            let mut s = i + 1;
                            while s < chars.len() && chars[s] == ' ' && cluster_len[s] != 0 {
                                s += 1;
                            }
                            s
                        };
                        let has_content_after = seg_start_skipped < chars.len();
                        if has_content_after {
                            let seg_w_full =
                                measure_segment_from(&chars, &cluster_len, i + 1, &char_width);
                            let cell_right_run_rel = style.text_start_offset
                                + style.available_width
                                - style.line_x_offset;
                            x = (cell_right_run_rel - seg_w_full).max(x);
                        } else {
                            let seg_w = measure_segment_from(
                                &chars,
                                &cluster_len,
                                seg_start_skipped,
                                &char_width,
                            );
                            x = (body_right_legacy - seg_w).max(x);
                        }
                    }
                    2 => {
                        // RIGHT (no leader)
                        let seg_start = {
                            let mut s = i + 1;
                            while s < chars.len() && chars[s] == ' ' && cluster_len[s] != 0 {
                                s += 1;
                            }
                            s
                        };
                        let seg_w =
                            measure_segment_from(&chars, &cluster_len, seg_start, &char_width);
                        x = (tab_target - seg_w).max(x);
                    }
                    3 => {
                        // CENTER
                        let seg_w = measure_segment_from(&chars, &cluster_len, i + 1, &char_width);
                        x = (tab_target - seg_w / 2.0).max(x);
                    }
                    _ => {
                        // LEFT(0/1), DECIMAL(4), 기타
                        x = tab_target.max(x);
                    }
                }
            }
            x
        };
        compute_char_positions_walk(text, style, &char_px_raw, &inline_tab_x)
    }
}

// ── 플랫폼별 기본 측정기 선택 ───────────────────────────────────────

#[cfg(target_arch = "wasm32")]
fn default_measurer() -> WasmTextMeasurer {
    WasmTextMeasurer
}

#[cfg(not(target_arch = "wasm32"))]
fn default_measurer() -> EmbeddedTextMeasurer {
    EmbeddedTextMeasurer
}

// ── 스타일 변환 ─────────────────────────────────────────────────────

pub(crate) fn resolved_to_text_style(
    styles: &ResolvedStyleSet,
    char_style_id: u32,
    lang_index: usize,
) -> TextStyle {
    if let Some(cs) = styles.char_styles.get(char_style_id as usize) {
        TextStyle {
            font_family: cs.font_family_for_lang(lang_index).to_string(),
            font_size: cs.font_size,
            color: cs.text_color,
            bold: cs.bold,
            italic: cs.italic,
            underline: cs.underline,
            strikethrough: cs.strikethrough,
            letter_spacing: cs.letter_spacing_for_lang(lang_index),
            ratio: cs.ratio_for_lang(lang_index),
            default_tab_width: 0.0,
            tab_stops: Vec::new(),
            auto_tab_right: false,
            available_width: 0.0,
            line_x_offset: 0.0,
            text_start_offset: 0.0,
            right_tab_block_width_override: None,
            tab_leaders: Vec::new(),
            inline_tabs: Vec::new(),
            extra_word_spacing: 0.0,
            extra_char_spacing: 0.0,
            extra_dash_advance: 0.0,
            outline_type: cs.outline_type,
            shadow_type: cs.shadow_type,
            shadow_color: cs.shadow_color,
            shadow_offset_x: cs.font_size * cs.shadow_offset_x as f64 / 100.0,
            shadow_offset_y: cs.font_size * cs.shadow_offset_y as f64 / 100.0,
            emboss: cs.emboss,
            engrave: cs.engrave,
            superscript: cs.superscript,
            subscript: cs.subscript,
            emphasis_dot: cs.emphasis_dot,
            underline_shape: cs.underline_shape,
            strike_shape: cs.strike_shape,
            underline_color: cs.underline_color,
            strike_color: cs.strike_color,
            shade_color: cs.shade_color,
        }
    } else {
        TextStyle::default()
    }
}

// ── 내장 폰트 메트릭 측정 ───────────────────────────────────────────

/// 폰트가 고정폭(monospace)인지 판정한다.
///
/// Basic Latin (U+0021~U+007E) 의 0 이 아닌 글자폭이 모두 동일하면 monospace.
/// 돋움체/바탕체/굴림체 등 한컴 고정폭 폰트는 `·` 를 포함한 모든 글리프가
/// em_size 폭을 가지므로, U+00B7 의 `.notdef` 위장값 가드에서 이들을 제외해
/// 전각 측정을 보존하기 위함이다 (Issue #630, aift 목차 right-tab 정합).
fn is_monospace_metric(metric: &font_metrics_data::FontMetric) -> bool {
    let mut common: Option<u16> = None;
    let mut count = 0u32;
    for range in metric.latin_ranges {
        if range.start > 0x007E || range.end < 0x0021 {
            continue;
        }
        for (i, &w) in range.widths.iter().enumerate() {
            let code = range.start + i as u32;
            if !(0x0021..=0x007E).contains(&code) || w == 0 {
                continue;
            }
            count += 1;
            match common {
                None => common = Some(w),
                Some(cw) if cw != w => return false,
                _ => {}
            }
        }
    }
    // 표본이 충분할 때만 monospace 로 판정 (Latin 글리프가 거의 없는 폰트 오판 방지).
    count >= 16
}

/// 내장 폰트 메트릭으로 문자 폭 측정 (em 단위 → px 변환)
///
/// 내장 메트릭이 있으면 JS 브릿지 호출 없이 즉시 반환.
/// 없으면 None을 반환하여 폴백 경로를 사용하게 한다.
fn quantize_hwp_px(px: f64) -> f64 {
    let hwp = (px * 75.0) as i32;
    hwp as f64 / 75.0
}

fn kopub_char_width(primary_name: &str, c: char, font_size: f64) -> Option<f64> {
    let lower = primary_name.to_lowercase();
    let is_dotum = primary_name.contains("KoPub돋움체") || lower.contains("kopub dotum");
    let is_batang = primary_name.contains("KoPub바탕체") || lower.contains("kopub batang");
    if !is_dotum && !is_batang {
        return None;
    }

    if c == ' ' {
        return Some(quantize_hwp_px(font_size * 0.5));
    }
    if is_narrow_punctuation(c) {
        return Some(quantize_hwp_px(font_size * 0.3));
    }
    // [#2239] 괄호 — KoPub 경로는 86712 한컴 PDF 글리프 직독 실측(13px 문서
    // 괄호 4px ≈ 0.3em, #2195 stage23)으로 narrow 유지. is_narrow_punctuation
    // 의 괄호가 폰트 한정(is_narrow_paren_for_font)으로 빠지면서 여기서 보존.
    if matches!(c, '(' | ')') {
        return Some(quantize_hwp_px(font_size * 0.3));
    }
    if c.is_ascii() {
        return Some(quantize_hwp_px(font_size * 0.5));
    }
    if is_cjk_char(c) || is_fullwidth_symbol(c) {
        // [#2195 stage57] KoPub 미설치 환경에서 한글은 바탕으로 치환해 **전각
        // 1.0em** 렌더 — 86712 한컴 PDF 글리프 직독(Haansoft Batang, 12pt 한글
        // 16px) 실측. 종전 0.84 는 r27 근거설명 25문단을 -11줄 과소(래핑 조기
        // 종료)시키던 성분.
        let factor = if is_dotum { 1.0 } else { 0.94 };
        return Some(quantize_hwp_px(font_size * factor));
    }

    None
}

/// [#2156] Haansoft Batang(한컴바탕, HBATANG.TTF upm=1024) ASCII advance/em.
/// 한글은 함초롬바탕(HCR Batang) 문서의 비한글 문자(라틴·숫자·구두점·U+00B7)를
/// HCR hmtx 가 아닌 이 메트릭으로 렌더한다 — 문자폭 사다리 통제 프로브로
/// 전 판별 클래스 확정 (괄호 0.32→0.50em 등, mydocs/working/task_m100_2156_*).
/// 공백(0x20)은 useFontSpace=0 고정 0.5em 경로(기존 em/2) 유지를 위해 제외.
const HAANSOFT_BATANG_ASCII: [f64; 95] = [
    0.3330, 0.4160, 0.4160, 0.8330, 0.6250, 0.9160, 0.8330, 0.2500, // ` !"#$%&'`
    0.5000, 0.5000, 0.5000, 0.8330, 0.2910, 0.8330, 0.2910, 0.3330, // `()*+,-./`
    0.5830, 0.5830, 0.5830, 0.5830, 0.5830, 0.5830, 0.5830, 0.5830, // `01234567`
    0.5830, 0.5830, 0.3330, 0.3330, 0.8330, 0.8330, 0.8330, 0.5000, // `89:;<=>?`
    1.0000, 0.7500, 0.6660, 0.6660, 0.7080, 0.6660, 0.6250, 0.7080, // `@ABCDEFG`
    0.7500, 0.3750, 0.4580, 0.7500, 0.6250, 0.9160, 0.7500, 0.7080, // `HIJKLMNO`
    0.6250, 0.7080, 0.6660, 0.6250, 0.7500, 0.7500, 0.7080, 0.9580, // `PQRSTUVW`
    0.6660, 0.6660, 0.6250, 0.5000, 0.3330, 0.5000, 1.0000, 0.5000, // `XYZ[\]^_`
    0.5830, 0.5000, 0.5410, 0.5000, 0.5410, 0.5410, 0.3750, 0.5410, // '`abcdefg'
    0.5410, 0.2910, 0.2910, 0.5410, 0.2910, 0.8330, 0.5410, 0.5410, // `hijklmno`
    0.5410, 0.5410, 0.4160, 0.5000, 0.3750, 0.5410, 0.5410, 0.7910, // `pqrstuvw`
    0.5830, 0.5830, 0.4580, 0.5830, 0.5830, 0.5830, 0.7910, // `xyz{|}~`
];

/// [#2156] 검증된 함초롬바탕 별칭의 비한글 문자 폭 오버라이드 (advance/em 비율).
fn haansoft_latin_override(primary_name: &str, c: char) -> Option<f64> {
    // 함초롬돋움/HCR Dotum은 Haansoft Dotum 등 별도 대체 가능성이 남아 있다.
    // 바탕 문자폭 사다리로 검증된 정확한 별칭만 이 테이블을 사용한다.
    if !matches!(primary_name, "함초롬바탕" | "HCR Batang") {
        return None;
    }
    if c == '\u{00B7}' {
        return Some(0.3330);
    }
    let cp = c as u32;
    if (0x21..0x7F).contains(&cp) {
        return Some(HAANSOFT_BATANG_ASCII[(cp - 0x20) as usize]);
    }
    None
}

/// [#2070] ㆍ(U+318D) 폭은 SYMBOL 폰트별: 한양신명조 = 전각(사다리 v3 실측),
/// 명조(HY견명조 치환) 등 여타 = 반각 (80168 개정안{{7}} p9/p13 '시ㆍ도조례'
/// 1줄 오라클, 개정안{{1}} P21 마크와 반각 양립 검증). embedded 메트릭
/// (HY견명조 수록분)이 전각이라 룩업보다 앞서 판정하되, 함초롬(HCR) 계열은
/// embedded 메트릭을 신뢰한다 (None 반환).
pub(crate) fn area_dot_fallback_width(font_family: &str, font_size: f64) -> Option<f64> {
    let fam = font_family.split(',').next().unwrap_or("").trim();
    if fam.contains("함초롬") || fam.contains("HCR") {
        return None;
    }
    Some(if fam.contains("한양신명조") {
        font_size
    } else {
        font_size * 0.5
    })
}

fn measure_char_width_embedded(
    font_family: &str,
    bold: bool,
    italic: bool,
    c: char,
    font_size: f64,
) -> Option<f64> {
    // ⚠ 컬러 이모지는 **문서 글꼴이 그리지 않는다** — 시스템 이모지 글꼴이 그린다.
    //   그런데 내장 메트릭이 .notdef 폭(1em)을 돌려주는 바람에 여기서 답이 정해져
    //   폴백 사다리(1.28em)까지 가지도 못했다(2026-08-01 실측: 😀 13.3px, 실제 17px).
    //   여기서 None 을 내면 모든 호출부(폭·캐럿·wasm·native)가 한꺼번에 고쳐진다.
    if is_emoji_presentation(c) {
        return None;
    }
    // CSS font-family 체인에서 첫 번째 폰트명으로 메트릭 조회
    let primary_name = font_family.split(',').next().unwrap_or(font_family).trim();
    // [#2156] 함초롬바탕 비한글 문자 — Haansoft Batang 메트릭 대체 (한글 동작).
    if let Some(r) = haansoft_latin_override(primary_name, c) {
        return Some(quantize_hwp_px(r * font_size));
    }
    if let Some(w) = kopub_char_width(primary_name, c, font_size) {
        return Some(w);
    }
    let mm = font_metrics_data::find_metric(primary_name, bold, italic)?;
    // HWP 반각 처리: space 및 한컴이 반각으로 처리하는 구두점/기호
    let w = if c == ' ' {
        mm.metric.em_size / 2
    } else {
        let glyph_w = mm.metric.get_width(c)?;
        // 한컴은 스마트 따옴표 등을 반각으로 처리.
        // 폰트 메트릭에서 전각(em_size)으로 기록되어 있어도 em/2로 강제.
        // [Issue #630] U+00B7 (가운뎃점) 은 본 분기에서 제외 — 한컴 저장본의
        // tab_extended 가 전각 측정 기반으로 산출되므로 반각 강제 시 right-tab
        // 정렬이 8.67px 좌측 이탈. 폰트 메트릭 그대로 사용 (전각).
        let is_halfwidth_punct = matches!(
            c,
            '\u{2018}'..='\u{2027}' // ''‚‛""„‟†‡•‣․‥…‧ 구두점/기호
        );
        // 휴먼명조/HY중고딕/HY신명조/HY견명조 등 일부 폰트 DB 가 U+2018/U+2019/
        // U+2027 을 fullwidth (1.0 em) 로 잘못 기록한 케이스 정정. em/2 (0.5 em)
        // 강제 시 한컴 대비 약 4px (font-size 20px 기준, 0.5→0.3 em 차) 과대.
        // glyph_w 가 비정상 fullwidth (>= em_size) 일 때만 0.3 em 강제 — 함초롬
        // 바탕 (0.32) / Pretendard (0.22) 등 정상 DB 값은 조건 미충족으로 영향 없음.
        let is_narrow_unicode_punct = matches!(c, '\u{2018}' | '\u{2019}' | '\u{2027}');
        // [U+00B7 .notdef 위장값 정정] 비례폰트(휴먼명조 등)가 `·` (가운뎃점)
        // 글리프를 갖지 않으면 cmap 이 .notdef(glyph 0) 로 매핑돼 advance 가
        // em_size(전각) 로 기록된다. 한컴은 이 경우 점 글리프를 가진 대체
        // 폰트(바탕 ≈0.33em 등)로 `·` 를 렌더하므로 전각 advance 는 PDF 대비
        // 과대 (시·군 점 좌우 공백 큼). 비례폰트에서 U+00B7 이 전각이면 위장값
        // 으로 보고 0.3em 으로 정정한다. 고정폭(monospace) 폰트(돋움체 등)는
        // 모든 글리프가 em_size 이므로 제외 — 해당 `·` 는 진짜 전각이다
        // (Issue #630, aift 목차 right-tab 정합 보존).
        let is_b7_notdef_artifact =
            c == '\u{00B7}' && glyph_w >= mm.metric.em_size && !is_monospace_metric(mm.metric);
        if (is_narrow_unicode_punct && glyph_w >= mm.metric.em_size) || is_b7_notdef_artifact {
            (mm.metric.em_size as f64 * 0.3) as u16
        } else if (is_halfwidth_punct || is_halfwidth_cjk_quote(c)) && glyph_w >= mm.metric.em_size
        {
            mm.metric.em_size / 2
        } else {
            glyph_w
        }
    };
    // em 단위 → px: w / em_size * font_size, 그 후 HWP 양자화
    let em = mm.metric.em_size as f64;
    let mut actual_px = w as f64 * font_size / em;

    // Bold 폴백: Regular 메트릭으로 폴백된 경우
    // 한컴은 faux bold(합성 Bold) 시 렌더링만 획을 두껍게 하고,
    // 텍스트 메트릭(폭 계산)에는 Regular 폭을 그대로 사용한다.
    // bold_fallback 보정을 적용하면 Justify 정렬에서 공백이 축소됨.
    // (26글자 × 1.02px/글자 = 26.5px 과대 → 공백 소멸)

    // 한컴과 동일한 HWPUNIT 정수 변환: w * base_size / em (내림)
    // round가 아닌 truncate (as i32)로 처리하여 한컴 정수 나눗셈과 일치
    Some(quantize_hwp_px(actual_px))
}

// ── 호환 래퍼 (기존 호출부 변경 없음) ──────────────────────────────

/// 텍스트 폭 추정
///
/// 플랫폼별 기본 TextMeasurer를 자동 선택하여 위임한다.
/// WASM: WasmTextMeasurer (JS Canvas + HWP 양자화)
/// 네이티브: EmbeddedTextMeasurer (내장 메트릭 + 휴리스틱)
pub(crate) fn estimate_text_width(text: &str, style: &TextStyle) -> f64 {
    default_measurer().estimate_text_width(text, style)
}

/// 텍스트 폭 추정 (round 없이 raw px 반환)
///
/// 줄바꿈 엔진 전용. 단일 문자 토큰의 반올림 누적 오차를 방지한다.
/// 한컴은 HWPUNIT 정수로 폭을 누적하므로, round 없이 px를 합산한 뒤
/// 줄바꿈 비교 시점에서 available_width와 비교하는 것이 더 정확하다.
pub(crate) fn estimate_text_width_unrounded(text: &str, style: &TextStyle) -> f64 {
    let measurer = EmbeddedTextMeasurer;
    let (font_size, ratio, tab_w) = style_params(style);
    let chars: Vec<char> = text.chars().collect();
    let cluster_len = build_cluster_len(&chars);
    let char_count = chars.len();

    let char_width = |i: usize| -> f64 {
        let c = chars[i];
        if c == '\u{2007}' {
            return font_size * 0.5 * ratio + style.letter_spacing + style.extra_char_spacing;
        }
        // 인라인 객체 placeholder 는 실제 control node 가 따로 그리므로 텍스트 폭은 0.
        if c == '\u{FFFC}' {
            return 0.0;
        }
        // [Issue #677] HWP PUA 채움 문자 (U+F081C) — 시각 폭 0
        if c == '\u{F081C}' {
            return 0.0;
        }
        // 이모지 결합 문자는 앞 글자에 붙어 그려진다 — 폭을 따로 세면 그만큼 밀린다.
        if is_emoji_zero_width(c) {
            return 0.0;
        }
        let base_w_raw = if let Some(w) = (c == '\u{318D}')
            .then(|| area_dot_fallback_width(&style.font_family, font_size))
            .flatten()
        {
            w
        } else if let Some(w) =
            measure_char_width_embedded(&style.font_family, style.bold, style.italic, c, font_size)
        {
            w
        } else {
            fallback_char_width(&style.font_family, c, cluster_len[i] > 1, font_size)
        };
        // Task #352: 3+ 연속 dash leader 좁은 base 0.3 em + 라인 슬랙 분배.
        let is_leader = is_dash_leader_run(&chars, i);
        let base_w = if is_leader {
            base_w_raw.min(font_size * 0.3)
        } else {
            base_w_raw
        };
        let mut w = base_w * ratio + style.letter_spacing + style.extra_char_spacing;
        if c == ' ' {
            w += style.extra_word_spacing;
        }
        if is_leader {
            w += style.extra_dash_advance;
        }
        // 음수 자간(letter_spacing + extra_char_spacing < 0) 시
        // per-char 최소 advance 클램프로 narrow glyph 역진 방지.
        if style.letter_spacing + style.extra_char_spacing < 0.0 {
            let min_w = base_w * ratio * 0.5;
            w = w.max(min_w);
        }
        w
    };

    let mut total = 0.0;
    for i in 0..char_count {
        if cluster_len[i] == 0 {
            continue;
        }
        let c = chars[i];
        if c == '\t' {
            let abs_x = style.line_x_offset + total;
            let next_abs = ((abs_x / tab_w).floor() + 1.0) * tab_w;
            total = (next_abs - style.line_x_offset).max(total);
            continue;
        }
        total += char_width(i);
    }
    total // round 없이 반환
}

/// 글자별 X 위치 경계값 계산
///
/// N글자 → N+1개 경계값을 반환한다 (0번째는 0.0, N번째는 전체 폭).
/// run 내부 상대 좌표이며, 절대 좌표는 run.bbox.x + charX[i]로 계산한다.
pub(crate) fn compute_char_positions(text: &str, style: &TextStyle) -> Vec<f64> {
    default_measurer().compute_char_positions(text, style)
}

// ── 문자 분류 함수 ──────────────────────────────────────────────────

/// CJK 문자 여부 판별 (EmbeddedTextMeasurer의 히우리스틱 폭 계산에서 사용)
pub(crate) fn is_cjk_char(c: char) -> bool {
    ('\u{1100}'..='\u{11FF}').contains(&c)   // 한글 자모
    || ('\u{3130}'..='\u{318F}').contains(&c) // 한글 호환 자모 (ㆍ U+318D 포함)
    || ('\u{AC00}'..='\u{D7AF}').contains(&c) // 한글 음절
    || ('\u{A960}'..='\u{A97F}').contains(&c) // 한글 자모 확장-A (옛한글 초성)
    || ('\u{D7B0}'..='\u{D7FF}').contains(&c) // 한글 자모 확장-B (옛한글 중/종성)
    || ('\u{4E00}'..='\u{9FFF}').contains(&c) // CJK Unified Ideographs
    || ('\u{3400}'..='\u{4DBF}').contains(&c) // CJK Extension A
    || ('\u{F900}'..='\u{FAFF}').contains(&c) // CJK Compatibility
    || ('\u{3040}'..='\u{30FF}').contains(&c) // 히라가나/카타카나
    || ('\u{FF00}'..='\u{FFEF}').contains(&c) // 전각 문자
}

/// 실제 글리프 폭이 반각(em/2)보다 뚜렷이 좁은 구두점·기호.
/// 메트릭 DB 미등록 폰트의 폴백 폭 계산 시 `font_size * 0.5` 대신
/// `font_size * 0.3` 을 쓰도록 분기하는 기준 (Task #257).
///
/// General Punctuation 좁은 글리프 확장: 휴먼명조 U+2027 등 DB 미수록
/// 폰트의 폴백 `font_size * 0.5` 가 한컴 대비 ~10px 과대 (font-size 20px
/// 기준). 한컴은 약 0.25-0.3 em 으로 렌더하므로 동일 분기 적용.
fn is_narrow_punctuation(c: char) -> bool {
    matches!(
        c,
        ',' | '.' | ':' | ';' | '\'' | '"' | '`' |
        '\u{00B7}' |  // · MIDDLE DOT
        '\u{2018}' |  // ' LEFT SINGLE QUOTATION MARK
        '\u{2019}' |  // ' RIGHT SINGLE QUOTATION MARK
        '\u{2027}' |  // ‧ HYPHENATION POINT
        // [Task #1735] 한글 방점. 렌더 경로에서 좁은 가운데 점(·)으로 치환되므로
        // 측정 폭도 narrow 로 맞춰 측정-렌더 폭 정합 유지(0.5em 기본 폴백 방지).
        '\u{302E}' |  // 〮 HANGUL SINGLE DOT TONE MARK (방점)
        '\u{302F}' // 〯 HANGUL DOUBLE DOT TONE MARK (쌍방점)
    )
}

/// [#2239] 괄호 '(' ')' narrow 폭(0.3em) — 사다리 실측 폰트 한정.
///
/// 통제 사다리 실측(#2195 stage30/31): 휴먼명조 '(' = 0.31em(embedded 정합),
/// 한양중고딕 '(' <= 317HU(0.29em) — fallback 0.5em 은 과대
/// (76076 표325 r0 '(정량)영향집단명' 11pt: 8800>8642 로 2줄, 한글 1줄).
/// 단 HY신명조·바탕 계열은 0.5em(#2156 ASCII 폭 표 정합)이므로 폰트 무관
/// `is_narrow_punctuation` 전역 분류는 금지 — 실측된 폰트에서만 좁힌다.
/// (KoPub 계열은 `kopub_char_width` 자체 분기에서 별도 실측 근거로 유지.)
fn is_narrow_paren_for_font(font_family: &str, c: char) -> bool {
    if !matches!(c, '(' | ')') {
        return false;
    }
    let primary = font_family.split(',').next().unwrap_or(font_family).trim();
    primary.contains("휴먼명조") || primary.contains("한양중고딕") || primary.contains("HY중고딕")
}

/// 한컴이 수평 조판에서 반각 advance 로 처리하는 CJK 낫표.
///
/// 일부 등록 폰트는 `「」` glyph advance 를 전각으로 제공하지만, 한컴 PDF 기준
/// 본문 조판에서는 법령명 낫표 뒤에 전각 공백처럼 보이는 간격이 생기지 않는다.
pub(crate) fn is_halfwidth_cjk_quote(c: char) -> bool {
    matches!(c, '\u{300C}' | '\u{300D}')
}

/// 3 개 이상 연속하는 dash leader 시퀀스의 일부 여부 (Task #352).
///
/// 한컴 문서의 빈칸/구분선은 ASCII '-' 반복으로 구성되며, PDF 도 좁은
/// advance 로 렌더된다. 그러나 일부 한글 폰트(HY신명조 등) 의 메트릭 DB 가
/// '-' 글리프 폭을 0.83 em 으로 저장하고 있어 반복 시 자연 폭이
/// 사용 가능 폭을 크게 초과한다. 본 헬퍼로 leader 시퀀스를 식별해
/// 좁은 advance(`font_size * 0.3`) 로 재산출한다.
///
/// 자연 텍스트의 단발 dash(예: "stimulus-driven", "32.-") 는 ≥3 조건을
/// 만족하지 않으므로 영향 없음.
fn is_dash_leader_run(chars: &[char], i: usize) -> bool {
    if chars[i] != '-' {
        return false;
    }
    let mut count = 1usize;
    let mut j = i;
    while j > 0 && chars[j - 1] == '-' {
        count += 1;
        j -= 1;
        if count >= 3 {
            return true;
        }
    }
    let mut j = i;
    while j + 1 < chars.len() && chars[j + 1] == '-' {
        count += 1;
        j += 1;
        if count >= 3 {
            return true;
        }
    }
    false
}

/// 한컴이 전각으로 처리하는 기호 (메트릭 폴백 시 font_size 사용)
fn is_fullwidth_symbol(c: char) -> bool {
    matches!(c,
        '\u{20A9}' |                   // ₩ WON SIGN
        '\u{20AC}' |                   // € EURO SIGN
        '\u{00A3}' |                   // £ POUND SIGN
        '\u{00A5}'                     // ¥ YEN SIGN
    )
    || ('\u{2190}'..='\u{21FF}').contains(&c) // Arrows (→, ⇨, ⇒ 등)
    || ('\u{2460}'..='\u{24FF}').contains(&c) // Enclosed Alphanumerics (①②③ 등)
    || ('\u{25A0}'..='\u{25FF}').contains(&c) // Geometric Shapes (□■▲◆○ 등, 섹션 머리 기호)
    || ('\u{2600}'..='\u{26FF}').contains(&c) // Miscellaneous Symbols (☆★ 등)
    || ('\u{2700}'..='\u{27BF}').contains(&c) // Dingbats (✓✗ 등)
    || ('\u{3200}'..='\u{32FF}').contains(&c) // Enclosed CJK Letters (㉠㉡ 등)
    || ('\u{3300}'..='\u{33FF}').contains(&c) // CJK Compatibility (㎜㎝ 등)
    || ('\u{2160}'..='\u{217F}').contains(&c) // Roman Numerals (Ⅰ Ⅱ Ⅲ 등)
}

/// 한글 자모 초성 여부 (옛한글 포함)
fn is_hangul_choseong(c: char) -> bool {
    ('\u{1100}'..='\u{115F}').contains(&c) || ('\u{A960}'..='\u{A97F}').contains(&c)
}

/// 한글 자모 중성 여부 (옛한글 포함, ᆞ U+119E 포함)
fn is_hangul_jungseong(c: char) -> bool {
    ('\u{1160}'..='\u{11A7}').contains(&c) || ('\u{D7B0}'..='\u{D7C6}').contains(&c)
}

/// 한글 자모 종성 여부 (옛한글 포함)
fn is_hangul_jongseong(c: char) -> bool {
    ('\u{11A8}'..='\u{11FF}').contains(&c) || ('\u{D7CB}'..='\u{D7FB}').contains(&c)
}

/// 텍스트를 렌더링 클러스터 단위로 분할한다.
/// 한글 자모 조합 시퀀스(초+중+종)를 하나의 클러스터로 묶어
/// 옛한글(아래아 등)이 올바르게 합성될 수 있도록 한다.
/// 반환값: Vec<(시작_문자_인덱스, 클러스터_문자열)>
pub fn split_into_clusters(text: &str) -> Vec<(usize, String)> {
    let chars: Vec<char> = text.chars().collect();
    let mut clusters: Vec<(usize, String)> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        // 초성으로 시작하는 자모 조합 시퀀스 감지
        if is_hangul_choseong(chars[i]) {
            let start = i;
            let mut cluster = String::new();
            cluster.push(chars[i]);
            i += 1;
            // 중성 (필수)
            if i < chars.len() && is_hangul_jungseong(chars[i]) {
                cluster.push(chars[i]);
                i += 1;
                // 종성 (선택)
                if i < chars.len() && is_hangul_jongseong(chars[i]) {
                    cluster.push(chars[i]);
                    i += 1;
                }
            }
            clusters.push((start, cluster));
        } else {
            clusters.push((i, chars[i].to_string()));
            i += 1;
        }
    }
    clusters
}

/// 세로쓰기에서 CW 90° 회전해야 하는 문자 판별
///
/// text_direction과 무관하게 항상 회전되는 문자:
/// - 괄호류: ( ) [ ] { } < > 〈 〉 《 》 「 」 『 』 【 】
/// - 문장부호: . , _ - ~ … ― ─
pub(crate) fn is_vertical_rotate_char(c: char) -> bool {
    matches!(
        c,
        '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>'
        | '.' | ',' | '_' | '-' | '~'
        | '\u{2026}' // … (ellipsis)
        | '\u{2015}' // ― (horizontal bar)
        | '\u{2500}' // ─ (box drawing horizontal)
        | '\u{2014}' // — (em dash)
        | '\u{2013}' // – (en dash)
        | '\u{3008}' | '\u{3009}' // 〈 〉
        | '\u{300A}' | '\u{300B}' // 《 》
        | '\u{300C}' | '\u{300D}' // 「 」
        | '\u{300E}' | '\u{300F}' // 『 』
        | '\u{3010}' | '\u{3011}' // 【 】
        | '\u{FF08}' | '\u{FF09}' // （ ）
        | '\u{FF3B}' | '\u{FF3D}' // ［ ］
        | '\u{FF5B}' | '\u{FF5D}' // ｛ ｝
    )
}

/// 세로쓰기 기호 대체: 수평 형태 → 세로 형태 Unicode 변환
///
/// CJK Compatibility Forms (U+FE30-FE4F) 및 Vertical Forms 활용.
/// 대체 가능한 문자가 있으면 Some(세로형태)를 반환하고,
/// 없으면 None을 반환한다 (호출측에서 회전 처리).
pub(crate) fn vertical_substitute_char(c: char) -> Option<char> {
    match c {
        // 괄호류
        '(' | '\u{FF08}' => Some('\u{FE35}'), // ︵
        ')' | '\u{FF09}' => Some('\u{FE36}'), // ︶
        '{' | '\u{FF5B}' => Some('\u{FE37}'), // ︷
        '}' | '\u{FF5D}' => Some('\u{FE38}'), // ︸
        '[' | '\u{FF3B}' => Some('\u{FE39}'), // ︹
        ']' | '\u{FF3D}' => Some('\u{FE3A}'), // ︺
        '\u{3010}' => Some('\u{FE3B}'),       // 【 → ︻
        '\u{3011}' => Some('\u{FE3C}'),       // 】 → ︼
        '\u{3008}' => Some('\u{FE3F}'),       // 〈 → ︿
        '\u{3009}' => Some('\u{FE40}'),       // 〉 → ﹀
        '\u{300A}' => Some('\u{FE3D}'),       // 《 → ︽
        '\u{300B}' => Some('\u{FE3E}'),       // 》 → ︾
        '\u{300C}' => Some('\u{FE41}'),       // 「 → ﹁
        '\u{300D}' => Some('\u{FE42}'),       // 」 → ﹂
        '\u{300E}' => Some('\u{FE43}'),       // 『 → ﹃
        '\u{300F}' => Some('\u{FE44}'),       // 』 → ﹄
        // 대시/선
        '\u{2014}' => Some('\u{FE31}'), // — → ︱ (em dash)
        '\u{2013}' => Some('\u{FE32}'), // – → ︲ (en dash)
        '\u{2015}' => Some('\u{FE31}'), // ― → ︱ (horizontal bar)
        '\u{2500}' => Some('\u{2502}'), // ─ → │ (box drawing)
        // 말줄임
        '\u{2026}' => Some('\u{FE19}'), // … → ︙ (vertical ellipsis)
        // 물결표
        '~' => Some('\u{FE34}'), // ~ → ︴ (vertical wavy low line)
        // 밑줄
        '_' => Some('\u{FE33}'), // _ → ︳ (vertical low line)
        _ => None,
    }
}

// ── 테스트 ──────────────────────────────────────────────────────────
