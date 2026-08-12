//! 줄 나눔 엔진 (Line Breaking Engine)
//!
//! 문단 텍스트를 토큰화하고 줄 나눔을 수행한다.
//! 한글 어절/글자, 영어 단어/하이픈, CJK 개별 분할을 지원한다.

use super::{find_active_char_shape, is_lang_neutral};
use crate::model::control::Control;
use crate::model::paragraph::{CharShapeRef, LineSeg, Paragraph};
use crate::model::style::LineSpacingType;
use crate::renderer::layout::{
    estimate_text_width, estimate_text_width_unrounded, is_cjk_char, resolved_to_text_style,
};
use crate::renderer::px_to_hwpunit;
use crate::renderer::style_resolver::{detect_lang_category, ResolvedStyleSet};

/// 줄 나눔 토큰
#[derive(Debug, Clone)]
pub(crate) enum BreakToken {
    /// 분할 불가 텍스트 조각 (어절/단어/글자)
    /// char_widths: 글자별 px 폭 (char_level_break용, 단일 글자 토큰은 비어있음)
    Text {
        start_idx: usize,
        end_idx: usize,
        width: f64,
        max_font_size: f64,
        char_widths: Vec<f64>,
    },
    /// 공백 (줄 바꿈 가능 지점, 줄 끝에서 흡수)
    Space {
        idx: usize,
        width: f64,
        max_font_size: f64,
    },
    /// 탭 (줄 바꿈 가능 지점, 폭은 줄 위치에 따라 동적)
    Tab { idx: usize, max_font_size: f64 },
    /// 강제 줄 바꿈 (\n)
    LineBreak { idx: usize },
    /// [oracle-corpus-mining-20260805 (h)] 글자처럼 취급(TAC) 개체 — **폭을 가진 한 글자**.
    /// 텍스트 문자는 0개지만 줄 채움에 자기 폭으로 참여하고, 남은 폭에 안 들어가면
    /// 개체 앞에서 줄이 끊긴다(대형 표가 자기 줄을 갖는 것은 특례가 아니라 이 규칙의
    /// 귀결). `idx` 는 개체가 **바로 앞에 놓이는** 텍스트 char 인덱스.
    Object {
        idx: usize,
        /// 바깥여백 좌우를 포함한 글리프 폭(HWPUNIT). px 왕복을 거치지 않는다 —
        /// 코퍼스 표본이 잔여 폭을 2HU 차이로 넘기므로(aift s0#0) 절삭 오차가 판정을
        /// 뒤집는다.
        width_hwp: i32,
        height_hwp: i32,
    },
}

/// 줄 채움 결과
#[derive(Debug)]
struct LineBreakResult {
    start_idx: usize,
    end_idx: usize, // exclusive
    max_font_size: f64,
    has_line_break: bool, // 강제 줄 바꿈 여부
    /// [양쪽 흐름] 이 결과가 앞 결과와 **같은 시각적 줄**의 다음 세그먼트인가.
    /// (표 좌우로 글이 갈라질 때 한 줄 = 왼쪽 세그 + 오른쪽 세그)
    continues_line: bool,
    /// 이 줄에 놓인 TAC 개체의 최대 높이(HWPUNIT, 0 = 없음).
    object_height_hwp: i32,
}

/// 줄 머리 금칙: 줄 시작에 올 수 없는 문자
pub(crate) fn is_line_start_forbidden(ch: char) -> bool {
    matches!(
        ch,
        ')' | ']'
            | '}'
            | ','
            | '.'
            | '!'
            | '?'
            | ';'
            | ':'
            | '\''
            | '"'
            | '\u{3001}'
            | '\u{3002}'
            | '\u{2026}'
            | '\u{00B7}'
            | '\u{2015}'
            | '\u{30FC}'
            | '\u{300B}'
            | '\u{300D}'
            | '\u{300F}'
            | '\u{3011}'
            | '\u{FF09}'
            | '\u{FF5D}'
            | '\u{3015}'
            | '\u{3009}'
            | '\u{FF1E}'
            | '\u{226B}'
            | '\u{FF3D}'
            | '\u{FE5E}'
            | '\u{301E}'
            | '\u{2019}'
            | '\u{201D}'
            | '\u{FF0C}'
            | '\u{FF0E}'
            | '\u{FF01}'
            | '\u{FF1F}'
            | '\u{FF1B}'
            | '\u{FF1A}'
            | '%'
            | '\u{2030}'
            | '\u{2103}'
            | '\u{00B0}'
            | '\u{FF05}'
    )
}

/// 줄 꼬리 금칙: 줄 끝에 올 수 없는 문자
pub(crate) fn is_line_end_forbidden(ch: char) -> bool {
    matches!(
        ch,
        '(' | '['
            | '{'
            | '\''
            | '"'
            | '\u{300A}'
            | '\u{300C}'
            | '\u{300E}'
            | '\u{3010}'
            | '\u{FF08}'
            | '\u{FF5B}'
            | '\u{3014}'
            | '\u{3008}'
            | '\u{FF1C}'
            | '\u{226A}'
            | '\u{FF3B}'
            | '\u{301D}'
            | '\u{2018}'
            | '\u{201C}'
            | '$'
            | '\u{20A9}'
            | '\u{00A3}'
            | '\u{20AC}'
            | '\u{00A5}'
            | '\u{FF04}'
            | '\u{FFE5}'
    )
}

/// 한글 음절/자모 여부 (옛한글 확장 자모 포함)
fn is_hangul(ch: char) -> bool {
    ('\u{AC00}'..='\u{D7A3}').contains(&ch)       // 한글 음절
        || ('\u{1100}'..='\u{11FF}').contains(&ch) // 한글 자모
        || ('\u{3130}'..='\u{318F}').contains(&ch) // 한글 호환 자모 (ㆍ U+318D 포함)
        || ('\u{A960}'..='\u{A97F}').contains(&ch) // 한글 자모 확장-A (옛한글 초성)
        || ('\u{D7B0}'..='\u{D7FF}').contains(&ch) // 한글 자모 확장-B (옛한글 중/종성)
}

/// 라틴 문자 여부 (영문+숫자)
fn is_latin(ch: char) -> bool {
    let lang = detect_lang_category(ch);
    lang == 1 // English/Latin
}

/// CJK 문자 여부 (한자/일본어 — 개별 분할 대상)
fn is_cjk_ideograph(ch: char) -> bool {
    let lang = detect_lang_category(ch);
    lang == 2 || lang == 3 // Chinese or Japanese
}

/// 문단 텍스트를 줄 나눔 토큰으로 분할한다.
pub(crate) fn tokenize_paragraph(
    text_chars: &[char],
    char_offsets: &[u32],
    char_shapes: &[CharShapeRef],
    styles: &ResolvedStyleSet,
    english_break_unit: u8,
    korean_break_unit: u8,
) -> Vec<BreakToken> {
    let text_len = text_chars.len();
    if text_len == 0 {
        return Vec::new();
    }

    let mut tokens = Vec::new();
    let mut i = 0;
    let mut current_lang: usize = 0;

    while i < text_len {
        let ch = text_chars[i];

        // 강제 줄 바꿈
        if ch == '\n' {
            tokens.push(BreakToken::LineBreak { idx: i });
            i += 1;
            continue;
        }

        // 탭
        if ch == '\t' {
            let utf16_pos = if i < char_offsets.len() {
                char_offsets[i]
            } else {
                i as u32
            };
            let style_id = find_active_char_shape(char_shapes, utf16_pos);
            let ts = resolved_to_text_style(styles, style_id, current_lang);
            let font_size = if ts.font_size > 0.0 {
                ts.font_size
            } else {
                12.0
            };
            tokens.push(BreakToken::Tab {
                idx: i,
                max_font_size: font_size,
            });
            i += 1;
            continue;
        }

        // 공백 (줄 바꿈 지점) — NonBreakingSpace(\u{00A0})는 제외
        if ch == ' ' {
            let utf16_pos = if i < char_offsets.len() {
                char_offsets[i]
            } else {
                i as u32
            };
            let style_id = find_active_char_shape(char_shapes, utf16_pos);
            let ts = resolved_to_text_style(styles, style_id, current_lang);
            let font_size = if ts.font_size > 0.0 {
                ts.font_size
            } else {
                12.0
            };
            let w = estimate_text_width_unrounded(" ", &ts);
            tokens.push(BreakToken::Space {
                idx: i,
                width: w,
                max_font_size: font_size,
            });
            i += 1;
            continue;
        }

        // 한글 어절 또는 글자.
        // [#2185] bit7=1(KEEP_WORD)이 **글자 단위**, bit7=0(BREAK_WORD)이
        // 어절 단위 — 스키마 명목과 반대 (한컴 통제 실측 3중 확증: #2169
        // kbu 사다리, 80168 r10, #2185 giant-cell LINE_SEG [0,44,84,122]
        // 보존 대조). 종전 == 1 어절 분기는 역해석 (0da18bbc 회귀).
        if is_hangul(ch) {
            if korean_break_unit == 0 {
                // 어절 모드: 연속 한글 + 후행 금칙 문자를 하나의 토큰으로
                let start = i;
                let mut max_fs = 0.0f64;
                let mut token_text = String::new();
                let mut token_lang = current_lang;

                while i < text_len {
                    let c = text_chars[i];
                    if c == ' ' || c == '\n' || c == '\t' {
                        break;
                    }
                    // 한글이 아니고 라틴이면 다른 토큰으로 분리
                    if !is_hangul(c) && is_latin(c) {
                        break;
                    }
                    // CJK 한자/일본어는 개별 토큰
                    if is_cjk_ideograph(c) {
                        break;
                    }

                    let utf16_pos = if i < char_offsets.len() {
                        char_offsets[i]
                    } else {
                        i as u32
                    };
                    let style_id = find_active_char_shape(char_shapes, utf16_pos);
                    let lang = if is_lang_neutral(c) {
                        token_lang
                    } else {
                        let detected = detect_lang_category(c);
                        token_lang = detected;
                        current_lang = detected;
                        detected
                    };
                    let ts = resolved_to_text_style(styles, style_id, lang);
                    let fs = if ts.font_size > 0.0 {
                        ts.font_size
                    } else {
                        12.0
                    };
                    if fs > max_fs {
                        max_fs = fs;
                    }
                    token_text.push(c);
                    i += 1;
                }

                // 후행 금칙 문자 (줄 머리 금칙) 흡수
                while i < text_len
                    && is_line_start_forbidden(text_chars[i])
                    && text_chars[i] != '\n'
                    && text_chars[i] != '\t'
                {
                    let c = text_chars[i];
                    let utf16_pos = if i < char_offsets.len() {
                        char_offsets[i]
                    } else {
                        i as u32
                    };
                    let style_id = find_active_char_shape(char_shapes, utf16_pos);
                    let lang = if is_lang_neutral(c) {
                        current_lang
                    } else {
                        let detected = detect_lang_category(c);
                        current_lang = detected;
                        detected
                    };
                    let ts = resolved_to_text_style(styles, style_id, lang);
                    let fs = if ts.font_size > 0.0 {
                        ts.font_size
                    } else {
                        12.0
                    };
                    if fs > max_fs {
                        max_fs = fs;
                    }
                    token_text.push(c);
                    i += 1;
                }

                if !token_text.is_empty() {
                    let width = measure_token_width(
                        &token_text,
                        start,
                        char_offsets,
                        char_shapes,
                        styles,
                        current_lang,
                    );
                    tokens.push(BreakToken::Text {
                        start_idx: start,
                        end_idx: i,
                        width,
                        max_font_size: max_fs,
                        char_widths: vec![],
                    });
                }
                continue;
            } else {
                // 글자 모드: 한글 개별 분할
                let utf16_pos = if i < char_offsets.len() {
                    char_offsets[i]
                } else {
                    i as u32
                };
                let style_id = find_active_char_shape(char_shapes, utf16_pos);
                current_lang = detect_lang_category(ch);
                let ts = resolved_to_text_style(styles, style_id, current_lang);
                let fs = if ts.font_size > 0.0 {
                    ts.font_size
                } else {
                    12.0
                };
                let w = estimate_text_width_unrounded(&ch.to_string(), &ts);
                tokens.push(BreakToken::Text {
                    start_idx: i,
                    end_idx: i + 1,
                    width: w,
                    max_font_size: fs,
                    char_widths: vec![],
                });
                i += 1;
                continue;
            }
        }

        // 라틴 단어 또는 글자
        if is_latin(ch) {
            if english_break_unit == 0 || english_break_unit == 1 {
                // 단어/하이픈 모드: 연속 라틴 문자를 하나의 토큰으로
                let start = i;
                let mut max_fs = 0.0f64;
                let mut token_text = String::new();

                while i < text_len {
                    let c = text_chars[i];
                    if c == ' ' || c == '\n' || c == '\t' {
                        break;
                    }
                    if !is_latin(c) && !is_lang_neutral(c) {
                        break;
                    }
                    // 하이픈 모드: 하이픈에서 분할 (하이픈 포함 후 분리)
                    if english_break_unit == 1 && c == '-' && !token_text.is_empty() {
                        let utf16_pos = if i < char_offsets.len() {
                            char_offsets[i]
                        } else {
                            i as u32
                        };
                        let style_id = find_active_char_shape(char_shapes, utf16_pos);
                        let lang = 1usize; // English
                        let ts = resolved_to_text_style(styles, style_id, lang);
                        let fs = if ts.font_size > 0.0 {
                            ts.font_size
                        } else {
                            12.0
                        };
                        if fs > max_fs {
                            max_fs = fs;
                        }
                        token_text.push(c);
                        i += 1;
                        break; // 하이픈 뒤에서 분할
                    }

                    let utf16_pos = if i < char_offsets.len() {
                        char_offsets[i]
                    } else {
                        i as u32
                    };
                    let style_id = find_active_char_shape(char_shapes, utf16_pos);
                    let lang = if is_lang_neutral(c) {
                        current_lang
                    } else {
                        current_lang = 1; // English
                        1
                    };
                    let ts = resolved_to_text_style(styles, style_id, lang);
                    let fs = if ts.font_size > 0.0 {
                        ts.font_size
                    } else {
                        12.0
                    };
                    if fs > max_fs {
                        max_fs = fs;
                    }
                    token_text.push(c);
                    i += 1;
                }

                if !token_text.is_empty() {
                    let width = measure_token_width(
                        &token_text,
                        start,
                        char_offsets,
                        char_shapes,
                        styles,
                        current_lang,
                    );
                    // 개별 글자 폭 수집 (char_level_break용)
                    let cw: Vec<f64> = (start..i)
                        .map(|ci| {
                            let c = text_chars[ci];
                            let u16p = if ci < char_offsets.len() {
                                char_offsets[ci]
                            } else {
                                ci as u32
                            };
                            let sid = find_active_char_shape(char_shapes, u16p);
                            let lang = if is_lang_neutral(c) { current_lang } else { 1 };
                            let ts = resolved_to_text_style(styles, sid, lang);
                            estimate_text_width_unrounded(&c.to_string(), &ts)
                        })
                        .collect();
                    tokens.push(BreakToken::Text {
                        start_idx: start,
                        end_idx: i,
                        width,
                        max_font_size: max_fs,
                        char_widths: cw,
                    });
                }
                continue;
            } else {
                // 글자 모드
                let utf16_pos = if i < char_offsets.len() {
                    char_offsets[i]
                } else {
                    i as u32
                };
                let style_id = find_active_char_shape(char_shapes, utf16_pos);
                current_lang = 1;
                let ts = resolved_to_text_style(styles, style_id, current_lang);
                let fs = if ts.font_size > 0.0 {
                    ts.font_size
                } else {
                    12.0
                };
                let w = estimate_text_width_unrounded(&ch.to_string(), &ts);
                tokens.push(BreakToken::Text {
                    start_idx: i,
                    end_idx: i + 1,
                    width: w,
                    max_font_size: fs,
                    char_widths: vec![],
                });
                i += 1;
                continue;
            }
        }

        // CJK 한자/일본어: 항상 개별 토큰
        if is_cjk_ideograph(ch) {
            let utf16_pos = if i < char_offsets.len() {
                char_offsets[i]
            } else {
                i as u32
            };
            let style_id = find_active_char_shape(char_shapes, utf16_pos);
            current_lang = detect_lang_category(ch);
            let ts = resolved_to_text_style(styles, style_id, current_lang);
            let fs = if ts.font_size > 0.0 {
                ts.font_size
            } else {
                12.0
            };
            let w = estimate_text_width_unrounded(&ch.to_string(), &ts);
            tokens.push(BreakToken::Text {
                start_idx: i,
                end_idx: i + 1,
                width: w,
                max_font_size: fs,
                char_widths: vec![],
            });
            i += 1;
            continue;
        }

        // 기타 문자 (기호, NonBreakingSpace 등): 개별 Text 토큰
        {
            let utf16_pos = if i < char_offsets.len() {
                char_offsets[i]
            } else {
                i as u32
            };
            let style_id = find_active_char_shape(char_shapes, utf16_pos);
            let lang = if is_lang_neutral(ch) {
                current_lang
            } else {
                let detected = detect_lang_category(ch);
                current_lang = detected;
                detected
            };
            let ts = resolved_to_text_style(styles, style_id, lang);
            let fs = if ts.font_size > 0.0 {
                ts.font_size
            } else {
                12.0
            };
            let w = estimate_text_width_unrounded(&ch.to_string(), &ts);
            tokens.push(BreakToken::Text {
                start_idx: i,
                end_idx: i + 1,
                width: w,
                max_font_size: fs,
                char_widths: vec![],
            });
            i += 1;
        }
    }

    tokens
}

/// 토큰 텍스트의 폭을 글자별 언어 인식 측정으로 합산한다.
fn measure_token_width(
    text: &str,
    start_char_idx: usize,
    char_offsets: &[u32],
    char_shapes: &[CharShapeRef],
    styles: &ResolvedStyleSet,
    default_lang: usize,
) -> f64 {
    let mut total = 0.0;
    let mut current_lang = default_lang;
    for (offset, ch) in text.chars().enumerate() {
        let idx = start_char_idx + offset;
        let utf16_pos = if idx < char_offsets.len() {
            char_offsets[idx]
        } else {
            idx as u32
        };
        let style_id = find_active_char_shape(char_shapes, utf16_pos);
        let lang = if is_lang_neutral(ch) {
            current_lang
        } else {
            let detected = detect_lang_category(ch);
            current_lang = detected;
            detected
        };
        let ts = resolved_to_text_style(styles, style_id, lang);
        total += estimate_text_width_unrounded(&ch.to_string(), &ts);
    }
    total
}

/// px를 HWPUNIT(i32)로 변환 (내림, DPI=96 기준: px * 75)
#[inline]
fn to_hwp(px: f64) -> i32 {
    (px * 75.0) as i32
}

fn condense_space_savings_hwp(space_width_hwp: i32, condense_min_space: u8) -> i32 {
    if condense_min_space == 0 || space_width_hwp <= 0 {
        return 0;
    }
    let shrink_percent = condense_min_space.min(75) as i32;
    space_width_hwp * shrink_percent / 100
}

fn condensed_line_width_hwp(width_hwp: i32, space_savings_hwp: i32) -> i32 {
    width_hwp - space_savings_hwp
}

fn condense_fit_can_pull_next_token(
    current_width_hwp: i32,
    current_space_savings_hwp: i32,
    effective_width_hwp: i32,
    max_font_size: f64,
) -> bool {
    let current_condensed_width =
        condensed_line_width_hwp(current_width_hwp, current_space_savings_hwp);
    let remaining_hwp = effective_width_hwp - current_condensed_width;
    // Hancom uses condense to rescue a line that still has a meaningful
    // natural gap, but it does not pull the next word into an already tight
    // line. The p03 PDF preface is sensitive to that distinction.
    let min_remaining_hwp = to_hwp((max_font_size * 2.5).max(20.0));
    remaining_hwp >= min_remaining_hwp
}

/// 토큰을 줄에 배치하는 Greedy 알고리즘
/// 한컴과 동일한 결과를 위해 HWPUNIT 정수로 폭을 누적한다.
fn fill_lines(
    tokens: &[BreakToken],
    text_chars: &[char],
    available_width_px: f64,
    indent_px: f64,
    default_tab_width: f64,
    korean_break_unit: u8,
    condense_min_space: u8,
) -> Vec<LineBreakResult> {
    fill_lines_per_line(
        tokens,
        text_chars,
        available_width_px,
        indent_px,
        default_tab_width,
        korean_break_unit,
        condense_min_space,
        None,
    )
}

/// [officex/어울림 본편] 줄바꿈에 줄별 가용 폭을 공급하는 확장판.
///
/// `per_line_width(line_idx)` 가 Some(px) 을 주면 그 줄의 가용 폭이 그 값이 된다
/// (부분폭 밴드 옆 줄). None 이면 `available_width_px`. 들여쓰기 규칙은 폭 위에
/// 그대로 적용된다. 기본 경로(fill_lines)는 공급자 None 으로 위임 — 동작 불변.
#[allow(clippy::too_many_arguments)]
fn fill_lines_per_line(
    tokens: &[BreakToken],
    text_chars: &[char],
    available_width_px: f64,
    indent_px: f64,
    default_tab_width: f64,
    korean_break_unit: u8,
    condense_min_space: u8,
    per_line_width: Option<&dyn Fn(usize) -> Option<f64>>,
) -> Vec<LineBreakResult> {
    if tokens.is_empty() {
        return vec![LineBreakResult {
            start_idx: 0,
            end_idx: 0,
            max_font_size: 0.0,
            has_line_break: false,
            continues_line: false,
            object_height_hwp: 0,
        }];
    }

    let tab_w_hwp = to_hwp(if default_tab_width > 0.0 {
        default_tab_width
    } else {
        48.0
    });
    let tab_w_px = if default_tab_width > 0.0 {
        default_tab_width
    } else {
        48.0
    };
    let mut results = Vec::new();
    let mut line_start_idx = 0usize;
    let mut lw = 0i32; // HWPUNIT 정수 누적
    let mut line_space_savings = 0i32;
    let mut line_max_fs = 0.0f64;
    // 이 줄에 놓인 TAC 개체의 최대 높이 — 줄 상자를 개체에 맞추는 데 쓴다.
    let mut line_max_obj_h = 0i32;
    let mut is_first_line = true;

    let mut last_break_token_idx: Option<usize> = None;
    let mut last_break_char_idx: usize = 0;
    let mut width_at_last_break = 0i32;
    let mut space_savings_at_last_break = 0i32;
    let mut fs_at_last_break = 0.0f64;

    let mut current_line_idx = 0usize;
    let base_w = |line_idx: usize| -> f64 {
        per_line_width
            .and_then(|f| f(line_idx))
            .unwrap_or(available_width_px)
    };
    let eff_w_at = |first: bool, line_idx: usize| -> i32 {
        let w = base_w(line_idx);
        if indent_px > 0.0 {
            if first {
                to_hwp((w - indent_px).max(1.0))
            } else {
                to_hwp(w)
            }
        } else if indent_px < 0.0 {
            if first {
                to_hwp(w)
            } else {
                to_hwp((w + indent_px).max(1.0))
            }
        } else {
            to_hwp(w)
        }
    };

    for (ti, token) in tokens.iter().enumerate() {
        match token {
            BreakToken::LineBreak { idx } => {
                results.push(LineBreakResult {
                    start_idx: line_start_idx,
                    end_idx: *idx + 1,
                    max_font_size: line_max_fs,
                    has_line_break: true,
                    continues_line: false,
                    object_height_hwp: line_max_obj_h,
                });
                current_line_idx += 1;
                line_start_idx = *idx + 1;
                lw = 0;
                line_space_savings = 0;
                line_max_fs = 0.0;
                line_max_obj_h = 0;
                is_first_line = false;
                last_break_token_idx = None;
            }
            BreakToken::Tab { idx, max_font_size } => {
                // 탭 계산은 px로 수행 후 HWPUNIT 변환 (정밀도 유지)
                let lw_px = lw as f64 / 75.0;
                let next_tab_px = ((lw_px / tab_w_px).floor() + 1.0) * tab_w_px;
                let next_tab_hwp = to_hwp(next_tab_px);
                if *max_font_size > line_max_fs {
                    line_max_fs = *max_font_size;
                }

                if next_tab_hwp > eff_w_at(is_first_line, current_line_idx) && line_start_idx < *idx
                {
                    if let Some(_) = last_break_token_idx {
                        results.push(LineBreakResult {
                            start_idx: line_start_idx,
                            end_idx: last_break_char_idx,
                            max_font_size: fs_at_last_break,
                            has_line_break: false,
                            continues_line: false,
                            object_height_hwp: line_max_obj_h,
                        });
                        current_line_idx += 1;
                        line_start_idx = last_break_char_idx;
                        lw = lw - width_at_last_break;
                        line_space_savings -= space_savings_at_last_break;
                        line_max_obj_h = 0;
                    } else {
                        results.push(LineBreakResult {
                            start_idx: line_start_idx,
                            end_idx: *idx,
                            max_font_size: line_max_fs,
                            has_line_break: false,
                            continues_line: false,
                            object_height_hwp: line_max_obj_h,
                        });
                        current_line_idx += 1;
                        line_start_idx = *idx;
                        lw = 0;
                        line_space_savings = 0;
                        line_max_fs = *max_font_size;
                        line_max_obj_h = 0;
                    }
                    is_first_line = false;
                    last_break_token_idx = None;
                    let lw_px2 = lw as f64 / 75.0;
                    let next_tab2 = ((lw_px2 / tab_w_px).floor() + 1.0) * tab_w_px;
                    lw = to_hwp(next_tab2);
                } else {
                    last_break_token_idx = Some(ti);
                    last_break_char_idx = *idx;
                    width_at_last_break = lw;
                    space_savings_at_last_break = line_space_savings;
                    fs_at_last_break = line_max_fs;
                    lw = next_tab_hwp;
                }
            }
            BreakToken::Space {
                idx,
                width,
                max_font_size,
            } => {
                if *max_font_size > line_max_fs {
                    line_max_fs = *max_font_size;
                }
                last_break_token_idx = Some(ti);
                last_break_char_idx = *idx;
                width_at_last_break = lw;
                space_savings_at_last_break = line_space_savings;
                fs_at_last_break = line_max_fs;
                let space_hwp = to_hwp(*width);
                lw += space_hwp;
                line_space_savings += condense_space_savings_hwp(space_hwp, condense_min_space);
            }
            BreakToken::Object {
                idx,
                width_hwp,
                height_hwp,
            } => {
                // [코퍼스 (h)] TAC 개체는 "큰 글자" — 남은 폭에 안 들어가면 개체 앞에서
                // 줄을 끊는다. 개체 하나가 줄폭보다 넓으면(대형 표) 줄 시작에 그대로
                // 놓여 넘치고, 뒤 텍스트는 아래 Text arm 의 역추적으로 다음 줄이 된다.
                let w_hwp = *width_hwp;
                if lw > 0
                    && condensed_line_width_hwp(lw + w_hwp, line_space_savings)
                        > eff_w_at(is_first_line, current_line_idx)
                {
                    results.push(LineBreakResult {
                        start_idx: line_start_idx,
                        end_idx: *idx,
                        max_font_size: line_max_fs,
                        has_line_break: false,
                        continues_line: false,
                        object_height_hwp: line_max_obj_h,
                    });
                    current_line_idx += 1;
                    line_start_idx = *idx;
                    lw = 0;
                    line_space_savings = 0;
                    line_max_fs = 0.0;
                    line_max_obj_h = 0;
                    is_first_line = false;
                }
                lw += w_hwp;
                line_max_obj_h = line_max_obj_h.max(*height_hwp);
                // 개체 뒤는 줄바꿈 가능 지점 — 개체는 이 줄에 남고 뒤 텍스트만 내려간다.
                // (recalc_width_hwp 가 Object 를 세지 않는 것과 짝을 이룬다.)
                last_break_token_idx = Some(ti);
                last_break_char_idx = *idx;
                width_at_last_break = lw;
                space_savings_at_last_break = line_space_savings;
                fs_at_last_break = line_max_fs;
            }
            BreakToken::Text {
                start_idx,
                end_idx,
                width,
                max_font_size,
                ref char_widths,
            } => {
                if *max_font_size > line_max_fs {
                    line_max_fs = *max_font_size;
                }

                let w_hwp = to_hwp(*width);

                // 단일 문자 CJK/한글 토큰의 줄바꿈 가능 지점 처리
                // 이 글자를 포함한 후 break point 갱신 (end_idx 사용)
                // → 초과 시 이 글자까지 L0에 포함하고 다음 토큰부터 다음 줄
                if *end_idx - *start_idx == 1 && *start_idx > line_start_idx {
                    let c = text_chars[*start_idx];
                    let allow_break = if is_hangul(c) {
                        // [#2185] bit7=1 = 글자 단위 break 허용 (위 주석 참조)
                        korean_break_unit == 1
                    } else {
                        is_cjk_ideograph(c)
                    };
                    let candidate_w = lw + w_hwp;
                    // 이 글자가 줄에 들어가는 경우에만 break point 갱신
                    if allow_break
                        && condensed_line_width_hwp(candidate_w, line_space_savings)
                            <= eff_w_at(is_first_line, current_line_idx) + LINE_BREAK_TOLERANCE
                    {
                        last_break_token_idx = Some(ti);
                        last_break_char_idx = *end_idx; // 이 글자 다음 (이 글자 포함)
                        width_at_last_break = candidate_w; // 이 글자 폭 포함
                        space_savings_at_last_break = line_space_savings;
                        fs_at_last_break = line_max_fs;
                    }
                }
                // 한컴은 HWPUNIT 정수 양자화 시 미세한 반올림 차이를 허용
                // 12 HU(~0.17mm) 이내의 초과는 줄에 포함 (경험적 허용 오차)
                const LINE_BREAK_TOLERANCE: i32 = 15;
                let effective_width = eff_w_at(is_first_line, current_line_idx);
                let natural_candidate = lw + w_hwp;
                let condensed_candidate =
                    condensed_line_width_hwp(natural_candidate, line_space_savings);
                let needs_condense_to_fit = natural_candidate
                    > effective_width + LINE_BREAK_TOLERANCE
                    && condensed_candidate <= effective_width + LINE_BREAK_TOLERANCE;
                let condense_pull_allowed = !needs_condense_to_fit
                    || condense_fit_can_pull_next_token(
                        lw,
                        line_space_savings,
                        effective_width,
                        *max_font_size,
                    );
                if condensed_candidate > effective_width + LINE_BREAK_TOLERANCE
                    || !condense_pull_allowed
                {
                    // 직전 줄바꿈 지점이 TAC 개체면 이 줄의 텍스트가 0글자여도 정당한
                    // 줄이다(개체만의 줄) — `start_idx > line_start_idx` 가드는 그 경우
                    // 텍스트를 개체 옆에 억지로 붙여 줄폭을 넘긴다.
                    let breaking_after_object = last_break_token_idx
                        .is_some_and(|b| matches!(tokens[b], BreakToken::Object { .. }));
                    if *start_idx > line_start_idx || breaking_after_object {
                        if let Some(_) = last_break_token_idx {
                            results.push(LineBreakResult {
                                start_idx: line_start_idx,
                                end_idx: last_break_char_idx,
                                max_font_size: fs_at_last_break,
                                has_line_break: false,
                                continues_line: false,
                                object_height_hwp: line_max_obj_h,
                            });
                            current_line_idx += 1;
                            let mut next_start = last_break_char_idx;
                            while next_start < text_chars.len() && text_chars[next_start] == ' ' {
                                next_start += 1;
                            }
                            line_start_idx = next_start;
                            lw = recalc_width_hwp(tokens, ti, next_start);
                            line_space_savings = recalc_space_savings_hwp(
                                tokens,
                                ti,
                                next_start,
                                condense_min_space,
                            );
                            lw += w_hwp;
                            line_max_fs = *max_font_size;
                            line_max_obj_h = 0;
                            is_first_line = false;
                            last_break_token_idx = None;
                            continue;
                        }
                    }
                    // 토큰에 저장된 개별 글자 폭을 HWPUNIT로 변환
                    let cw_hwp: Vec<i32> = char_widths.iter().map(|w| to_hwp(*w)).collect();
                    let (results_part, remaining_w, remaining_fs) = char_level_break_hwp(
                        text_chars,
                        *start_idx,
                        *end_idx,
                        &mut line_start_idx,
                        lw,
                        line_max_fs,
                        eff_w_at(is_first_line, current_line_idx),
                        eff_w_at(false, current_line_idx + 1),
                        is_first_line,
                        &cw_hwp,
                    );
                    for (k, mut r) in results_part.into_iter().enumerate() {
                        if k == 0 {
                            // 쪼개진 첫 줄만 진행 중이던 줄 — 개체가 있었다면 그 줄 소유.
                            r.object_height_hwp = line_max_obj_h;
                            line_max_obj_h = 0;
                        }
                        results.push(r);
                        is_first_line = false;
                    }
                    lw = remaining_w;
                    line_space_savings = 0;
                    line_max_fs = remaining_fs;
                    last_break_token_idx = None;
                    continue;
                } else {
                    lw += w_hwp;
                }
            }
        }
    }

    let last_end = tokens
        .last()
        .map(|t| match t {
            BreakToken::Text { end_idx, .. } => *end_idx,
            BreakToken::Space { idx, .. }
            | BreakToken::Tab { idx, .. }
            | BreakToken::LineBreak { idx } => *idx + 1,
            // 개체는 char 를 소비하지 않는다 — 뒤에 텍스트가 없으면 여기서 끝.
            BreakToken::Object { idx, .. } => *idx,
        })
        .unwrap_or(text_chars.len());

    if line_start_idx <= last_end {
        results.push(LineBreakResult {
            start_idx: line_start_idx,
            end_idx: last_end,
            max_font_size: line_max_fs,
            has_line_break: false,
            continues_line: false,
            object_height_hwp: line_max_obj_h,
        });
        current_line_idx += 1;
    }

    if results.is_empty() {
        results.push(LineBreakResult {
            start_idx: 0,
            end_idx: text_chars.len(),
            max_font_size: 0.0,
            has_line_break: false,
            continues_line: false,
            object_height_hwp: line_max_obj_h,
        });
        current_line_idx += 1;
    }

    results
}

/// 줄 바꿈 지점 이후 토큰의 누적 폭 재계산 (HWPUNIT)
fn recalc_width_hwp(tokens: &[BreakToken], current_token_idx: usize, new_line_start: usize) -> i32 {
    let mut w = 0i32;
    for t in &tokens[..current_token_idx] {
        match t {
            BreakToken::Text {
                start_idx, width, ..
            } if *start_idx >= new_line_start => {
                w += to_hwp(*width);
            }
            BreakToken::Space { idx, width, .. } if *idx >= new_line_start => {
                w += to_hwp(*width);
            }
            _ => {}
        }
    }
    w
}

/// 줄 바꿈 지점 이후 공백 압축 가능 폭 재계산 (HWPUNIT)
fn recalc_space_savings_hwp(
    tokens: &[BreakToken],
    current_token_idx: usize,
    new_line_start: usize,
    condense_min_space: u8,
) -> i32 {
    let mut w = 0i32;
    for t in &tokens[..current_token_idx] {
        match t {
            BreakToken::Space {
                idx,
                width,
                max_font_size,
            } if *idx >= new_line_start => {
                let space_hwp = to_hwp(*width);
                w += condense_space_savings_hwp(space_hwp, condense_min_space);
            }
            _ => {}
        }
    }
    w
}

/// 긴 단어 폴백: 글자 단위 분할 (HWPUNIT)
/// char_widths_hwp: 토큰 내 각 글자의 HWPUNIT 폭 (None이면 휴리스틱)
fn char_level_break_hwp(
    text_chars: &[char],
    token_start: usize,
    token_end: usize,
    line_start_idx: &mut usize,
    mut lw: i32,
    mut line_max_fs: f64,
    first_line_w: i32,
    normal_w: i32,
    mut is_first_line: bool,
    char_widths_hwp: &[i32], // 토큰 내 글자별 HWPUNIT 폭
) -> (Vec<LineBreakResult>, i32, f64) {
    let mut results = Vec::new();
    let mut current_w = if is_first_line {
        first_line_w
    } else {
        normal_w
    };

    for ci in token_start..token_end {
        let rel_idx = ci - token_start;
        let char_w = if rel_idx < char_widths_hwp.len() {
            char_widths_hwp[rel_idx]
        } else {
            let ch = text_chars[ci];
            let char_w_px = if is_cjk_char(ch) {
                line_max_fs.max(12.0)
            } else {
                line_max_fs.max(12.0) * 0.5
            };
            to_hwp(char_w_px)
        };

        if lw + char_w > current_w && ci > *line_start_idx {
            results.push(LineBreakResult {
                start_idx: *line_start_idx,
                end_idx: ci,
                max_font_size: line_max_fs,
                has_line_break: false,
                continues_line: false,
                object_height_hwp: 0,
            });
            *line_start_idx = ci;
            lw = char_w;
            is_first_line = false;
            current_w = normal_w;
        } else {
            lw += char_w;
        }
    }

    (results, lw, line_max_fs)
}

/// 문단 안 TAC 개체의 최대 **글리프 높이**(= 개체 높이 + 바깥여백 상하, 횡단 법칙 1).
/// 상자 정의는 `composer::tac_box_hwp` 단일 소스.
fn inline_control_line_height_hwp(para: &Paragraph) -> Option<i32> {
    para.controls
        .iter()
        .filter_map(super::tac_box_hwp)
        .map(|b| b.glyph_height())
        .filter(|height| *height > 0)
        .max()
}

/// 줄 채움에 폭으로 참여하는 TAC 개체의 **글리프 상자**(폭, 높이). 잉크 크기가 미상인
/// (폭·높이 0) 개체는 제외 — 폭 판정을 할 수 없다.
fn inline_control_glyph_box_hwp(ctrl: &Control) -> Option<(i32, i32)> {
    super::tac_box_hwp(ctrl)
        .filter(super::TacBox::has_ink)
        .map(|b| (b.glyph_width(), b.glyph_height()))
}

/// [oracle-corpus-mining-20260805 (h)] TAC 개체를 **폭을 가진 한 글자**로 토큰 흐름에
/// 끼워 넣는다 — 개체가 놓이는 텍스트 char 인덱스(`control_text_positions`) 바로 앞.
/// 이걸로 줄바꿈이 개체 폭을 보게 되고, 대형 표의 "자기 줄"은 특례가 아니라 폭 초과의
/// 귀결이 된다(횡단 법칙 3).
///
/// [oracle-pdf-mining-20260806 §1-B/§1-C] end-anchor 축도 예외가 아니다: 저장 코퍼스
/// 656파일·글자취급 표 5,751건 전수에서 "폭이 남고 `\n` 도 없는" END-anchor 표는 전부
/// seg 1개(표+글 한 줄)이고, `samples/tac-case-001..005` 는 표가 넓어질 때 **뒤 텍스트만**
/// 다음 줄로 밀리고 표는 앞 텍스트 줄에 남음을 보인다. 종전의 end-anchor 제외(자기 줄
/// 생산과 짝)는 이로써 폭 규칙 하나로 흡수됐다.
fn insert_object_tokens(tokens: &mut Vec<BreakToken>, para: &Paragraph) {
    if para.controls.is_empty() {
        return;
    }
    let positions = para.control_text_positions();
    let mut objects: Vec<(usize, i32, i32)> = para
        .controls
        .iter()
        .enumerate()
        .filter_map(|(i, ctrl)| {
            // 글리프 상자 = 잉크 + 바깥여백 (`composer::tac_box_hwp` 단일 소스).
            // 폭: 기부 양식 s0#25 47813+570 vs sw 47833, aift s0#0 47624+566 vs sw 48188 —
            // 맨몸 폭만 보면 둘 다 "들어감"이 된다.
            // 높이: 저장 lh 32352=31782+570 / 15998=15432+566 을 그대로 재현한다.
            let (w_hwp, h_hwp) = inline_control_glyph_box_hwp(ctrl)?;
            Some((positions.get(i).copied()?, w_hwp, h_hwp))
        })
        .collect();
    if objects.is_empty() {
        return;
    }
    objects.sort_by_key(|(idx, ..)| *idx);
    let token_char_idx = |t: &BreakToken| match t {
        BreakToken::Text { start_idx, .. } => *start_idx,
        BreakToken::Space { idx, .. }
        | BreakToken::Tab { idx, .. }
        | BreakToken::LineBreak { idx }
        | BreakToken::Object { idx, .. } => *idx,
    };
    // 뒤에서부터 넣어 앞쪽 삽입 위치가 밀리지 않게 한다.
    for (idx, width_hwp, height_hwp) in objects.into_iter().rev() {
        let at = tokens
            .iter()
            .position(|t| token_char_idx(t) >= idx)
            .unwrap_or(tokens.len());
        tokens.insert(
            at,
            BreakToken::Object {
                idx,
                width_hwp,
                height_hwp,
            },
        );
    }
}

/// `baseline_ratio` = 그 문단의 r (세로 정렬 — `para_vertical_align_baseline_ratio`).
fn apply_inline_control_line_height(seg: &mut LineSeg, height_hwp: i32, baseline_ratio: f64) {
    if height_hwp > seg.line_height {
        seg.line_height = height_hwp;
        seg.text_height = height_hwp;
        seg.baseline_distance = (height_hwp as f64 * baseline_ratio).round() as i32;
    }
}

/// [이모지 줄박스 2026-08-04] 독스 원리 — 이모지가 있는 줄은 줄 상자가 글리프를 감싼다.
/// 이모지는 글자 크기 그대로 그리므로(무보정) 잉크가 한글보다 크다: 실측 top −0.88em·
/// bottom +0.11em(한글 −0.80/+0.15). 상자 1.15em + 기준선 0.96em 이면 위아래를 다 덮고,
/// 상자는 **아래로** 자란다 — 선택 하이라이트·캐럿이 TextLine bbox 를 쓰므로 같이 커진다.
fn apply_emoji_line_height(seg: &mut LineSeg, font_size_px: f64, dpi: f64) {
    let lh = px_to_hwpunit(font_size_px * 1.15, dpi);
    if lh > seg.line_height {
        seg.line_height = lh;
        seg.text_height = lh;
        seg.baseline_distance = px_to_hwpunit(font_size_px * 0.96, dpi);
    }
}

/// [officex/어울림 본편] 부분폭 밴드(빈 host Square 표 상자)의 세로 구간.
/// 좌표는 **컬럼 로컬 px**(문단이 배치되는 단의 x=0 기준), top 은 문서 흐름 y.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ReflowBand {
    pub top_px: f64,
    pub bottom_px: f64,
    pub x0_px: f64,
    pub x1_px: f64,
    /// 한컴 "본문 위치" — 글이 개체의 어느 쪽에 흐르나(양쪽/왼쪽/오른쪽/큰 쪽).
    pub flow: crate::model::shape::TextFlow,
}

/// 옆 조각으로 인정하는 최소 폭(px).
///
/// [2026-07-30 한컴 실측] samples/pic2.hwpx 의 linesegarray 는 좌 조각 **horzsize=2570HU
/// = 34.3px@96dpi** 를 유지한다(그 폭에 글이 안 들어가는 줄은 세그를 없애지 않고
/// flags 에 EMPTY(0x10000)를 얹어 남긴다). 종전 40px 은 근거 없는 값이라 34~40px 구간의
/// 옆 흐름을 통째로 포기했다 — 실측 하한으로 내린다.
/// ⚠ 우리 fill 은 빈 세그를 생산할 수 없어(줄 시작 토큰 강제 배치) 이 하한 미달이면
/// 여전히 한쪽 흐름으로 접는다. 시각 결과는 한컴의 EMPTY 세그와 같다(부록4 갭 #1).
pub const MIN_SIDE_PX: f64 = 34.0;

/// 밴드 옆 공간 선택 — 한컴 "본문 위치"를 따른다(최소 MIN_SIDE_PX).
/// 반환 (cs_px, w_px): 줄 시작 오프셋(컬럼 로컬)과 가용 폭.
///
/// - 왼쪽(LeftOnly): 개체 왼쪽에만 글 · 오른쪽(RightOnly): 오른쪽에만
/// - 큰 쪽(LargestOnly): 넓은 쪽 하나(동률은 오른쪽)
/// - 양쪽(BothSides): 좌·우 두 세그로 쪼갠다 — 그 분기는 segs_for_top 이 먼저 처리하므로
///   이 함수에 오는 BothSides 는 **한쪽이 하한 미달**인 경우뿐이고 큰 쪽으로 접는다.
pub fn side_pick_for_band(
    full_w_px: f64,
    x0: f64,
    x1: f64,
    flow: crate::model::shape::TextFlow,
) -> Option<(f64, f64)> {
    use crate::model::shape::TextFlow;
    let left_room = x0.max(0.0);
    let right_room = (full_w_px - x1).max(0.0);
    let left = || (left_room >= MIN_SIDE_PX).then_some((0.0, left_room));
    let right = || (right_room >= MIN_SIDE_PX).then_some((x1, right_room));
    match flow {
        TextFlow::LeftOnly => left(),
        TextFlow::RightOnly => right(),
        TextFlow::BothSides | TextFlow::LargestOnly => {
            if right_room >= left_room {
                right().or_else(left)
            } else {
                left().or_else(right)
            }
        }
    }
}

pub(crate) fn reflow_line_segs(
    para: &mut Paragraph,
    available_width_px: f64,
    styles: &ResolvedStyleSet,
    dpi: f64,
) {
    reflow_line_segs_with_bands(para, available_width_px, styles, dpi, 0.0, &[]);
}

/// 줄바꿈이 부분폭 밴드를 인지하는 본체 — 밴드와 세로로 겹치는 줄은 옆 남은 폭으로
/// 줄바꿈되고, 그 줄의 line_seg 에 column_start/segment_width 가 **줄별로** 기록된다
/// (한컴 저장 형식과 동일 — 이후 렌더는 기존 재생 파이프라인이 그대로 소비).
/// 줄 폭이 줄 수를 바꾸고 줄 수가 줄 위치(top)를 바꾸는 상호의존은 고정점 반복
/// (최대 3회 — 실측 2회 수렴)으로 푼다.
pub(crate) fn reflow_line_segs_with_bands(
    para: &mut Paragraph,
    available_width_px: f64,
    styles: &ResolvedStyleSet,
    dpi: f64,
    para_top_px: f64,
    bands: &[ReflowBand],
) {
    // 기존 LineSeg에서 dimension 값 보존 (원본 HWP 호환성 유지)
    let seg_width_hwp = px_to_hwpunit(available_width_px, dpi);
    let orig = para.line_segs.first().cloned();
    let has_valid_orig = orig.as_ref().map(|ls| ls.line_height > 0).unwrap_or(false);

    // ParaPr의 줄간격 설정 (합성 LineSeg에서 line_spacing 계산에 사용)
    let para_style = styles.para_styles.get(para.para_shape_id as usize);
    let ls_type = para_style
        .map(|s| s.line_spacing_type)
        .unwrap_or(LineSpacingType::Percent);
    let ls_value = para_style.map(|s| s.line_spacing).unwrap_or(160.0);
    // 줄 기준선 비율 r = bd/lh — 문단 모양의 세로 정렬에서 온다(글꼴기준 0.85 /
    // 가운데 0.50 / 아래쪽 1.00, oracle-pdf-mining-20260806 §2-A).
    let baseline_ratio = para_style.map(|s| s.line_baseline_ratio).unwrap_or(crate::renderer::style_resolver::FONT_BASELINE_RATIO);

    // 줄별 max_font_size에 따라 line_height/text_height/baseline_distance를 계산
    // 한컴은 줄마다 최대 폰트 크기에 맞게 다른 치수를 사용
    let make_line_seg = |utf16_start: u32, max_font_size: f64| -> LineSeg {
        let fs = if max_font_size > 0.0 {
            max_font_size
        } else {
            12.0
        };
        let line_height_hwp = font_size_to_line_height(fs, dpi);
        let text_height_hwp = line_height_hwp;
        let baseline_distance_hwp = (line_height_hwp as f64 * baseline_ratio) as i32;
        let line_spacing_hwp = compute_line_spacing_hwp(ls_type, ls_value, line_height_hwp, dpi);
        // [Task #1811] 원본 linesegarray 부재(orig=None) 시 합성 seg 에 구현속성
        // 태그를 부여 — vpos 보정 등에서 실제 저장 증거와 구분한다 (컨버터의
        // 합성 lineseg flags=0x8000_0000 관례와 정합).
        let orig_tag = orig
            .as_ref()
            .map(|ls| ls.tag)
            .unwrap_or(LineSeg::TAG_SINGLE_SEGMENT_LINE | LineSeg::TAG_IMPLEMENTATION_PROPERTY);
        LineSeg {
            text_start: utf16_start,
            line_height: line_height_hwp,
            text_height: text_height_hwp,
            baseline_distance: baseline_distance_hwp,
            line_spacing: line_spacing_hwp,
            segment_width: seg_width_hwp,
            tag: if orig_tag != 0 {
                orig_tag
            } else {
                LineSeg::TAG_SINGLE_SEGMENT_LINE
            },
            ..Default::default()
        }
    };

    if para.text.is_empty() {
        // 텍스트 없는 문단도 개체는 "큰 글자" — 줄 채움·줄 높이 모두 글리프 상자
        // (잉크 + 바깥여백)로 잰다. 텍스트 있는 문단의 `BreakToken::Object` 경로와
        // 같은 상자여야 표만 있는 문단과 표+글 문단의 줄높이가 갈리지 않는다.
        let inline_sizes = para
            .controls
            .iter()
            .filter_map(inline_control_glyph_box_hwp)
            .collect::<Vec<_>>();
        if !inline_sizes.is_empty() {
            let max_line_width = seg_width_hwp.max(1);
            let mut line_specs: Vec<(usize, i32, i32)> = Vec::new();
            let mut line_start = 0usize;
            let mut line_width = 0i32;
            let mut line_height = 0i32;

            for (idx, (ctrl_width, ctrl_height)) in inline_sizes.iter().copied().enumerate() {
                if line_width > 0 && line_width + ctrl_width > max_line_width {
                    line_specs.push((line_start, line_width, line_height));
                    line_start = idx;
                    line_width = 0;
                    line_height = 0;
                }
                line_width += ctrl_width;
                line_height = line_height.max(ctrl_height);
            }
            line_specs.push((line_start, line_width, line_height));

            let orig_line_segs = para.line_segs.clone();
            let mut new_line_segs = Vec::with_capacity(line_specs.len());
            for (line_idx, (start_pos, _line_width, height_hwp)) in
                line_specs.into_iter().enumerate()
            {
                let mut seg = make_line_seg(start_pos as u32, 0.0);
                if let Some(template) = orig_line_segs
                    .get(line_idx)
                    .or_else(|| orig_line_segs.first())
                {
                    // ⚠ line_spacing 은 **상속하지 않는다** — 문단 모양에서 다시 계산한
                    // make_line_seg 값을 쓴다. 종전 상속은 개체(표·그림)만 있는 문단의
                    // 줄간격을 첫 값에 고착시켜, 글자처럼취급 이후 줄간격을 바꿔도
                    // 화면이 그대로였다(2026-08-11 신고). 텍스트 문단은 원래 재계산한다.
                    seg.segment_width = if template.segment_width > 0 {
                        template.segment_width
                    } else {
                        seg_width_hwp
                    };
                    seg.tag = if template.tag != 0 {
                        template.tag
                    } else {
                        seg.tag
                    };
                }
                apply_inline_control_line_height(&mut seg, height_hwp, baseline_ratio);
                new_line_segs.push(seg);
            }

            let mut vpos = orig.as_ref().map(|ls| ls.vertical_pos).unwrap_or(0);
            for seg in &mut new_line_segs {
                seg.vertical_pos = vpos;
                vpos += seg.line_height + seg.line_spacing;
            }
            para.line_segs = new_line_segs;
        } else {
            // 빈 문단도 활성 글자 모양의 크기로 줄을 만든다. 앞 문단 LINE_SEG의
            // 치수를 복사하면 TAC 그림 높이까지 상속되므로 vpos 원점만 보존한다.
            let font_size = para
                .char_shapes
                .first()
                .and_then(|char_shape| styles.char_styles.get(char_shape.char_shape_id as usize))
                .map(|style| style.font_size)
                .unwrap_or(12.0);
            let mut seg = make_line_seg(0, font_size);
            if let Some(template) = orig.as_ref() {
                seg.vertical_pos = template.vertical_pos;
            }
            if let Some(height_hwp) = inline_control_line_height_hwp(para) {
                apply_inline_control_line_height(&mut seg, height_hwp, baseline_ratio);
            }
            para.line_segs = vec![seg];
        }
        return;
    }

    let text_chars: Vec<char> = para.text.chars().collect();
    let text_len = text_chars.len();

    // 문단 스타일에서 들여쓰기 및 줄 나눔 설정 조회
    let para_style = styles.para_styles.get(para.para_shape_id as usize);
    let indent_px = para_style.map(|s| s.indent).unwrap_or(0.0);
    let english_break_unit = para_style.map(|s| s.english_break_unit).unwrap_or(0);
    let korean_break_unit = para_style.map(|s| s.korean_break_unit).unwrap_or(0);
    let condense_min_space = para_style.map(|s| s.condense_min_space).unwrap_or(0);
    let tab_width = para_style.map(|s| s.default_tab_width).unwrap_or(0.0);

    // 토큰화 → 줄 채움 → LineSeg 생성
    let mut tokens = tokenize_paragraph(
        &text_chars,
        &para.char_offsets,
        &para.char_shapes,
        styles,
        english_break_unit,
        korean_break_unit,
    );
    insert_object_tokens(&mut tokens, para);
    // [officex/어울림 본편] 밴드가 있으면 줄별 폭으로 줄바꿈 — 고정점 반복.
    // 줄 top(문단 상단 기준) = Σ(이전 줄 line_height+line_spacing). 폭이 줄 수를 바꾸면
    // top 이 밀리므로, 이전 반복의 높이 목록으로 폭 함수를 만들고 다시 줄바꿈한다.
    let line_breaks = if bands.is_empty() {
        fill_lines(
            &tokens,
            &text_chars,
            available_width_px,
            indent_px,
            tab_width,
            korean_break_unit,
            condense_min_space,
        )
    } else {
        let advance_px_of = |fs: f64| -> f64 {
            let fs = if fs > 0.0 { fs } else { 12.0 };
            let lh = font_size_to_line_height(fs, dpi);
            let sp = compute_line_spacing_hwp(ls_type, ls_value, lh, dpi);
            (lh + sp) as f64 / 7200.0 * dpi
        };
        // 시각적 한 줄이 차지하는 세그들: 보통 1개, 양쪽(BothSides) 밴드 줄은 2개
        // (왼쪽 세그 + 오른쪽 세그 — 같은 y 를 공유). fill 에는 각 세그가 "연속된
        // 좁은 줄"로 공급되고, 기록 단계(아래)가 같은 vertical_pos 로 묶는다.
        let segs_for_top = |top: f64, adv: f64| -> Vec<(f64, f64, bool)> {
            // 이 시각 줄과 겹치는 밴드를 **전부** 모은다(종전엔 첫 밴드에서 즉시 return 해
            // 표가 둘 이상 걸친 줄에서 둘째 표 위로 글이 덮였다 — 부록4 갭 #6).
            let mut hits: Vec<&ReflowBand> = bands
                .iter()
                .filter(|b| top + adv > b.top_px + 0.5 && top + 0.5 < b.bottom_px)
                .collect();
            if hits.is_empty() {
                return vec![(0.0, available_width_px, false)];
            }
            hits.sort_by(|a, b| {
                a.x0_px
                    .partial_cmp(&b.x0_px)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            // 겹침 밴드가 전부 "양쪽"이면 밴드 **사이·바깥**의 빈 구간을 모두 조각으로.
            // 밴드 1개면 종전과 같은 좌·우 2조각이 나온다(일반화이지 동작 변경이 아니다).
            if hits
                .iter()
                .all(|b| matches!(b.flow, crate::model::shape::TextFlow::BothSides))
            {
                let mut segs: Vec<(f64, f64, bool)> = Vec::new();
                let mut cursor = 0.0f64;
                for b in &hits {
                    let gap = b.x0_px.max(cursor) - cursor;
                    if gap >= MIN_SIDE_PX {
                        segs.push((cursor, gap, !segs.is_empty()));
                    }
                    cursor = cursor.max(b.x1_px);
                }
                let tail = (available_width_px - cursor).max(0.0);
                if tail >= MIN_SIDE_PX {
                    segs.push((cursor, tail, !segs.is_empty()));
                }
                if segs.len() >= 2 {
                    return segs;
                }
                if let Some(&(cs, w, _)) = segs.first() {
                    return vec![(cs, w, false)];
                }
                // 어느 구간도 하한을 못 넘었다 — 전폭(줄은 layout 이 밴드 아래로 민다)
                return vec![(0.0, available_width_px, false)];
            }

            // 그 외(왼쪽/오른쪽/큰 쪽 혼재)는 종전대로 첫 밴드 기준 단일 조각.
            let b = hits[0];
            match side_pick_for_band(available_width_px, b.x0_px, b.x1_px, b.flow) {
                Some((cs, w)) => vec![(cs, w, false)],
                None => vec![(0.0, available_width_px, false)],
            }
        };
        let mut breaks = fill_lines(
            &tokens,
            &text_chars,
            available_width_px,
            indent_px,
            tab_width,
            korean_break_unit,
            condense_min_space,
        );
        let mut final_plan: Vec<(f64, f64, bool)> = Vec::new();
        for _ in 0..4 {
            // 이전 결과(가상 줄 수) 기준으로 시각 줄 top 을 전진 계산하며 세그 계획 수립.
            // is_cont(세그 연속) 가상 줄은 y 를 전진시키지 않는다.
            let mut plan: Vec<(f64, f64, bool)> = Vec::new();
            let mut y = para_top_px;
            let mut consumed = 0usize;
            while consumed < breaks.len() + 4 {
                let fs = breaks
                    .get(consumed)
                    .map(|lb| lb.max_font_size)
                    .unwrap_or(12.0);
                let adv = advance_px_of(fs);
                let segs = segs_for_top(y, adv);
                let n = segs.len();
                plan.extend(segs);
                y += adv;
                consumed += n;
            }
            let widths: Vec<(f64, f64, bool)> = plan.clone();
            let next = fill_lines_per_line(
                &tokens,
                &text_chars,
                available_width_px,
                indent_px,
                tab_width,
                korean_break_unit,
                condense_min_space,
                Some(&|i: usize| widths.get(i).map(|(_, w, _)| *w)),
            );
            // 수렴 판정: 줄 수 + **세그 계획**이 모두 같아야 한다. 종전엔 줄 수만 봐서
            // 폭·시작 오프셋이 바뀌었는데 줄 수가 우연히 같으면 한 패스 뒤처진 계획으로
            // 확정됐다(부록4 갭 #6).
            let plan_same = final_plan.len() == plan.len()
                && final_plan.iter().zip(plan.iter()).all(|(a, b)| {
                    (a.0 - b.0).abs() < 0.01 && (a.1 - b.1).abs() < 0.01 && a.2 == b.2
                });
            let converged = next.len() == breaks.len() && plan_same;
            breaks = next;
            final_plan = plan;
            if converged {
                break;
            }
        }
        para.reflow_seg_plan = final_plan
            .iter()
            .take(breaks.len())
            .map(|(cs, w, cont)| (*cs, *w, *cont))
            .collect();
        breaks
    };
    let para_chars: Vec<char> = para.text.chars().collect();
    let mut new_line_segs: Vec<LineSeg> = Vec::new();
    for (lb_i, lb) in line_breaks.iter().enumerate() {
        let utf16_start = if new_line_segs.is_empty() {
            0 // 첫 번째 줄의 text_start는 항상 0 (문단 시작)
        } else if lb.start_idx < para.char_offsets.len() {
            para.char_offsets[lb.start_idx]
        } else if !para.char_offsets.is_empty() {
            // start_idx가 텍스트 끝을 넘을 때: 마지막 문자 다음 UTF-16 위치
            let last_idx = para.char_offsets.len() - 1;
            let last_char_utf16_len = para
                .text
                .chars()
                .nth(last_idx)
                .map(|c| c.len_utf16() as u32)
                .unwrap_or(1);
            para.char_offsets[last_idx] + last_char_utf16_len
        } else {
            lb.start_idx as u32
        };
        let fs = if lb.max_font_size > 0.0 {
            lb.max_font_size
        } else {
            12.0
        };
        new_line_segs.push(make_line_seg(utf16_start as u32, fs));
        // [이모지 줄박스] 이 줄의 문자 범위에 이모지가 있으면 상자를 글리프에 맞춘다
        let line_end = line_breaks
            .get(lb_i + 1)
            .map(|n| n.start_idx)
            .unwrap_or(para_chars.len())
            .min(para_chars.len());
        let line_start = lb.start_idx.min(line_end);
        if para_chars[line_start..line_end]
            .iter()
            .any(|&c| crate::renderer::layout::text_measurement::is_emoji_presentation(c))
        {
            if let Some(seg) = new_line_segs.last_mut() {
                apply_emoji_line_height(seg, fs, dpi);
            }
        }
    }

    if new_line_segs.is_empty() {
        new_line_segs.push(make_line_seg(0, 12.0));
    }

    // 인라인 TAC 개체의 높이 반영: 개체가 포함된 줄의 line_height를 개체 높이 이상으로 보정.
    // 개체 높이는 **개체가 실제로 놓인 줄**에 얹는다 — 줄 배정은 위 폭 기준 줄바꿈
    // (Object 토큰)이 이미 확정했다. 폭이 0이라 토큰이 안 만들어진 문단(폭 미상 개체)만
    // 종전대로 첫 줄에 얹는다.
    //
    // [oracle-pdf-mining-20260806 §1-B/§1-C/§2-B] end-anchor 표 전용 "자기 줄" seg 생산은
    // 삭제됐다: 저장 코퍼스 전수와 한컴 인쇄 PDF 실측 모두 표가 앞 텍스트와 **같은 줄**에
    // 있고(seg 1개), 자기 줄이 생기는 표본은 100% 폭 초과·강제 줄바꿈의 귀결이다.
    // 그 블록이 쓰던 `baseline = 바깥여백상 + 표높이`(≈0.99·lh) 도 PDF 실측으로 반증됐다
    // (표 바닥이 기준선보다 30.57pt 아래 = (1−0.85)·lh − 바깥여백하).
    {
        let mut applied = false;
        for (i, lb) in line_breaks.iter().enumerate() {
            if lb.object_height_hwp > 0 {
                if let Some(seg) = new_line_segs.get_mut(i) {
                    apply_inline_control_line_height(seg, lb.object_height_hwp, baseline_ratio);
                    applied = true;
                }
            }
        }
        if !applied {
            if let Some(height_hwp) = inline_control_line_height_hwp(para) {
                if let Some(seg) = new_line_segs.first_mut() {
                    apply_inline_control_line_height(seg, height_hwp, baseline_ratio);
                }
            }
        }
    }

    // vertical_pos 누적 계산 (각 줄의 문단 내 Y 오프셋)
    // 원본 첫 LineSeg의 vertical_pos를 보존하여 vpos 체계 연속성 유지
    // (layout.rs의 vpos 보정이 문단 간 vpos 연속성을 가정하므로)
    let vpos_start = orig.as_ref().map(|ls| ls.vertical_pos).unwrap_or(0);
    let mut vpos = vpos_start;
    for i in 0..new_line_segs.len() {
        new_line_segs[i].vertical_pos = vpos;
        vpos += new_line_segs[i].line_height + new_line_segs[i].line_spacing;
    }

    // [officex/어울림 본편] 세그 계획(reflow_seg_plan)을 line_segs 에 기록 — 줄별
    // column_start/segment_width + 양쪽 세그는 **같은 vertical_pos** 로 묶는다(한 줄).
    // 한컴 저장 형식 그대로라 렌더의 재생 소비(줄 폭·x·같은 y)가 그대로 먹는다.
    if !bands.is_empty() && !para.reflow_seg_plan.is_empty() {
        let px_to_hu = |px: f64| (px * 7200.0 / dpi) as i32;
        let plan = std::mem::take(&mut para.reflow_seg_plan);
        // vpos 재누적: cont 세그는 이전 세그와 같은 vpos, advance 는 시각 줄당 1회.
        let mut vpos = vpos_start;
        for (i, seg) in new_line_segs.iter_mut().enumerate() {
            let (cs, w, cont) = plan
                .get(i)
                .copied()
                .unwrap_or((0.0, available_width_px, false));
            if cs > 0.5 || w < available_width_px - 0.5 {
                seg.column_start = px_to_hu(cs);
                seg.segment_width = px_to_hu(w);
            }
            if cont {
                // 같은 시각 줄의 다음 세그 — y 공유, 누적 없음
                seg.vertical_pos = vpos - (seg.line_height + seg.line_spacing).max(0);
                // 위 식은 직전 누적을 되돌린 값 — 아래 일반식과 함께 정리된다
            }
            let _ = seg;
        }
        // 누적을 처음부터 다시: cont 세그는 이전 vpos 복사, 아니면 누적 후 진행.
        // 아울러 **세그 태그를 한컴 인코딩으로** 맞춘다(2026-07-30 samples/pic2.hwpx 실측):
        // 한 줄이 2세그면 좌=FIRST-only(0x20000) · 우=LAST-only(0x40000). 종전엔 둘 다
        // 원본 태그(보통 SINGLE=FIRST|LAST=0x60000)를 복사해 "단일 세그 줄"이라 거짓
        // 저장했다 — HWPX/HWP5 왕복에서 한컴이 줄 구조를 오해할 수 있다(부록4 갭 #3).
        let mut vpos2 = vpos_start;
        let mut prev_vpos = vpos_start;
        for (i, seg) in new_line_segs.iter_mut().enumerate() {
            let cont = plan.get(i).map(|p| p.2).unwrap_or(false);
            let next_is_cont = plan.get(i + 1).map(|p| p.2).unwrap_or(false);
            if cont {
                seg.vertical_pos = prev_vpos;
                // 줄의 마지막 세그 — FIRST 를 떼고 LAST 만 남긴다
                seg.tag = (seg.tag & !LineSeg::TAG_FIRST_SEGMENT) | LineSeg::TAG_LAST_SEGMENT;
            } else {
                seg.vertical_pos = vpos2;
                prev_vpos = vpos2;
                vpos2 += seg.line_height + seg.line_spacing;
                if next_is_cont {
                    // 줄의 첫 세그이고 뒤에 짝이 있다 — LAST 를 떼고 FIRST 만
                    seg.tag = (seg.tag & !LineSeg::TAG_LAST_SEGMENT) | LineSeg::TAG_FIRST_SEGMENT;
                }
            }
        }
        vpos = vpos2;
        let _ = vpos;
    } else {
        para.reflow_seg_plan.clear();
    }

    para.line_segs = new_line_segs;
}

/// 구역 내 문단들의 vertical_pos를 순차적으로 재계산한다.
///
/// `start_para`부터 구역 끝까지 각 문단의 vpos를 이전 문단의 vpos_end 기준으로 재계산.
/// 표 등 특수 문단의 line_height는 보존하고 vpos만 갱신한다.
///
/// [Task #2299] 저장 vpos 리셋(단/쪽 경계 인코딩) 보존: 편집발 재계산이 구역 전체를
/// 선형 누적 좌표로 이어붙이면 다단 zone 의 단-상대 리셋(급감)이 소멸해
/// typeset(#321/#470/#702)·pagination 의 단/쪽 진행 신호가 무력화된다
/// (shortcut.hwp 앞문단 편집 시 col=[0,1]→[0], 7→9쪽). 현재 문단의 저장 first 가
/// 직전 문단의 "이동 전(저장)" end 보다 감소하면 경계 인코딩으로 보고 delta=0 으로
/// 보존한다. 저장 좌표는 밴드 내 정상 흐름에서 단조 증가하므로 감소 감지에 임계가
/// 필요 없다.
///
/// 좌표 갱신은 경계 성격별로 셋으로 나뉜다.
///
/// - **리셋 경계**: delta=0 보존.
/// - **변조 인접 경계**(현재 문단이 편집 대상 `start_para` 이거나 신규
///   문단(`ignore_reset_range`)이거나, 직전 문단이 그중 하나): 직전 이동 후 end 에
///   문단 여백 gap(spacing_after + spacing_before, 셀 recalc `boundary_gaps` 동일
///   산식)을 더해 다시 잇는다. reflow/신규 생성으로 저장 gap 이 소실된 경계라
///   스타일에서 재유도한다. gap 없는 abutment 는 문단 간격을 압축해 near-top
///   리셋(#1086/#1921)의 `prev_vpos_end > 60000` 임계를 무너뜨렸다
///   (SO-SUEOP.hwpx 46→44).
/// - **미변조 연속 경계**: 직전 문단의 delta 를 그대로 캐리해 저장(또는 로드 합성
///   #927) 문단 간격을 정확히 보존한다. 스타일 gap 재유도는 저장 gap 과의
///   오차(px 왕복 절삭 ±1HU, 스타일-저장 불일치)를 밴드 전체에 누적시키고 로드
///   합성 gap-less 체인과도 어긋나므로 쓰지 않는다. delta==0 이면 순수 no-op.
///
/// 리셋 감지는 저장 좌표끼리의 비교여야 한다. 직전 문단이 변조 대상이면 그 end 는
/// 저장 좌표가 아니므로(성장 편집이 다음 문단을 가짜 리셋으로 동결시키고,
/// placeholder 는 기준을 붕괴시킨다) reflow 가 보존하는 **first** 로 비교한다.
/// 미변조 경계는 end 기준을 유지한다(연속 0-first 밴드 감지에 필요).
///
/// placeholder 저지선 2종: ① split/insert/paste 가 방금 만든 신규 문단의 vpos=0 은
/// 경계 인코딩이 아니다 — 보존하면 문단마다 가짜 쪽나눔이 생긴다
/// (test_page_boundary_with_incremental_spacing_increase 핀). 호출자가 신규 구간을
/// `ignore_reset_range` 로 지정하면 보존 없이 흐름에 연결한다(셀 경로
/// `recalculate_cell_paragraph_vpos` 의 ignore_reset_at 과 동일 취지, 다중 삽입을
/// 위해 범위형). ② lineseg 부재였다가 on-demand reflow(#177/#927)로 합성된
/// seg(TAG_IMPLEMENTATION_PROPERTY, #1811)도 보존하지 않는다.
///
/// 줄 전진량은 로드 경로(document.rs 의 vpos 체인)와 동일하게 TAC 호스트
/// 줄(lh>th)을 th 기준으로 센다 — lh 기준이면 인라인 개체 호스트의 end 가 저장
/// 후속 first 를 넘어서 가짜 리셋을 만든다.
pub(crate) fn recalculate_section_vpos(
    paragraphs: &mut [Paragraph],
    start_para: usize,
    ignore_reset_range: Option<std::ops::Range<usize>>,
    start_stored_end: Option<i32>,
    styles: &ResolvedStyleSet,
    dpi: f64,
    is_hwp3_variant: bool,
) {
    if paragraphs.is_empty() || start_para >= paragraphs.len() {
        return;
    }

    // 문단 경계 gap (HWPUNIT) = 앞 문단 spacing_after + 뒤 문단 spacing_before.
    // recalculate_cell_paragraph_vpos 의 boundary_gaps 와 동일 산식.
    let boundary_gap = |prev: &Paragraph, curr: &Paragraph| -> i32 {
        let spacing_after = styles
            .para_styles
            .get(prev.para_shape_id as usize)
            .map(|style| style.spacing_after)
            .unwrap_or(0.0);
        let spacing_before = styles
            .para_styles
            .get(curr.para_shape_id as usize)
            .map(|style| style.spacing_before)
            .unwrap_or(0.0);
        let spacing_before =
            crate::renderer::hwp3_variant_flow_spacing_before(spacing_before, is_hwp3_variant);
        px_to_hwpunit(spacing_after + spacing_before, dpi)
    };

    // 줄 전진량 — 로드 경로와 동일한 TAC th-관례. saturating: 조작 파일의 극단
    // spacing/좌표로 i32 가 넘치지 않게 한다 (release wasm 은 overflow-check 가
    // 없어 무음 랩 → 전 문단 오판으로 이어진다).
    let seg_advance = |ls: &LineSeg| -> i32 {
        let height = if ls.line_height > ls.text_height && ls.text_height > 0 {
            ls.text_height
        } else {
            ls.line_height
        };
        height.saturating_add(ls.line_spacing)
    };
    let seg_end = |p: &Paragraph| -> Option<i32> {
        p.line_segs
            .last()
            .map(|ls| ls.vertical_pos.saturating_add(seg_advance(ls)))
    };
    let is_ignored = |pi: usize| {
        ignore_reset_range
            .as_ref()
            .is_some_and(|range| range.contains(&pi))
    };

    // 직전 문단(마지막 비어있지 않은 lineseg 보유 문단) 인덱스.
    // start_para 이전 문단들은 이 호출에서 이동하지 않으므로 현재 좌표가 곧 저장 좌표다.
    let mut prev_idx: Option<usize> = paragraphs[..start_para]
        .iter()
        .rposition(|p| !p.line_segs.is_empty());
    let mut next_vpos = match prev_idx {
        Some(pp) => seg_end(&paragraphs[pp]).unwrap_or(0),
        // 첫 문단: 기존 vpos 유지
        None => paragraphs[start_para]
            .line_segs
            .first()
            .map(|ls| ls.vertical_pos)
            .unwrap_or(0),
    };
    // 리셋 감지 기준 — 직전 문단의 "이동 전(저장)" first/end.
    let mut orig_prev_first: Option<i32> = prev_idx
        .and_then(|pp| paragraphs[pp].line_segs.first())
        .map(|ls| ls.vertical_pos);
    let mut orig_prev_end: Option<i32> = prev_idx.and_then(|pp| seg_end(&paragraphs[pp]));
    // 직전 문단이 이번 편집의 변조 대상이었는가 + 직전 문단에 적용된 delta.
    let mut prev_modified = false;
    let mut prev_delta: i32 = 0;

    for pi in start_para..paragraphs.len() {
        if paragraphs[pi].line_segs.is_empty() {
            continue;
        }

        let para_modified = pi == start_para || is_ignored(pi);
        let current_start = paragraphs[pi].line_segs[0].vertical_pos;
        let is_original_lineseg =
            paragraphs[pi].line_segs[0].tag & LineSeg::TAG_IMPLEMENTATION_PROPERTY == 0;

        // 리셋 감지: 신규 문단(placeholder)·합성 seg 는 제외. 기준은 직전 문단의
        // "저장" 좌표여야 한다 — 직전이 편집 문단(start_para)이면 reflow 로 end 가
        // 이미 변조됐으므로 호출자가 캡처해 준 reflow 이전 저장 end 를 쓰고(성장
        // 편집의 가짜 리셋과 저장-겹침 문서의 정당한 리셋을 모두 정확히 판별),
        // 없으면 reflow 가 보존하는 first 로 보수적으로 비교한다. 신규 문단이
        // 직전이면 placeholder 라 first(=0) 기준. 미변조 경계는 end 기준을
        // 유지한다(연속 0-first 밴드 감지에 필요).
        let prev_stored_bound = if prev_idx == Some(start_para) && !is_ignored(start_para) {
            start_stored_end.or(orig_prev_first)
        } else if prev_modified {
            orig_prev_first
        } else {
            orig_prev_end
        };
        let is_reset = is_original_lineseg
            && !is_ignored(pi)
            && prev_stored_bound.is_some_and(|bound| current_start < bound);

        let delta = if is_reset {
            // 단/쪽 리셋 경계 — 저장 좌표 유지.
            0
        } else if para_modified || prev_modified {
            // 변조 인접 경계 — 이동 후 흐름에 스타일 여백 gap 으로 다시 잇는다.
            let gap = prev_idx
                .map(|pp| boundary_gap(&paragraphs[pp], &paragraphs[pi]))
                .unwrap_or(0);
            next_vpos.saturating_add(gap) - current_start
        } else {
            // 미변조 연속 경계 — 직전 delta 캐리로 기존 간격을 정확히 보존.
            prev_delta
        };

        // 다음 문단의 리셋 감지 기준은 "이동 전(저장)" first/end 로 기록한다.
        let orig_first = current_start;
        let orig_end = seg_end(&paragraphs[pi]);

        if delta != 0 {
            // 모든 LineSeg의 vpos를 delta만큼 이동
            for seg in &mut paragraphs[pi].line_segs {
                seg.vertical_pos = seg.vertical_pos.saturating_add(delta);
            }
        }

        // 다음 문단의 시작 vpos 계산 (이동 후 end = 저장 end + delta)
        if let Some(end) = orig_end {
            next_vpos = end.saturating_add(delta);
        }
        orig_prev_first = Some(orig_first);
        orig_prev_end = orig_end;
        prev_modified = para_modified;
        prev_delta = delta;
        prev_idx = Some(pi);
    }
}

/// [Task #2299] 문단의 흐름 end (마지막 LineSeg 의 vpos + 전진량, TAC th-관례).
/// 편집 호출자가 reflow 이전에 캡처해 `recalculate_section_vpos` 의
/// `start_stored_end` 로 전달하기 위한 헬퍼 — reflow 가 end 를 덮은 뒤에는 저장
/// 좌표를 복원할 수 없다.
pub(crate) fn paragraph_flow_end(para: &Paragraph) -> Option<i32> {
    para.line_segs.last().map(|ls| {
        let height = if ls.line_height > ls.text_height && ls.text_height > 0 {
            ls.text_height
        } else {
            ls.line_height
        };
        ls.vertical_pos
            .saturating_add(height.saturating_add(ls.line_spacing))
    })
}

/// font_size(px)를 LineSeg의 line_height(HWPUNIT)로 변환한다.
/// HWP의 LineSeg.line_height = 폰트 크기 (HWPUNIT).
/// 실증 데이터: 10pt → lh=1000, 12pt → lh=1200, 25pt → lh=2500
fn font_size_to_line_height(font_size_px: f64, dpi: f64) -> i32 {
    px_to_hwpunit(font_size_px, dpi)
}

/// ParaPr의 줄간격 설정으로부터 LineSeg.line_spacing(HWPUNIT)을 계산한다.
///
/// line_spacing = 현재 줄 하단 → 다음 줄 상단 사이의 추가 간격.
/// Y advance = line_height + line_spacing.
fn compute_line_spacing_hwp(
    ls_type: LineSpacingType,
    ls_value: f64,
    line_height_hwp: i32,
    dpi: f64,
) -> i32 {
    match ls_type {
        LineSpacingType::Percent => {
            // ls_value = 비율값 (예: 160 = 160%)
            // 전체 줄 피치 = line_height * percent / 100
            // line_spacing = 전체 줄 피치 - line_height
            (line_height_hwp as f64 * (ls_value - 100.0) / 100.0).max(0.0) as i32
        }
        LineSpacingType::Fixed => {
            // ls_value = 고정 줄 피치 (px, resolver가 HWPUNIT→px 변환 완료)
            // line_spacing = 고정값 - line_height
            let fixed_hwp = px_to_hwpunit(ls_value, dpi);
            (fixed_hwp - line_height_hwp).max(0)
        }
        LineSpacingType::SpaceOnly => {
            // ls_value = 줄 사이 추가 간격만 (px)
            px_to_hwpunit(ls_value, dpi)
        }
        LineSpacingType::Minimum => {
            // 최소값: 콘텐츠가 최소값보다 크면 추가 간격 없음
            let min_hwp = px_to_hwpunit(ls_value, dpi);
            (min_hwp - line_height_hwp).max(0)
        }
    }
}
