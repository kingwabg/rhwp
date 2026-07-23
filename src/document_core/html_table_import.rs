//! HTML 표 파싱 + BorderFill 생성 + 이미지 파싱 관련 native 메서드

use super::helpers::*;
use crate::document_core::DocumentCore;
use crate::error::HwpError;
use crate::model::control::Control;
use crate::model::paragraph::Paragraph;
use crate::model::shape::common_obj_offsets;
use crate::renderer::style_resolver::resolve_styles;

impl DocumentCore {
    pub(crate) fn parse_table_html(&mut self, paragraphs: &mut Vec<Paragraph>, table_html: &str) {
        use crate::model::control::Control;
        use crate::model::table::{Cell, Table, TablePageBreak};

        // --- 1. HTML 파싱: 행/셀 구조 추출 ---
        let table_lower = table_html.to_lowercase();

        #[derive(Default)]
        struct ParsedCell {
            col_span: u16,
            row_span: u16,
            width_pt: f64,
            width_pct: f64, // [paste-import/폭%] 퍼센트 폭(0=미지정) — 표 폭 기준 환산
            height_pt: f64,
            padding_pt: [f64; 4],          // left, right, top, bottom
            has_explicit_padding: bool,    // [paste-import/여백] CSS로 padding을 준 셀인지
            border_widths_pt: [f64; 4],    // left, right, top, bottom
            border_colors: [u32; 4],       // BGR
            border_styles: [u8; 4],        // 0=none, 1=solid, 2=dashed, 3=dotted, 4=double
            background_color: Option<u32>, // BGR
            // [paste-import/셀서식] td의 char/para CSS(정렬·굵기·색·크기·글꼴 등) — 셀 문단
            // 서식으로 옮긴다. 예전엔 셀 레벨 속성(테두리·배경·폭)만 읽고 글자·정렬 서식은 버렸다.
            cell_css: String,
            content_html: String,
            is_header: bool,
            is_filler: bool,    // [paste-import/ragged] 직사각형 채움용 빈 셀
            vertical_align: u8, // 0=top, 1=center, 2=bottom
        }

        let mut parsed_rows: Vec<Vec<ParsedCell>> = Vec::new();

        let mut pos = 0;
        while let Some(tr_start) = table_lower[pos..].find("<tr") {
            let tr_abs = pos + tr_start;
            let tr_end = find_closing_tag(table_html, tr_abs, "tr");
            let tr_inner = &table_html[tr_abs..tr_end.min(table_html.len())];
            let tr_inner_lower = tr_inner.to_lowercase();

            let mut row_cells: Vec<ParsedCell> = Vec::new();

            // <td>와 <th>를 출현 순서대로 처리
            let mut td_pos = 0;
            loop {
                // 불완전 HTML 방어: </td>/</th>를 못 찾으면 td_pos가 문자열 길이를 넘어
                // tr_inner_lower[td_pos..] 슬라이싱이 panic(unreachable trap)한다 — 안 닫힌
                // <td>/<table>·중첩 표 같은 외부 클립보드 입력에서 실측. panic 대신 지금까지
                // 수집한 셀로 degrade한다(NaN hitTest 가드와 같은 철학: 위험 연산 직전 방어).
                if td_pos >= tr_inner_lower.len() {
                    break;
                }
                let td_match = tr_inner_lower[td_pos..].find("<td");
                let th_match = tr_inner_lower[td_pos..].find("<th");

                let (tag_offset, is_th) = match (td_match, th_match) {
                    (Some(a), Some(b)) => {
                        if a <= b {
                            (a, false)
                        } else {
                            (b, true)
                        }
                    }
                    (Some(a), None) => (a, false),
                    (None, Some(b)) => (b, true),
                    (None, None) => break,
                };

                let cell_abs = td_pos + tag_offset;
                let tag_name = if is_th { "th" } else { "td" };

                // <td ...> 태그에서 속성 추출
                if let Some(gt) = tr_inner[cell_abs..].find('>') {
                    let tag_str = &tr_inner[cell_abs..cell_abs + gt + 1];

                    // colspan / rowspan 파싱
                    let col_span = parse_html_attr_u16(tag_str, "colspan").unwrap_or(1).max(1);
                    let row_span = parse_html_attr_u16(tag_str, "rowspan").unwrap_or(1).max(1);

                    // 인라인 style 파싱
                    let css = parse_inline_style(tag_str);
                    let css_lower = css.to_lowercase();

                    // 크기 파싱
                    let mut width_pt = parse_css_dimension_pt(&css_lower, "width");
                    // [paste-import/폭%] parse_css_dimension_pt는 %를 0으로 버린다 —
                    // 결재 양식이 폭을 %로만 준다. 원본 %값을 따로 붙잡아 표 폭 기준으로 환산.
                    let mut width_pct = parse_css_value(&css_lower, "width")
                        .and_then(|v| v.trim().strip_suffix('%').and_then(|n| n.trim().parse::<f64>().ok()))
                        .unwrap_or(0.0);
                    let height_pt = parse_css_dimension_pt(&css_lower, "height");

                    // 패딩 파싱
                    let padding_pt = parse_css_padding_pt(&css_lower);
                    let mut has_explicit_padding = padding_pt.iter().any(|&p| p > 0.01);

                    // [paste-import/html4] style= 이 없을 때 HTML4 표현 속성으로 폴백
                    if width_pt <= 0.0 && width_pct <= 0.0 {
                        if let Some(w) = parse_html_attr_str(tag_str, "width") {
                            let w = w.trim();
                            if let Some(pct) = w.strip_suffix('%').and_then(|n| n.trim().parse::<f64>().ok())
                            {
                                width_pct = pct;
                            } else if let Some(px) = w.strip_suffix("px").unwrap_or(w).trim().parse::<f64>().ok()
                            {
                                width_pt = px * 0.75; // HTML width 속성은 px
                            }
                        }
                    }

                    // 테두리 파싱 (left, right, top, bottom)
                    let mut border_widths_pt = [0.0f64; 4];
                    let mut border_colors = [0u32; 4]; // black
                    let mut border_styles = [1u8; 4]; // solid default

                    // 축약형 border 먼저
                    if let Some(bval) = parse_css_value(&css_lower, "border") {
                        let (w, c, s) = parse_css_border_shorthand(&bval);
                        for i in 0..4 {
                            border_widths_pt[i] = w;
                            border_colors[i] = c;
                            border_styles[i] = s;
                        }
                    }
                    // 개별 방향 오버라이드
                    let sides = ["border-left", "border-right", "border-top", "border-bottom"];
                    for (i, side) in sides.iter().enumerate() {
                        if let Some(bval) = parse_css_value(&css_lower, side) {
                            let (w, c, s) = parse_css_border_shorthand(&bval);
                            border_widths_pt[i] = w;
                            border_colors[i] = c;
                            border_styles[i] = s;
                        }
                    }

                    // 배경색 (CSS 우선, 없으면 HTML4 bgcolor 속성 폴백)
                    let background_color = parse_css_value(&css_lower, "background-color")
                        .or_else(|| parse_css_value(&css_lower, "background"))
                        .and_then(|v| css_color_to_hwp_bgr(&v))
                        .or_else(|| {
                            parse_html_attr_str(tag_str, "bgcolor")
                                .and_then(|v| css_color_to_hwp_bgr(&v))
                        });

                    // 수직 정렬 (0=미지정, 1=center, 2=bottom, 3=명시적 top)
                    let vertical_align =
                        match parse_css_value(&css_lower, "vertical-align").as_deref() {
                            Some("middle") | Some("center") => 1u8,
                            Some("bottom") => 2u8,
                            Some("top") => 3u8, // 명시적 top
                            _ => 0u8,           // 미지정 → Center (HWP 기본)
                        };

                    // [paste-import/셀서식] 셀 문단에 옮길 char/para CSS를 조립한다.
                    // HTML4 align 속성은 CSS text-align 이 없을 때만 합성한다.
                    let mut cell_css = css.clone();
                    if parse_css_value(&css_lower, "text-align").is_none() {
                        if let Some(align) = parse_html_attr_str(tag_str, "align") {
                            let a = align.trim().to_lowercase();
                            if matches!(a.as_str(), "left" | "right" | "center" | "justify") {
                                if !cell_css.is_empty() && !cell_css.trim_end().ends_with(';') {
                                    cell_css.push(';');
                                }
                                cell_css.push_str("text-align:");
                                cell_css.push_str(&a);
                            }
                        }
                    }

                    // 셀 내용 HTML 추출
                    let content_start = cell_abs + gt + 1;
                    let close_tag = format!("</{}>", tag_name);
                    let content_end =
                        if let Some(close) = tr_inner_lower[content_start..].find(&close_tag) {
                            content_start + close
                        } else {
                            tr_inner.len()
                        };
                    let content_html = tr_inner[content_start..content_end].to_string();

                    row_cells.push(ParsedCell {
                        col_span,
                        row_span,
                        width_pt,
                        width_pct,
                        height_pt,
                        padding_pt,
                        has_explicit_padding,
                        border_widths_pt,
                        border_colors,
                        border_styles,
                        background_color,
                        cell_css,
                        content_html,
                        is_header: is_th,
                        is_filler: false,
                        vertical_align,
                    });

                    // close 태그를 못 찾은 폴백(content_end=len)에서 len을 넘어서지 않도록 클램프.
                    td_pos = (content_end + close_tag.len()).min(tr_inner_lower.len());
                } else {
                    break;
                }
            }

            if !row_cells.is_empty() {
                parsed_rows.push(row_cells);
            }
            pos = tr_end;
        }

        if parsed_rows.is_empty() {
            return;
        }

        // --- 2. 그리드 정규화: 실제 col 인덱스 계산 ---
        let row_count = parsed_rows.len() as u16;
        // colspan 합산으로 최대 열 수 추정
        let mut max_cols: usize = 0;
        for row in &parsed_rows {
            let sum: usize = row.iter().map(|c| c.col_span as usize).sum();
            if sum > max_cols {
                max_cols = sum;
            }
        }
        max_cols = max_cols.max(1);
        // rowspan 처리를 위한 점유 그리드
        let grid_rows = row_count as usize + 16;
        let grid_cols = max_cols + 16;
        let mut occupied = vec![vec![false; grid_cols]; grid_rows];

        struct CellPos {
            row: u16,
            col: u16,
            col_span: u16,
            row_span: u16,
            parsed_row: usize,
            parsed_col: usize,
        }

        let mut cell_positions: Vec<CellPos> = Vec::new();
        let mut actual_col_count: u16 = 0;

        for (ri, row) in parsed_rows.iter().enumerate() {
            let mut col_cursor: usize = 0;
            for (ci, cell) in row.iter().enumerate() {
                // 이미 점유된 위치 건너뛰기
                while col_cursor < grid_cols && occupied[ri][col_cursor] {
                    col_cursor += 1;
                }
                let col = col_cursor as u16;

                // 점유 표시
                for dr in 0..cell.row_span as usize {
                    for dc in 0..cell.col_span as usize {
                        let r = ri + dr;
                        let c = col_cursor + dc;
                        if r < grid_rows && c < grid_cols {
                            occupied[r][c] = true;
                        }
                    }
                }

                cell_positions.push(CellPos {
                    row: ri as u16,
                    col,
                    col_span: cell.col_span,
                    row_span: cell.row_span,
                    parsed_row: ri,
                    parsed_col: ci,
                });

                let end_col = col + cell.col_span;
                if end_col > actual_col_count {
                    actual_col_count = end_col;
                }
                col_cursor += cell.col_span as usize;
            }
        }

        let col_count = actual_col_count.max(1);

        // [paste-import/ragged] 행마다 열 수가 다른 표(ragged)나 과한 colspan은 격자에
        // 구멍을 남겨 직사각형이 아닌 표가 된다 — 한글에서 표가 손상돼 보인다. 점유되지
        // 않은 [row×col] 칸을 1x1 빈 셀로 메워 항상 직사각형이 되게 한다.
        for r in 0..row_count as usize {
            for c in 0..col_count as usize {
                if !occupied[r][c] {
                    occupied[r][c] = true;
                    let filler_col = parsed_rows[r].len();
                    parsed_rows[r].push(ParsedCell {
                        col_span: 1,
                        row_span: 1,
                        is_filler: true,
                        ..Default::default()
                    });
                    cell_positions.push(CellPos {
                        row: r as u16,
                        col: c as u16,
                        col_span: 1,
                        row_span: 1,
                        parsed_row: r,
                        parsed_col: filler_col,
                    });
                }
            }
        }

        // --- 3. 셀 크기 계산 ---
        let default_page_width: u32 = 42520; // A4 좌우 여백 제외
        let default_col_width = default_page_width / col_count as u32;
        let default_row_height: u32 = 1000;

        // [paste-import/폭%] 퍼센트 폭 환산의 기준 폭 — 표 자체 폭(CSS px/pt 또는 HTML4
        // width 속성)을 쓰고, 없으면 본문 폭(default_page_width). 결재 양식은 표 width:100%
        // + 셀 width:14%/86% 로만 폭을 준다.
        let table_open_tag = &table_html[..table_html
            .find('>')
            .map(|i| i + 1)
            .unwrap_or(table_html.len())];
        let table_open_style = parse_inline_style(table_open_tag).to_lowercase();
        let table_width_pt = parse_css_dimension_pt(&table_open_style, "width");
        let table_target_width: u32 = if table_width_pt > 0.0 {
            (table_width_pt * 100.0).round() as u32
        } else {
            parse_html_attr_str(table_open_tag, "width")
                .and_then(|w| {
                    let w = w.trim();
                    w.strip_suffix("px")
                        .unwrap_or(w)
                        .trim()
                        .parse::<f64>()
                        .ok()
                        .map(|px| (px * 0.75 * 100.0).round() as u32)
                })
                .filter(|&w| w > 0)
                .unwrap_or(default_page_width)
        };

        // 열별 폭 (CSS px/% 지정 우선, 없으면 균등 분할)
        let mut col_widths = vec![0u32; col_count as usize];
        for cp in &cell_positions {
            if cp.col_span == 1 {
                let pc = &parsed_rows[cp.parsed_row][cp.parsed_col];
                let w = if pc.width_pct > 0.0 {
                    ((pc.width_pct / 100.0) * table_target_width as f64).round() as u32
                } else if pc.width_pt > 0.0 {
                    (pc.width_pt * 100.0).round() as u32
                } else {
                    0
                };
                if w > col_widths[cp.col as usize] {
                    col_widths[cp.col as usize] = w;
                }
            }
        }
        for w in col_widths.iter_mut() {
            if *w == 0 {
                *w = default_col_width;
            }
        }

        // 행별 높이
        let mut row_heights = vec![0u32; row_count as usize];
        for cp in &cell_positions {
            if cp.row_span == 1 {
                let pc = &parsed_rows[cp.parsed_row][cp.parsed_col];
                if pc.height_pt > 0.0 {
                    let h = (pc.height_pt * 100.0).round() as u32;
                    if h > row_heights[cp.row as usize] {
                        row_heights[cp.row as usize] = h;
                    }
                }
            }
        }
        for h in row_heights.iter_mut() {
            if *h == 0 {
                *h = default_row_height;
            }
        }

        // --- 4. BorderFill 생성 및 Cell 구조체 조립 ---
        let mut cells: Vec<Cell> = Vec::new();
        let mut has_header_row = false;

        for cp in &cell_positions {
            let pc = &parsed_rows[cp.parsed_row][cp.parsed_col];

            // 셀 폭/높이 (병합 고려)
            let cell_width: u32 = (cp.col..cp.col + cp.col_span)
                .map(|c| {
                    col_widths
                        .get(c as usize)
                        .copied()
                        .unwrap_or(default_col_width)
                })
                .sum();
            let cell_height: u32 = (cp.row..cp.row + cp.row_span)
                .map(|r| {
                    row_heights
                        .get(r as usize)
                        .copied()
                        .unwrap_or(default_row_height)
                })
                .sum();

            // BorderFill 생성/재사용
            let border_fill_id = self.create_border_fill_from_css(
                &pc.border_widths_pt,
                &pc.border_colors,
                &pc.border_styles,
                pc.background_color,
            );

            // 패딩 (pt → HWPUNIT16, CSS 미지정 시 기본 1.4mm ≈ 397 HWPUNIT)
            let default_pad: f64 = 141.0; // ~0.5mm HWPUNIT
            let padding = crate::model::Padding {
                left: if pc.padding_pt[0] > 0.01 {
                    (pc.padding_pt[0] * 100.0).round() as i16
                } else {
                    default_pad as i16
                },
                right: if pc.padding_pt[1] > 0.01 {
                    (pc.padding_pt[1] * 100.0).round() as i16
                } else {
                    default_pad as i16
                },
                top: if pc.padding_pt[2] > 0.01 {
                    (pc.padding_pt[2] * 100.0).round() as i16
                } else {
                    default_pad as i16
                },
                bottom: if pc.padding_pt[3] > 0.01 {
                    (pc.padding_pt[3] * 100.0).round() as i16
                } else {
                    default_pad as i16
                },
            };

            // [approval 워크어라운드 계약] 셀 글자/문단 서식(굵기·크기·색·정렬·줄간격)은 엔진이
            // 보존하지 않는다 — 앱(lib/features/approval-document/hwp-cell-format.ts의
            // restoreApprovalCellFormat)이 붙여넣기 뒤에 복원한다. 여기서 CSS 서식을 입히면
            // 앱 복원과 충돌해 라벨 정렬·별표 색이 어긋난다(approval-document-template.test 회귀).
            // 셀 레벨 속성(배경·테두리·폭%·padding·정렬 vertical)은 아래에서 그대로 반영한다.
            let cell_char_shape_id = 0;
            let cell_para_shape_id = 0;

            // 셀 내용 파싱
            // &nbsp; 등 HTML 엔티티를 디코딩한 후 공백만 남으면 빈 셀로 처리
            let cell_paragraphs = if pc.content_html.trim().is_empty()
                || html_to_plain_text(&pc.content_html).is_empty()
            {
                vec![Paragraph::new_empty()]
            } else {
                // 셀 문단 구조 파서(원래 동작). 셀 안 글자 서식은 앱 복원 패스가 맡는다.
                let parsed = self.parse_html_to_paragraphs(&pc.content_html);
                if parsed.is_empty()
                    || parsed
                        .iter()
                        .all(|p| p.text.trim().is_empty() && p.controls.is_empty())
                {
                    vec![Paragraph::new_empty()]
                } else {
                    parsed
                }
            };

            // 셀 문단 보정: char_count_msb, char_count, para_shape_id, raw_header_extra, line_segs
            let mut cell_paragraphs = cell_paragraphs;
            for cp_para in &mut cell_paragraphs {
                cp_para.char_count_msb = true; // 셀 문단은 항상 MSB 설정
                                               // char_count에 문단끝 마커(+1) 포함
                let text_chars = cp_para.text.chars().count() as u32;
                cp_para.char_count = text_chars + 1;

                // para_shape_id: 기본 "본문" ParaShape 사용 (DIFF-3)
                cp_para.para_shape_id = cell_para_shape_id;

                // DIFF-2: 모든 셀 문단은 시작 위치(0)에 명시적 CharShapeRef를 가져야 한다.
                // [paste-import/셀글자서식] 예전엔 무조건 id 0(기본)만 넣어 td의 굵기·색·크기
                // 서식이 사라졌다 — 시작 서식을 td의 char shape(cell_char_shape_id)로 넣는다.
                // 인라인 서식이 붙어 첫 run 이 이미 pos 0 이면 그대로 두고, pos>0 이면 앞에
                // 기본 서식을 끼운다.
                if cp_para.char_shapes.is_empty() {
                    cp_para
                        .char_shapes
                        .push(crate::model::paragraph::CharShapeRef {
                            start_pos: 0,
                            char_shape_id: cell_char_shape_id,
                        });
                } else if cp_para.char_shapes[0].start_pos != 0 {
                    cp_para.char_shapes.insert(
                        0,
                        crate::model::paragraph::CharShapeRef {
                            start_pos: 0,
                            char_shape_id: cell_char_shape_id,
                        },
                    );
                }

                // raw_header_extra에 instance_id = 0x80000000 설정
                if cp_para.raw_header_extra.len() >= 10 {
                    cp_para.raw_header_extra[6..10].copy_from_slice(&0x80000000u32.to_le_bytes());
                } else {
                    let mut rhe = vec![0u8; 10];
                    let n_cs = cp_para.char_shapes.len() as u16;
                    rhe[0..2].copy_from_slice(&n_cs.to_le_bytes());
                    // [2..4] n_range_tags = 0
                    let n_ls = cp_para.line_segs.len().max(1) as u16;
                    rhe[4..6].copy_from_slice(&n_ls.to_le_bytes());
                    rhe[6..10].copy_from_slice(&0x80000000u32.to_le_bytes());
                    cp_para.raw_header_extra = rhe;
                }

                // line_segs: 폰트 크기 기반 높이 계산
                let font_size = cp_para
                    .char_shapes
                    .first()
                    .and_then(|cs| {
                        self.document
                            .doc_info
                            .char_shapes
                            .get(cs.char_shape_id as usize)
                    })
                    .map(|cs| cs.base_size.max(400))
                    .unwrap_or(1000);
                let line_h = font_size;
                let text_h = font_size;
                let baseline = (font_size as f64 * 0.85) as i32;
                let spacing = (font_size as f64 * 0.6) as i32;
                // seg_width: 셀 폭에서 좌우 패딩을 뺀 텍스트 영역 폭
                let seg_w = (cell_width as i32) - (padding.left as i32) - (padding.right as i32);
                // tag(flags): bit 17(first segment) + bit 18(last segment).
                let line_tag: u32 = crate::model::paragraph::LineSeg::TAG_SINGLE_SEGMENT_LINE;

                if cp_para.line_segs.is_empty() {
                    cp_para.line_segs.push(crate::model::paragraph::LineSeg {
                        text_start: 0,
                        line_height: line_h,
                        text_height: text_h,
                        baseline_distance: baseline,
                        line_spacing: spacing,
                        segment_width: seg_w,
                        tag: line_tag,
                        ..Default::default()
                    });
                } else {
                    for ls in &mut cp_para.line_segs {
                        if ls.line_height < font_size {
                            ls.line_height = line_h;
                            ls.text_height = text_h;
                            ls.baseline_distance = baseline;
                            ls.line_spacing = spacing;
                        }
                        if ls.segment_width == 0 {
                            ls.segment_width = seg_w;
                        }
                        if ls.tag == 0 {
                            ls.tag = line_tag;
                        }
                    }
                }
            }

            if pc.is_header {
                has_header_row = true;
            }

            // [paste-import/여백] CSS로 padding을 준 셀은 안 여백 사용(apply_inner_margin)을
            // 켠다 — 예전엔 padding 값은 저장되나 플래그가 꺼진 채라 한글이 표 기본 여백을
            // 써 다르게 보였다. width_ref bit 0 도 함께 세워 라운드트립에서 일관되게 한다.
            let apply_inner_margin = pc.has_explicit_padding;

            // list_header_width_ref: is_header면 bit 2, apply_inner_margin이면 bit 0
            let lh_width_ref: u16 =
                (if pc.is_header { 0x04 } else { 0 }) | (if apply_inner_margin { 0x01 } else { 0 });

            // vertical_align → VerticalAlign enum
            // CSS에서 지정하지 않으면 기본값 Center (정상 HWP 파일 패턴)
            let v_align = match pc.vertical_align {
                0 => crate::model::table::VerticalAlign::Center, // CSS 미지정 → Center (HWP 기본)
                1 => crate::model::table::VerticalAlign::Center,
                2 => crate::model::table::VerticalAlign::Bottom,
                3 => crate::model::table::VerticalAlign::Top, // 명시적 top
                _ => crate::model::table::VerticalAlign::Center,
            };

            // raw_list_extra: 13바이트 (첫 4바이트 = 셀 폭 u32, 나머지 0)
            // 정상 파일에서 raw_list_extra[0..4] = cell_width (u32)
            let mut raw_list_extra = vec![0u8; 13];
            raw_list_extra[0..4].copy_from_slice(&cell_width.to_le_bytes());

            // [paste-import/rowspan] rowspan이 실제 행 수를 넘으면 표 밖을 가리키는
            // 병합 정보가 저장돼 규격을 벗어난다. 저장값을 남은 행 수로 클램프한다
            // (레이아웃 계산에 쓰인 occupied 그리드는 그대로 두고 저장/조회값만 보정).
            let clamped_row_span = cp
                .row_span
                .min(row_count.saturating_sub(cp.row))
                .max(1);

            cells.push(Cell {
                col: cp.col,
                row: cp.row,
                col_span: cp.col_span,
                row_span: clamped_row_span,
                width: cell_width,
                height: cell_height,
                padding,
                border_fill_id,
                paragraphs: cell_paragraphs,
                is_header: pc.is_header,
                apply_inner_margin,
                list_header_width_ref: lh_width_ref,
                vertical_align: v_align,
                raw_list_extra,
                ..Default::default()
            });
        }

        // 행 우선 순서로 정렬
        cells.sort_by(|a, b| a.row.cmp(&b.row).then(a.col.cmp(&b.col)));

        // --- 5. Table 구조체 조립 ---
        let total_width: u32 = col_widths.iter().sum();
        let total_height: u32 = row_heights.iter().sum();

        // table.attr = CommonObjAttr 플래그(파서: table.attr = common.attr).
        // [pagination-overflow/paste-import #2] 종전값 0x082A2311 은 bit0(글자처럼취급)·
        // bit13(쪽영역제한/restrictInPage)이 켜져 있었다. 조판기 is_effective_tac_table
        // (=`table.attr & 0x01`)이 이 표를 인라인 개체로 오판해 통째 배치 경로로 빠지고,
        // 쪽을 넘는 표(200행)가 안 갈라졌다(pageCount=1, 종이 위 겹침). bit0·bit13 을 꺼
        // "자리차지(TopAndBottom)·vert=Para·비-TAC 블록 표"로 만들면 조판기가 행 단위 분할
        // 경로(typeset_block_table)를 타 정상 분할된다. bit0 을 raw_ctrl_data 에도 함께 꺼
        // attr==common.attr 정합을 유지 → 저장·재로드 왕복 후에도 분할이 보존된다.
        // (0x082A2311 & !0x2001 = 0x082A0310)
        let table_attr: u32 = 0x082A2311 & !0x2001;

        // raw_ctrl_data: CommonObjAttr 전체 (attr 포함, parse_common_obj_attr 정합)
        // [0..4] attr, [4..8] vertical_offset, [8..12] horizontal_offset,
        // [12..16] width, [16..20] height, [20..24] z_order,
        // [24..26] margin.left, [26..28] margin.right,
        // [28..30] margin.top, [30..32] margin.bottom,
        // [32..36] instance_id, [36..38] desc_len(=0)
        let outer_margin: i16 = 283; // 바깥 여백 ~1mm
        let mut raw_ctrl_data = vec![0u8; 38]; // 32(base) + 2(desc_len) + 4(extra)
        raw_ctrl_data[common_obj_offsets::FLAGS].copy_from_slice(&table_attr.to_le_bytes());
        raw_ctrl_data[common_obj_offsets::WIDTH].copy_from_slice(&total_width.to_le_bytes());
        raw_ctrl_data[common_obj_offsets::HEIGHT].copy_from_slice(&total_height.to_le_bytes());
        // 바깥 여백 (left, right, top, bottom)
        raw_ctrl_data[common_obj_offsets::MARGIN_LEFT].copy_from_slice(&outer_margin.to_le_bytes());
        raw_ctrl_data[common_obj_offsets::MARGIN_RIGHT]
            .copy_from_slice(&outer_margin.to_le_bytes());
        raw_ctrl_data[common_obj_offsets::MARGIN_TOP].copy_from_slice(&outer_margin.to_le_bytes());
        raw_ctrl_data[common_obj_offsets::MARGIN_BOTTOM]
            .copy_from_slice(&outer_margin.to_le_bytes());
        // [32..36] instance_id (DIFF-7 수정: 해시 기반 유니크 값 생성)
        // 정상 HWP 파일에서는 instance_id가 고유한 비-0 값을 가짐
        let instance_id: u32 = {
            // 행/열 수, 셀 수, 총 폭/높이를 조합한 간단한 해시
            let mut h: u32 = 0x7c150000;
            h = h.wrapping_add(row_count as u32 * 0x1000);
            h = h.wrapping_add(col_count as u32 * 0x100);
            h = h.wrapping_add(total_width);
            h = h.wrapping_add(total_height.wrapping_mul(0x1b));
            h ^= cells.len() as u32 * 0x4b69;
            if h == 0 {
                h = 0x7c154b69;
            } // 절대 0이 되지 않도록
            h
        };
        raw_ctrl_data[common_obj_offsets::INSTANCE_ID].copy_from_slice(&instance_id.to_le_bytes());
        // [36..38] desc_len = 0

        // row_sizes: 각 행의 셀 수
        let row_sizes: Vec<i16> = (0..row_count)
            .map(|r| cells.iter().filter(|c| c.row == r).count() as i16)
            .collect();

        // 표 전체 기본 BorderFill: 정상 파일에서 모든 표가 border_fill_id=3 사용
        // border_fill_id는 1-based (DocInfo.border_fills 인덱스 + 1)
        let table_border_fill_id = if self.document.doc_info.border_fills.len() >= 3 {
            3u16
        } else if !self.document.doc_info.border_fills.is_empty() {
            1u16
        } else {
            0u16
        };

        // HTML <table> CSS에서 표 패딩 파싱
        let table_style =
            parse_inline_style(&table_html[..table_html.find('>').unwrap_or(table_html.len()) + 1])
                .to_lowercase();
        let table_padding_pt = parse_css_padding_pt(&table_style);
        // 기본값: L:510 R:510 T:141 B:141 (정상 HWP 파일 패턴)
        let table_padding = crate::model::Padding {
            left: if table_padding_pt[0] > 0.01 {
                (table_padding_pt[0] * 100.0).round() as i16
            } else {
                510
            },
            right: if table_padding_pt[1] > 0.01 {
                (table_padding_pt[1] * 100.0).round() as i16
            } else {
                510
            },
            top: if table_padding_pt[2] > 0.01 {
                (table_padding_pt[2] * 100.0).round() as i16
            } else {
                141
            },
            bottom: if table_padding_pt[3] > 0.01 {
                (table_padding_pt[3] * 100.0).round() as i16
            } else {
                141
            },
        };

        // raw_table_record_attr: 정상 파일 패턴 기반 (DIFF-5 수정)
        // bit 1: 셀 분리 금지 (항상 설정), bit 2: repeat_header
        // bit 26: 추가 레이아웃 속성
        // 정상 HWP 파일에서 모든 표는 bit 1 (셀 분리 금지) 이 항상 설정됨
        let tbl_rec_attr: u32 = 0x04000006; // bit 1(셀분리금지) + bit 2 + bit 26

        let outer_margin: i16 = 283; // 바깥 여백 기본값 ~1mm
        // [pagination-overflow/paste-import #2] in-memory `common` 을 raw_ctrl_data 에서
        // 파싱해 채운다(종전엔 Default 라 vert=Paper=종이 절대배치로 200행이 겹쳐 쌓였다).
        // 위 table_attr(bit0/bit13 off)에 따라 common 은 비-TAC·자리차지·vert=Para 로 잡혀
        // 흐름에 참여하고, in-memory==재로드 정합이 보장된다.
        let parsed_common =
            crate::parser::control::parse_common_obj_attr(&raw_ctrl_data);
        let mut table = Table {
            // attr == common.attr (파서 규약 유지) — 위 table_attr 에서 bit0/bit13 을 껐다.
            attr: table_attr,
            row_count,
            col_count,
            cell_spacing: 0,
            padding: table_padding,
            row_sizes,
            border_fill_id: table_border_fill_id,
            zones: Vec::new(),
            cells,
            cell_grid: Vec::new(),
            // 인라인 표는 행 경계 분할 허용(createTableEx 와 동일). None 이면 한 덩어리로
            // 남아 쪽을 넘겨도 안 갈라진다.
            page_break: TablePageBreak::RowBreak,
            repeat_header: has_header_row,
            caption: None,
            common: parsed_common,
            outer_margin_left: outer_margin,
            outer_margin_right: outer_margin,
            outer_margin_top: outer_margin,
            outer_margin_bottom: outer_margin,
            raw_ctrl_data,
            raw_table_record_attr: tbl_rec_attr,
            raw_table_record_extra: vec![0u8; 2], // 표준 추가 2바이트
            dirty: true,
            local_resize_rows: Vec::new(),
            local_resize_cols: Vec::new(),
            local_resize_cell_widths: Vec::new(),
            local_resize_cell_heights: Vec::new(),
        };
        table.rebuild_grid();

        // --- 6. Table Control을 포함하는 Paragraph 생성 ---
        // 제어문자는 text에 포함하지 않음 (serialize_para_text가 controls에서 생성)
        let default_char_shape_id = if !self.document.doc_info.char_shapes.is_empty() {
            0u32
        } else {
            self.document
                .doc_info
                .char_shapes
                .push(crate::model::style::CharShape::default());
            0
        };

        // 표 문단의 para_shape_id: 기존 문서의 표 문단에서 사용하는 값 탐색
        // 정상 파일에서 표 문단은 ps_id=1 사용 (기본 "본문" 스타일)
        let table_para_shape_id = {
            let mut found_ps = 0u16;
            'outer: for section in &self.document.sections {
                for para in &section.paragraphs {
                    for ctrl in &para.controls {
                        if let Control::Table(_) = ctrl {
                            found_ps = para.para_shape_id;
                            break 'outer;
                        }
                    }
                }
            }
            if found_ps == 0 && self.document.doc_info.para_shapes.len() > 1 {
                1u16 // 기본 "본문" ParaShape
            } else {
                found_ps
            }
        };

        // raw_header_extra: [0..2] n_char_shapes, [2..4] n_range_tags, [4..6] n_line_segs, [6..10] instance_id
        // 정상 파일에서 표 문단의 instance_id = 0x80000000
        let mut table_raw_header_extra = vec![0u8; 10];
        table_raw_header_extra[0..2].copy_from_slice(&1u16.to_le_bytes()); // n_char_shapes=1
                                                                           // [2..4] n_range_tags=0, [4..6] n_line_segs=1
        table_raw_header_extra[4..6].copy_from_slice(&1u16.to_le_bytes());
        // [6..10] instance_id=0x80000000
        table_raw_header_extra[6..10].copy_from_slice(&0x80000000u32.to_le_bytes());

        let table_para = Paragraph {
            text: String::new(),
            char_count: 9, // 확장 제어문자(8 code units) + 문단끝(1 code unit)
            control_mask: 0x00000800, // DrawTableObject (bit 11)
            char_offsets: vec![],
            char_shapes: vec![crate::model::paragraph::CharShapeRef {
                start_pos: 0,
                char_shape_id: default_char_shape_id,
            }],
            // [pagination-overflow/paste-import #2] 표 host 문단의 LINE_SEG는 한컴 표준을 따른다:
            // line_height=1000(placeholder)·segment_width=0. 종전엔 여기에 표 전체 높이/폭을
            // 통째로 실어(line_height=total_height, segment_width=total_width) 표가 "쪼갤 수 없는
            // 한 줄"로 굳어졌다 — 실제 표 높이는 HeightMeasurer가 셀에서 재므로 이 값이 크면
            // 인라인 페이지네이션이 행 단위로 못 가르고 한 쪽에 뭉친다. createTableEx 와 동일하게
            // 표준 placeholder 로 맞춰 행 분할 경로를 살린다.
            line_segs: vec![crate::model::paragraph::LineSeg {
                text_start: 0,
                line_height: 1000,
                text_height: 1000,
                baseline_distance: 850,
                line_spacing: 600,
                segment_width: 0,
                tag: crate::model::paragraph::LineSeg::TAG_SINGLE_SEGMENT_LINE,
                ..Default::default()
            }],
            para_shape_id: table_para_shape_id,
            style_id: 0,
            controls: vec![Control::Table(Box::new(table))],
            ctrl_data_records: vec![None],
            has_para_text: true,
            raw_header_extra: table_raw_header_extra,
            // 표 문단 자체의 MSB는 false (기존 HWP 문서 패턴)
            // FIX-1은 빈 문서 케이스에만 해당, 내용이 있는 문서에서는 false
            // 셀 내부 문단의 MSB는 true (셀 보정 코드에서 설정)
            char_count_msb: false,
            ..Default::default()
        };

        paragraphs.push(table_para);

        // [paste-import/caption] <caption> 텍스트는 표에도 본문에도 남지 않고 사라졌다.
        // 표 뒤에 캡션 문단을 두어 데이터 유실을 막는다(표는 para 0 을 유지). 렌더 표
        // 캡션(Table.caption)까지 붙이는 건 별도 과제 — 여기선 텍스트 보존이 목적.
        if let Some(cap_start) = table_lower.find("<caption") {
            if let Some(gt) = table_html[cap_start..].find('>') {
                let inner_start = cap_start + gt + 1;
                let inner_end = table_lower[inner_start..]
                    .find("</caption>")
                    .map(|i| inner_start + i)
                    .unwrap_or(inner_start);
                let cap_text = decode_html_entities(&html_strip_tags(
                    &table_html[inner_start..inner_end.min(table_html.len())],
                ));
                let cap_text = cap_text.trim();
                if !cap_text.is_empty() {
                    let mut cap_para = Paragraph::default();
                    cap_para.text = cap_text.to_string();
                    cap_para.char_count = cap_para.text.encode_utf16().count() as u32;
                    cap_para.char_offsets = cap_para
                        .text
                        .chars()
                        .scan(0u32, |acc, c| {
                            let off = *acc;
                            *acc += c.len_utf16() as u32;
                            Some(off)
                        })
                        .collect();
                    paragraphs.push(cap_para);
                }
            }
        }
    }

    /// CSS 테두리/배경 정보로 BorderFill을 생성하고 DocInfo에 등록한다.
    /// 동일한 BorderFill이 이미 있으면 기존 ID를 반환한다.
    pub(crate) fn create_border_fill_from_css(
        &mut self,
        border_widths_pt: &[f64; 4],
        border_colors: &[u32; 4],
        border_styles: &[u8; 4],
        background_color: Option<u32>,
    ) -> u16 {
        use crate::model::style::{
            BorderFill, BorderLine, BorderLineType, CenterLine, DiagonalLine, Fill, FillType,
            SolidFill,
        };

        let mut borders = [BorderLine::default(); 4];
        for i in 0..4 {
            if border_widths_pt[i] > 0.01 {
                borders[i] = BorderLine {
                    line_type: match border_styles[i] {
                        0 => BorderLineType::None,
                        1 => BorderLineType::Solid,
                        2 => BorderLineType::Dash,
                        3 => BorderLineType::Dot,
                        4 => BorderLineType::Double,
                        _ => BorderLineType::Solid,
                    },
                    width: css_border_width_to_hwp(border_widths_pt[i]),
                    color: border_colors[i],
                };
            } else {
                borders[i] = BorderLine {
                    line_type: BorderLineType::None,
                    width: 0,
                    color: 0,
                };
            }
        }

        let fill = if let Some(bg) = background_color {
            if bg != 0xFFFFFF {
                Fill {
                    fill_type: FillType::Solid,
                    solid: Some(SolidFill {
                        background_color: bg,
                        pattern_color: 0,
                        pattern_type: -1_i32, // 무늬 없음
                    }),
                    ..Default::default()
                }
            } else {
                Fill::default()
            }
        } else {
            Fill::default()
        };

        let bf = BorderFill {
            raw_data: None,
            attr: 0,
            borders,
            diagonal: DiagonalLine::default(),
            center_line: CenterLine::None,
            fill,
        };

        // 기존 BorderFill에서 동일한 항목 검색
        for (i, existing) in self.document.doc_info.border_fills.iter().enumerate() {
            if border_fills_equal(existing, &bf) {
                return (i + 1) as u16; // border_fill_id는 1-based
            }
        }

        // 새로 추가
        self.document.doc_info.border_fills.push(bf);
        self.document.doc_info.raw_stream_dirty = true;
        self.styles = resolve_styles(&self.document.doc_info, self.dpi);
        self.document.doc_info.border_fills.len() as u16
    }

    /// JSON에서 border/fill 속성을 파싱하여 BorderFill을 생성/재사용한다.
    /// 프론트엔드 글자 테두리/배경 대화상자에서 호출된다.
    pub(crate) fn create_border_fill_from_json(&mut self, json: &str) -> u16 {
        use crate::model::style::{BorderFill, BorderLine, CenterLine, DiagonalLine, Fill};

        // 기본 base: borderFillId 가 있으면 그 BorderFill 을 복제, 없으면 전 방향 실선(Solid)
        // 기본값(표 등 기존 호출부 동작 보존).
        let base = json_u32(json, "borderFillId")
            .and_then(|id| {
                if id == 0 {
                    None
                } else {
                    self.document
                        .doc_info
                        .border_fills
                        .get((id - 1) as usize)
                        .cloned()
                }
            })
            .unwrap_or(BorderFill {
                raw_data: None,
                attr: 0,
                borders: [BorderLine::default(); 4],
                diagonal: DiagonalLine::default(),
                center_line: CenterLine::None,
                fill: Fill::default(),
            });
        self.create_border_fill_from_json_based(json, base)
    }

    /// create_border_fill_from_json 의 base 지정 변형.
    /// [page-section/결함6] 쪽 테두리는 지정 안 한 방향까지 실선 기본값으로 덮이면 안 되므로,
    /// 호출부가 '현재 테두리' 또는 '선없음' base 를 넘겨 지정한 키만 병합되게 한다.
    pub(crate) fn create_border_fill_from_json_based(
        &mut self,
        json: &str,
        mut bf: crate::model::style::BorderFill,
    ) -> u16 {
        use crate::model::style::{CenterLine, Fill, FillType, SolidFill};

        fn json_diag_bits(json: &str, key: &str) -> Option<u16> {
            json_i32(json, key)
                .map(|v| (v as u16) & 0x07)
                .or_else(|| json_bool(json, key).map(|v| if v { 0b010 } else { 0 }))
        }

        fn json_center_line(json: &str) -> Option<CenterLine> {
            json_str(json, "centerLine").map(|value| {
                let normalized = value.trim().to_ascii_uppercase();
                match normalized.as_str() {
                    "VERTICAL" | "HORIZONTAL_BAR" => CenterLine::Vertical,
                    "HORIZONTAL" | "VERTICAL_BAR" => CenterLine::Horizontal,
                    "CROSS" => CenterLine::Cross,
                    _ => CenterLine::None,
                }
            })
        }

        bf.raw_data = None;

        // 4방향 테두리 파싱
        let dir_keys = ["borderLeft", "borderRight", "borderTop", "borderBottom"];
        for (i, key) in dir_keys.iter().enumerate() {
            if let Some(obj_str) = json_object(json, key) {
                let type_val = json_i32(&obj_str, "type").unwrap_or(0);
                bf.borders[i].line_type = u8_to_border_line_type(type_val as u8);
                bf.borders[i].width = json_i32(&obj_str, "width").unwrap_or(0) as u8;
                bf.borders[i].color = json_color(&obj_str, "color").unwrap_or(0);
            }
        }

        // 채우기 파싱
        if let Some(fill_type_str) = json_str(json, "fillType") {
            bf.fill = if fill_type_str == "solid" {
                let bg = json_color(json, "fillColor").unwrap_or(0xFFFFFF);
                let pat_c = json_color(json, "patternColor").unwrap_or(0);
                let pat_t = json_i32(json, "patternType").unwrap_or(0);
                Fill {
                    fill_type: FillType::Solid,
                    solid: Some(SolidFill {
                        background_color: bg,
                        pattern_color: pat_c,
                        pattern_type: pat_t,
                    }),
                    ..Default::default()
                }
            } else {
                Fill::default()
            };
        }

        let has_diagonal_json = json.contains("\"diagonalLine\"")
            || json.contains("\"diagonalSlash\"")
            || json.contains("\"diagonalBackSlash\"")
            || json.contains("\"diagonalWidth\"")
            || json.contains("\"diagonalColor\"")
            || json.contains("\"centerLine\"");
        if has_diagonal_json {
            let slash_bits_json = json_diag_bits(json, "diagonalSlash");
            let backslash_bits_json = json_diag_bits(json, "diagonalBackSlash");
            let center_line_json = json_center_line(json);
            let mut slash_bits = slash_bits_json.unwrap_or(((bf.attr >> 2) & 0x07) as u16);
            let mut backslash_bits = backslash_bits_json.unwrap_or(((bf.attr >> 5) & 0x07) as u16);
            let explicit_diagonal_bits = slash_bits_json.is_some() || backslash_bits_json.is_some();
            if let Some(center_line) = center_line_json {
                bf.center_line = center_line;
            }
            bf.diagonal.diagonal_type =
                json_i32(json, "diagonalLine").unwrap_or(bf.diagonal.diagonal_type as i32) as u8;
            bf.diagonal.width =
                json_i32(json, "diagonalWidth").unwrap_or(bf.diagonal.width as i32) as u8;
            bf.diagonal.color = json_color(json, "diagonalColor").unwrap_or(bf.diagonal.color);

            if bf.center_line != CenterLine::None
                && (center_line_json.is_some() || !explicit_diagonal_bits)
            {
                slash_bits = 0;
                backslash_bits = 0;
            } else if slash_bits != 0 || backslash_bits != 0 {
                bf.center_line = CenterLine::None;
            }

            bf.attr &= !((0x07 << 2)
                | (0x07 << 5)
                | (0x03 << 8)
                | (1 << 10)
                | (1 << 11)
                | (1 << 12)
                | (1 << 13));
            bf.attr |= (slash_bits & 0x07) << 2;
            bf.attr |= (backslash_bits & 0x07) << 5;
            bf.attr |= bf.center_line.hwp_attr_bits();
        }

        // 기존 BorderFill에서 동일한 항목 검색
        for (i, existing) in self.document.doc_info.border_fills.iter().enumerate() {
            if border_fills_equal(existing, &bf) {
                return (i + 1) as u16;
            }
        }

        // 새로 추가
        self.document.doc_info.border_fills.push(bf);
        self.document.doc_info.raw_stream_dirty = true;
        self.styles = resolve_styles(&self.document.doc_info, self.dpi);
        self.document.doc_info.border_fills.len() as u16
    }

    /// <img> 태그를 파싱하여 이미지 데이터를 문서에 추가한다.
    /// (base64 data URI만 지원)
    pub(crate) fn parse_img_html(&mut self, paragraphs: &mut Vec<Paragraph>, img_tag: &str) {
        // src="data:image/...;base64,..." 추출
        let src = if let Some(src_start) = img_tag.find("src=\"") {
            let after = &img_tag[src_start + 5..];
            if let Some(end) = after.find('"') {
                &after[..end]
            } else {
                return;
            }
        } else if let Some(src_start) = img_tag.find("src='") {
            let after = &img_tag[src_start + 5..];
            if let Some(end) = after.find('\'') {
                &after[..end]
            } else {
                return;
            }
        } else {
            return;
        };

        if !src.starts_with("data:") {
            // 외부 URL 이미지는 처리하지 않음 — 텍스트로 대체
            let mut para = Paragraph::default();
            para.text = "[이미지]".to_string();
            para.char_count = para.text.encode_utf16().count() as u32;
            para.char_offsets = para
                .text
                .chars()
                .scan(0u32, |acc, c| {
                    let off = *acc;
                    *acc += c.len_utf16() as u32;
                    Some(off)
                })
                .collect();
            paragraphs.push(para);
            return;
        }

        // data:image/png;base64,XXXXX 파싱
        let after_data = &src[5..]; // "image/png;base64,XXXXX"
        let base64_start = if let Some(comma) = after_data.find(',') {
            comma + 1
        } else {
            return;
        };
        let base64_str = &after_data[base64_start..];

        use base64::Engine;
        let decoded = match base64::engine::general_purpose::STANDARD.decode(base64_str) {
            Ok(d) => d,
            Err(_) => return,
        };

        if decoded.is_empty() {
            return;
        }

        // BinData로 등록 — bin_data_id(위치)와 storage id 분리 채번
        // (insert_picture_native 와 동일 규칙, 기존 storage id 충돌 방지)
        let new_bin_id = (self.document.bin_data_content.len() + 1) as u16;
        let storage_id = self.document.next_bin_data_storage_id();
        self.document
            .bin_data_content
            .push(crate::model::bin_data::BinDataContent {
                id: storage_id,
                data: decoded.clone().into(),
                extension: detect_clipboard_image_mime(&decoded)
                    .split('/')
                    .nth(1)
                    .unwrap_or("png")
                    .to_string(),
            });

        // width/height 추출
        let width = parse_html_attr_f64(img_tag, "width").unwrap_or(200.0);
        let height = parse_html_attr_f64(img_tag, "height").unwrap_or(150.0);

        // px → HWPUNIT
        let w_hu = crate::renderer::px_to_hwpunit(width, self.dpi) as u32;
        let h_hu = crate::renderer::px_to_hwpunit(height, self.dpi) as u32;

        // Picture Control 생성 (placeholder로 텍스트 표현)
        let mut para = Paragraph::default();
        para.text = "[이미지]".to_string();
        para.char_count = para.text.encode_utf16().count() as u32;
        para.char_offsets = para
            .text
            .chars()
            .scan(0u32, |acc, c| {
                let off = *acc;
                *acc += c.len_utf16() as u32;
                Some(off)
            })
            .collect();

        // Picture 컨트롤 생성
        let mut pic = crate::model::image::Picture::default();
        pic.image_attr.bin_data_id = new_bin_id;
        pic.common.width = w_hu;
        pic.common.height = h_hu;
        pic.common.vertical_offset = 0;
        pic.common.horizontal_offset = 0;
        para.controls.push(Control::Picture(Box::new(pic)));

        paragraphs.push(para);
    }
}
