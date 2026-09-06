//! 표 (Table, Cell, Row)

use super::paragraph::Paragraph;
use super::shape::{common_obj_offsets, Caption};
use super::table_grid::{self, Axis, Located, TableGrid};
use super::*;

pub const CELL_FLAG_HAS_MARGIN: u16 = 0x0001;
pub const CELL_FLAG_PROTECT: u16 = 0x0002;
pub const CELL_FLAG_HEADER: u16 = 0x0004;
pub const CELL_FLAG_EDITABLE_IN_FORM: u16 = 0x0008;

/// [불변식 가드 2026-09-02] 표 명령이 보존해야 하는 바깥 크기 — `Table::check_deltas` 의 검사 클래스.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CmdClass {
    /// 어긋내기·복원·병합·나누기·순델타 0 리사이즈: 표 폭·높이 모두 불변
    KeepWidthHeight,
    /// 행 삽입·삭제: 표 폭 불변(높이는 변함)
    KeepWidth,
    /// 열 삽입·삭제: 표 높이 불변(폭은 변함)
    KeepHeight,
    /// 셀 속성·열 폭 절대 설정·표 성장 허용 리사이즈: 크기 검사 없음(구조·내용·슬리버만)
    MayGrow,
}

/// 표 개체 (HWPTAG_TABLE)
#[derive(Debug, Default, Clone)]
pub struct Table {
    /// 속성 비트 플래그
    pub attr: u32,
    /// 행 수
    pub row_count: u16,
    /// 열 수
    pub col_count: u16,
    /// 셀 간격
    pub cell_spacing: HwpUnit16,
    /// 안쪽 여백
    pub padding: Padding,
    /// 행별 셀 수 (HWP 스펙: UINT16[NRows])
    pub row_sizes: Vec<HwpUnit16>,
    /// 테두리/배경 ID 참조
    pub border_fill_id: u16,
    /// 영역 속성 목록
    pub zones: Vec<TableZone>,
    /// 셀 목록 (행 우선 순서)
    pub cells: Vec<Cell>,
    /// 2D 그리드 인덱스: grid[row * col_count + col] = Some(cell_idx)
    /// 병합 셀의 span 영역 전체가 앵커 셀 인덱스를 가리킴
    pub cell_grid: Vec<Option<usize>>,
    /// 쪽 경계에서 나눔 (0: 나누지 않음, 1: 셀 단위로 나눔)
    pub page_break: TablePageBreak,
    /// 제목 줄 자동 반복
    pub repeat_header: bool,
    /// 캡션 정보
    pub caption: Option<Caption>,
    /// 공통 객체 속성 (위치, 배치, 크기 등)
    pub common: crate::model::shape::CommonObjAttr,
    /// 바깥 여백 (CommonObjAttr의 오브젝트 바깥 4방향 여백)
    pub outer_margin_left: i16,
    pub outer_margin_right: i16,
    pub outer_margin_top: i16,
    pub outer_margin_bottom: i16,
    /// CTRL_HEADER ctrl_data의 4바이트(attr) 이후 추가 바이트 (라운드트립 보존용)
    pub raw_ctrl_data: Vec<u8>,
    /// HWPTAG_TABLE 레코드의 원본 속성 값 (라운드트립 보존용, 0이면 재구성)
    pub raw_table_record_attr: u32,
    /// HWPTAG_TABLE 레코드의 border_fill_id 이후 추가 바이트 (라운드트립 보존용)
    pub raw_table_record_extra: Vec<u8>,
    /// 구조/내용 변경 시 true → 재측정 필요 (Default: false)
    #[doc(hidden)]
    pub dirty: bool,
}

/// 표 쪽 나눔 종류
/// bit 0-1: 0=나누지 않음, 1=셀 단위로 나눔, 2=나눔(행 단위)
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum TablePageBreak {
    /// 나누지 않음 (0)
    #[default]
    None,
    /// 셀 단위로 나눔 (1) — 행 내부(인트라-로우) 분할 허용
    CellBreak,
    /// 나눔 (2) — 행 경계에서만 나눔 (인트라-로우 분할 없음)
    RowBreak,
}

/// 표 영역 속성
#[derive(Debug, Clone, Default)]
pub struct TableZone {
    /// 시작 열 주소
    pub start_col: u16,
    /// 시작 행 주소
    pub start_row: u16,
    /// 끝 열 주소
    pub end_col: u16,
    /// 끝 행 주소
    pub end_row: u16,
    /// 테두리/배경 ID 참조
    pub border_fill_id: u16,
}

/// 표 셀 (HWPTAG_LIST_HEADER + 셀 속성)
#[derive(Debug, Default, Clone)]
pub struct Cell {
    /// 셀 열 주소 (0부터 시작)
    pub col: u16,
    /// 셀 행 주소 (0부터 시작)
    pub row: u16,
    /// 열 병합 개수
    pub col_span: u16,
    /// 행 병합 개수
    pub row_span: u16,
    /// 셀 폭
    pub width: HwpUnit,
    /// 셀 높이
    pub height: HwpUnit,
    /// 셀 여백
    pub padding: Padding,
    /// 테두리/배경 ID 참조
    pub border_fill_id: u16,
    /// 셀 내 문단 리스트
    pub paragraphs: Vec<Paragraph>,
    /// LIST_HEADER의 텍스트 영역 폭 참조 (라운드트립 보존용)
    pub list_header_width_ref: u16,
    /// 텍스트 방향 (0: 가로, 1: 세로)
    pub text_direction: u8,
    /// 세로 정렬 (0: top, 1: center, 2: bottom)
    pub vertical_align: VerticalAlign,
    /// 안 여백 지정 여부 (list_attr bit 16)
    /// true: 셀 고유 padding 사용, false: 표 기본 padding 사용
    pub apply_inner_margin: bool,
    /// 제목 셀 여부 (list_attr bit 18)
    pub is_header: bool,
    /// LIST_HEADER 레코드의 34바이트 이후 추가 바이트 (라운드트립 보존용)
    pub raw_list_extra: Vec<u8>,
    /// 셀 필드 이름 (한컴 셀 속성 → 필드 → 필드 이름)
    /// raw_list_extra의 offset 14-15(name_len) + offset 16~(UTF-16LE)에서 추출
    pub field_name: Option<String>,
}

/// 표 셀 행/열 바꿈 복사 데이터.
///
/// `cells[row][col]` 은 원본 범위의 셀 문단 목록이다.
#[derive(Debug, Clone, Default)]
pub struct TableTransposeData {
    pub source_rows: u16,
    pub source_cols: u16,
    pub cells: Vec<Vec<Vec<Paragraph>>>,
}

fn distribute_hwp_units(total: HwpUnit, count: u16) -> Vec<HwpUnit> {
    if count == 0 {
        return Vec::new();
    }
    let base = total / count as u32;
    let remainder = total % count as u32;
    (0..count)
        .map(|idx| base + u32::from(idx < remainder as u16))
        .collect()
}

/// 세로 정렬
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum VerticalAlign {
    #[default]
    Top,
    Center,
    Bottom,
}

impl Cell {
    pub fn set_apply_inner_margin(&mut self, value: bool) {
        self.apply_inner_margin = value;
        self.set_list_header_flag(CELL_FLAG_HAS_MARGIN, value);
    }

    /// [Task #1785] 렌더에 실제 적용되는 축별 안 여백 선택 규칙 (단일 출처).
    ///
    /// HWP 스펙: aim=true → cell.padding(단, 0 은 표 기본으로 폴백), aim=false →
    /// table.padding. 예외로 aim=false 여도 레거시 보존값(cell > table, 2500HU 미만)은
    /// 한컴이 렌더에 사용한다 (KTX 목차, exam_kor 보기 박스).
    /// 레이아웃(resolve_cell_padding)과 높이 측정(height_measurer)이 반드시 같은 값을
    /// 봐야 한다 — 규칙이 갈리면 예약 높이와 실제 렌더가 어긋나 표 높이가 틀어진다.
    pub fn use_cell_padding_axis(
        &self,
        cell_padding: i16,
        table_padding: i16,
        allow_saved_small_cell_margin: bool,
    ) -> bool {
        if self.apply_inner_margin {
            // [#2070] aim=true 의 0 은 사용자가 지정한 셀 고유 안 여백 — 존중한다
            // (한글 PDF 실측: 시장구조조사 c0 pad=(0,0) 코드 폭 37.0px > 표 폴백
            // inner 26.5px — 표 패딩 폴백이면 물리적으로 1줄 불가). 음수는 결측
            // 센티널로 보고 표 패딩 폴백 유지.
            return cell_padding >= 0;
        }
        // [#2195] aim=false 는 **전 축 표 기본** — pad 통제 사다리 2종 실측:
        // (1) 표(0,0,141,141)+셀(510,510,141,141): 실효 좌우 0·상하 141,
        // (2) 표(510,510,223,223)+셀 동일: inner = 표폭-1020(좌우 510x2)·상하 223.
        // 종전 #1785 보존 규칙(cell>table 시 셀 채택)은 사다리와 불합치 — 제거.
        // (36381023 결재란 RT 케이스는 전체 게이트로 재검증.)
        let _ = allow_saved_small_cell_margin;
        false
    }

    /// 축별 규칙(`use_cell_padding_axis`)을 네 축에 적용한 유효 안 여백 (HWPUNIT).
    /// [#2195 stage50] 표 기본 여백이 **네 축 모두 0**(미지정)이면 셀 저장 pad —
    /// 86712 구분선(한글 PDF 괘선 21.1px = 셀 141 상하 포함) + exam_social 머리말
    /// 글상자(#1100 rect 오라클) 실측. pad 사다리의 '표 기본' 실측은 표 기본이
    /// 일부 축만 0(0,0,141,141)인 케이스 — 전축 0 과 구분된다.
    pub fn table_padding_unspecified(table_padding: &crate::model::Padding) -> bool {
        table_padding.left == 0
            && table_padding.right == 0
            && table_padding.top == 0
            && table_padding.bottom == 0
    }

    pub fn effective_padding(
        &self,
        table_padding: &crate::model::Padding,
    ) -> crate::model::Padding {
        let unspec = !self.apply_inner_margin && Self::table_padding_unspecified(table_padding);
        let pick = |c: i16, t: i16| -> i16 {
            // [#1785 위생 한도 유지] 10mm급(>=2500HU) 보존 pad 는 한컴이 렌더에
            // 쓰지 않는다(36381023 render-diff) — 전축0 미지정 규칙에서도 제외.
            if (unspec && c < 2500) || self.use_cell_padding_axis(c, t, false) {
                c
            } else {
                t
            }
        };
        crate::model::Padding {
            left: pick(self.padding.left, table_padding.left),
            right: pick(self.padding.right, table_padding.right),
            top: pick(self.padding.top, table_padding.top),
            bottom: pick(self.padding.bottom, table_padding.bottom),
        }
    }

    pub fn cell_protect(&self) -> bool {
        self.list_header_width_ref & CELL_FLAG_PROTECT != 0
    }

    pub fn set_cell_protect(&mut self, value: bool) {
        self.set_list_header_flag(CELL_FLAG_PROTECT, value);
    }

    pub fn set_header(&mut self, value: bool) {
        self.is_header = value;
        self.set_list_header_flag(CELL_FLAG_HEADER, value);
    }

    pub fn editable_in_form(&self) -> bool {
        self.list_header_width_ref & CELL_FLAG_EDITABLE_IN_FORM != 0
    }

    pub fn set_editable_in_form(&mut self, value: bool) {
        self.set_list_header_flag(CELL_FLAG_EDITABLE_IN_FORM, value);
    }

    fn set_list_header_flag(&mut self, flag: u16, value: bool) {
        if value {
            self.list_header_width_ref |= flag;
        } else {
            self.list_header_width_ref &= !flag;
        }
    }

    /// 빈 셀을 생성한다 (빈 문단 1개 포함).
    pub fn new_empty(
        col: u16,
        row: u16,
        width: HwpUnit,
        height: HwpUnit,
        border_fill_id: u16,
    ) -> Self {
        Cell {
            col,
            row,
            col_span: 1,
            row_span: 1,
            width,
            height,
            border_fill_id,
            paragraphs: vec![Paragraph::new_empty()],
            ..Default::default()
        }
    }

    /// 기존 셀을 템플릿으로 사용하여 빈 셀을 생성한다.
    ///
    /// raw_list_extra, padding, vertical_align 등 메타데이터를 복사하고,
    /// 첫 문단의 raw_header_extra, char_shapes, line_segs 구조를 복사한다.
    pub fn new_from_template(
        col: u16,
        row: u16,
        width: HwpUnit,
        height: HwpUnit,
        template: &Cell,
    ) -> Self {
        // 템플릿 문단의 구조를 복사하되 텍스트는 비움
        let para = if let Some(tpl_para) = template.paragraphs.first() {
            // instanceId를 0으로 초기화 (새 셀의 문단은 고유 ID 불필요)
            let mut raw_header_extra = tpl_para.raw_header_extra.clone();
            if raw_header_extra.len() >= 10 {
                // raw_header_extra[6..10] = instanceId
                raw_header_extra[6..10].copy_from_slice(&[0, 0, 0, 0]);
            }

            Paragraph {
                char_count: 1,        // 빈 문단: 끝 마커(0x000D) 포함
                char_count_msb: true, // 셀 문단은 항상 MSB 설정
                text: String::new(),
                char_shapes: tpl_para.char_shapes.iter().take(1).cloned().collect(),
                line_segs: tpl_para.line_segs.iter().take(1).cloned().collect(),
                para_shape_id: tpl_para.para_shape_id,
                style_id: tpl_para.style_id,
                raw_header_extra,
                has_para_text: false, // 빈 셀은 PARA_TEXT 불필요
                ..Default::default()
            }
        } else {
            Paragraph::new_empty()
        };

        Cell {
            col,
            row,
            col_span: 1,
            row_span: 1,
            width,
            height,
            border_fill_id: template.border_fill_id,
            padding: template.padding,
            list_header_width_ref: template.list_header_width_ref,
            text_direction: template.text_direction,
            vertical_align: template.vertical_align,
            apply_inner_margin: template.apply_inner_margin,
            is_header: template.is_header,
            raw_list_extra: template.raw_list_extra.clone(),
            field_name: None,
            paragraphs: vec![para],
        }
    }
}

/// 축별 (start, span, size) 읽기 — Cols=(col,col_span,width), Rows=(row,row_span,height).
fn axis_of(c: &Cell, axis: Axis) -> (u16, u16, HwpUnit) {
    match axis {
        Axis::Cols => (c.col, c.col_span, c.width),
        Axis::Rows => (c.row, c.row_span, c.height),
    }
}

/// 직교 밴드 (start, span) — Cols 면 (row,row_span), Rows 면 (col,col_span).
fn ortho_of(c: &Cell, axis: Axis) -> (u16, u16) {
    match axis {
        Axis::Cols => (c.row, c.row_span),
        Axis::Rows => (c.col, c.col_span),
    }
}

fn axis_mut(c: &mut Cell, axis: Axis) -> (&mut u16, &mut u16, &mut HwpUnit) {
    match axis {
        Axis::Cols => (&mut c.col, &mut c.col_span, &mut c.width),
        Axis::Rows => (&mut c.row, &mut c.row_span, &mut c.height),
    }
}

/// 셀 `c` 의 밴드에서 축 위치 `line` 에서 시작하는 슬롯 키 (row, col).
fn key_at(axis: Axis, c: &Cell, line: u16) -> (u16, u16) {
    match axis {
        Axis::Cols => (c.row, line),
        Axis::Rows => (line, c.col),
    }
}

/// `c` 를 축 방향으로 (앞 span_a·size_a, 뒤 span_b·size_b) 두 조각으로. 뒤 조각은 `second_start` 에서
/// 시작한다. 내용(문단·필드)은 `content_far` 면 뒤 조각, 아니면 앞 조각에 남고 다른 조각은
/// `Cell::new_from_template` 빈 셀(문단 모양·테두리 유지, 필드명 없음)이다.
#[allow(clippy::too_many_arguments)]
fn split_pieces(
    c: &Cell,
    axis: Axis,
    second_start: u16,
    span_a: u16,
    span_b: u16,
    size_a: HwpUnit,
    size_b: HwpUnit,
    content_far: bool,
) -> (Cell, Cell) {
    let keep = c.clone();
    let mut empty = Cell::new_from_template(c.col, c.row, c.width, c.height, c);
    empty.col_span = c.col_span;
    empty.row_span = c.row_span;
    let (mut a, mut b) = if content_far { (empty, keep) } else { (keep, empty) };
    {
        let (_, sp, sz) = axis_mut(&mut a, axis);
        *sp = span_a;
        *sz = size_a;
    }
    {
        let (s, sp, sz) = axis_mut(&mut b, axis);
        *s = second_start;
        *sp = span_b;
        *sz = size_b;
    }
    (a, b)
}

impl Table {
    /// 셀 최소 크기(HU) — 어긋내기·resize·열 삽입·fit 의 공통 바닥(종전 지역 상수 5곳 통합).
    pub const MIN_CELL: i32 = 200;

    /// [Task #1716] 반복 제목행으로 재사용할 **표 상단의 연속 제목행 블록** `0..H` 를 반환한다.
    ///
    /// 행 r 이 제목행 ⟺ header 셀(`is_header`, rowspan 덮개 포함)이 r 을 덮음. 상단(행 0)부터
    /// 제목행이 연속되는 최대 구간만 반환하고, 표 중간·하단에 흩어진 `is_header` 행은 제외한다.
    /// (일부 문서는 본문 행에도 `header="1"` 을 다수 부여한다. 그 행들까지 반복 대상으로 잡으면
    /// 연속 페이지마다 반복 overhead 가 누적되어 가용 높이가 0이 되고 페이지당 1행 폭주가 발생.)
    pub fn leading_header_rows(&self) -> Vec<usize> {
        let rc = self.row_count as usize;
        if rc == 0 {
            return Vec::new();
        }
        let mut is_header_row = vec![false; rc];
        for cell in &self.cells {
            if !cell.is_header {
                continue;
            }
            let start = cell.row as usize;
            let span = (cell.row_span as usize).max(1);
            for r in start..(start + span).min(rc) {
                is_header_row[r] = true;
            }
        }
        let mut h = 0usize;
        while h < rc && is_header_row[h] {
            h += 1;
        }
        (0..h).collect()
    }

    /// 행 단위 가로 resize 를 셀 폭 패턴에서 보수적으로 추론한다(로드 경로 규칙 — 런타임 힌트 아님;
    /// 힌트 필드 local_resize_* 는 9-c 에서 폐기).
    ///
    /// 같은 셀 배치 패턴을 공유하는 행들 중 다수의 폭 벡터와 다른 소수 행만 행 단위
    /// resize 결과로 간주한다. 병합 패턴이 유일한 행은 원본 문서 구조일 가능성이 높아
    /// 추론 대상에서 제외한다.
    pub fn inferred_local_resize_rows(&self) -> Vec<u16> {
        let col_count = self.col_count as usize;
        if col_count == 0 || self.row_count == 0 {
            return Vec::new();
        }

        let mut grouped_rows =
            std::collections::BTreeMap::<Vec<(u16, u16)>, Vec<(u16, Vec<u32>)>>::new();

        for row in 0..self.row_count {
            let mut row_cells = self
                .cells
                .iter()
                .filter(|cell| cell.row == row && cell.row_span == 1)
                .collect::<Vec<_>>();
            row_cells.sort_by_key(|cell| cell.col);

            let mut next_col = 0u16;
            let mut pattern = Vec::new();
            let mut widths = Vec::new();
            let mut valid = !row_cells.is_empty();

            for cell in row_cells {
                let span = cell.col_span.max(1);
                let end_col = cell.col.saturating_add(span);
                if cell.col != next_col || end_col <= cell.col || end_col as usize > col_count {
                    valid = false;
                    break;
                }

                pattern.push((cell.col, span));
                widths.push(cell.width);
                next_col = end_col;
            }

            if !valid || next_col as usize != col_count {
                continue;
            }

            grouped_rows.entry(pattern).or_default().push((row, widths));
        }

        let mut inferred = std::collections::BTreeSet::new();
        for rows in grouped_rows.values() {
            if rows.len() < 2 {
                continue;
            }

            let mut width_counts = std::collections::BTreeMap::<Vec<u32>, usize>::new();
            for (_, widths) in rows {
                *width_counts.entry(widths.clone()).or_default() += 1;
            }

            let Some((dominant_widths, dominant_count)) =
                width_counts.iter().max_by_key(|(_, count)| **count)
            else {
                continue;
            };
            if *dominant_count < 2 {
                continue;
            }

            let dominant_is_tied = width_counts
                .values()
                .filter(|count| **count == *dominant_count)
                .count()
                > 1;
            if dominant_is_tied {
                continue;
            }

            for (row, widths) in rows {
                if widths != dominant_widths {
                    inferred.insert(*row);
                }
            }
        }

        inferred.into_iter().collect()
    }

    /// 2D 그리드 인덱스를 재구축한다.
    /// 구조 변경(파싱, 행/열 추가/삭제, 병합/분할) 후 호출해야 한다.
    pub fn rebuild_grid(&mut self) {
        let rc = self.row_count as usize;
        let cc = self.col_count as usize;
        self.cell_grid = vec![None; rc * cc];
        for (idx, cell) in self.cells.iter().enumerate() {
            for r in cell.row..(cell.row + cell.row_span) {
                for c in cell.col..(cell.col + cell.col_span) {
                    let gi = (r as usize) * cc + (c as usize);
                    if gi < self.cell_grid.len() {
                        self.cell_grid[gi] = Some(idx);
                    }
                }
            }
        }
    }

    /// O(1) 셀 인덱스 조회. rebuild_grid() 호출 후 사용해야 한다.
    pub fn cell_index_at(&self, row: u16, col: u16) -> Option<usize> {
        let idx = (row as usize) * (self.col_count as usize) + (col as usize);
        self.cell_grid.get(idx)?.as_ref().copied()
    }

    /// O(1) 셀 접근 (불변). rebuild_grid() 호출 후 사용해야 한다.
    pub fn cell_at(&self, row: u16, col: u16) -> Option<&Cell> {
        let idx = (row as usize) * (self.col_count as usize) + (col as usize);
        let &cell_idx = self.cell_grid.get(idx)?.as_ref()?;
        self.cells.get(cell_idx)
    }

    /// O(1) 셀 접근 (가변). rebuild_grid() 호출 후 사용해야 한다.
    pub fn cell_at_mut(&mut self, row: u16, col: u16) -> Option<&mut Cell> {
        let idx = (row as usize) * (self.col_count as usize) + (col as usize);
        let &cell_idx = self.cell_grid.get(idx)?.as_ref()?;
        self.cells.get_mut(cell_idx)
    }

    fn validate_unmerged_rect(
        &self,
        start_row: u16,
        start_col: u16,
        end_row: u16,
        end_col: u16,
    ) -> Result<(), String> {
        if start_row > end_row || start_col > end_col {
            return Err("셀 범위가 유효하지 않습니다".to_string());
        }
        if end_row >= self.row_count || end_col >= self.col_count {
            return Err(format!(
                "셀 범위 ({},{})~({},{})가 표 크기 {}×{}를 초과합니다",
                start_row, start_col, end_row, end_col, self.row_count, self.col_count
            ));
        }

        for row in start_row..=end_row {
            for col in start_col..=end_col {
                let cell_idx = self
                    .cell_index_at(row, col)
                    .ok_or_else(|| format!("셀 ({},{})을 찾을 수 없습니다", row, col))?;
                let cell = &self.cells[cell_idx];
                if cell.row != row || cell.col != col || cell.row_span != 1 || cell.col_span != 1 {
                    return Err(format!(
                        "병합 셀 ({},{})은 행/열 바꿈 범위에 포함할 수 없습니다",
                        cell.row, cell.col
                    ));
                }
            }
        }

        Ok(())
    }

    /// 직사각형 범위의 셀 문단을 행/열 바꿈 복사용 데이터로 복사한다.
    pub fn copy_transpose_range(
        &self,
        start_row: u16,
        start_col: u16,
        end_row: u16,
        end_col: u16,
    ) -> Result<TableTransposeData, String> {
        self.validate_unmerged_rect(start_row, start_col, end_row, end_col)?;

        let source_rows = end_row - start_row + 1;
        let source_cols = end_col - start_col + 1;
        let mut cells = Vec::with_capacity(source_rows as usize);

        for row in start_row..=end_row {
            let mut row_cells = Vec::with_capacity(source_cols as usize);
            for col in start_col..=end_col {
                let cell_idx = self
                    .cell_index_at(row, col)
                    .ok_or_else(|| format!("셀 ({},{})을 찾을 수 없습니다", row, col))?;
                row_cells.push(self.cells[cell_idx].paragraphs.clone());
            }
            cells.push(row_cells);
        }

        Ok(TableTransposeData {
            source_rows,
            source_cols,
            cells,
        })
    }

    /// 행/열 바꿈 복사 데이터를 대상 시작 셀부터 붙여넣는다.
    ///
    /// 반환값은 내용이 교체된 `(cell_idx, paragraph_count)` 목록이다.
    pub fn paste_transposed_cells(
        &mut self,
        start_row: u16,
        start_col: u16,
        data: &TableTransposeData,
    ) -> Result<Vec<(usize, usize)>, String> {
        if data.source_rows == 0 || data.source_cols == 0 || data.cells.is_empty() {
            return Err("행/열 바꿈 복사 데이터가 비어 있습니다".to_string());
        }
        if data.cells.len() != data.source_rows as usize
            || data
                .cells
                .iter()
                .any(|row| row.len() != data.source_cols as usize)
        {
            return Err("행/열 바꿈 복사 데이터의 행/열 크기가 일치하지 않습니다".to_string());
        }

        let target_rows = data.source_cols;
        let target_cols = data.source_rows;
        let end_row = start_row
            .checked_add(target_rows - 1)
            .ok_or_else(|| "대상 행 범위가 너무 큽니다".to_string())?;
        let end_col = start_col
            .checked_add(target_cols - 1)
            .ok_or_else(|| "대상 열 범위가 너무 큽니다".to_string())?;
        self.validate_unmerged_rect(start_row, start_col, end_row, end_col)?;

        let mut changed = Vec::with_capacity((target_rows as usize) * (target_cols as usize));
        for source_row in 0..data.source_rows {
            for source_col in 0..data.source_cols {
                let target_row = start_row + source_col;
                let target_col = start_col + source_row;
                let cell_idx = self.cell_index_at(target_row, target_col).ok_or_else(|| {
                    format!("셀 ({},{})을 찾을 수 없습니다", target_row, target_col)
                })?;
                let mut paragraphs = data.cells[source_row as usize][source_col as usize].clone();
                if paragraphs.is_empty() {
                    paragraphs.push(Paragraph::new_empty());
                }
                let para_count = paragraphs.len();
                self.cells[cell_idx].paragraphs = paragraphs;
                changed.push((cell_idx, para_count));
            }
        }

        self.dirty = true;
        Ok(changed)
    }

    /// 병합 없는 전체 표를 제자리에서 전치한다.
    pub fn transpose_unmerged_table_in_place(&mut self) -> Result<Vec<(usize, usize)>, String> {
        if self.row_count == 0 || self.col_count == 0 {
            return Err("행/열을 바꿀 표가 비어 있습니다".to_string());
        }
        self.validate_unmerged_rect(0, 0, self.row_count - 1, self.col_count - 1)?;

        let source_rows = self.row_count;
        let source_cols = self.col_count;
        let source_cells = (0..source_rows)
            .map(|row| {
                (0..source_cols)
                    .map(|col| {
                        let cell_idx = self
                            .cell_index_at(row, col)
                            .ok_or_else(|| format!("셀 ({},{})을 찾을 수 없습니다", row, col))?;
                        Ok(self.cells[cell_idx].clone())
                    })
                    .collect::<Result<Vec<_>, String>>()
            })
            .collect::<Result<Vec<_>, String>>()?;

        let target_rows = source_cols;
        let target_cols = source_rows;
        let total_width: HwpUnit = self.get_column_widths().iter().sum();
        let target_widths = distribute_hwp_units(total_width.max(1800), target_cols);
        let row_height = self
            .get_row_heights()
            .into_iter()
            .max()
            .unwrap_or(400)
            .max(400);

        let mut cells = Vec::with_capacity((target_rows as usize) * (target_cols as usize));
        for target_row in 0..target_rows {
            for target_col in 0..target_cols {
                let mut cell = source_cells[target_col as usize][target_row as usize].clone();
                cell.row = target_row;
                cell.col = target_col;
                cell.row_span = 1;
                cell.col_span = 1;
                cell.width = target_widths[target_col as usize];
                cell.height = row_height;
                if cell.paragraphs.is_empty() {
                    cell.paragraphs.push(Paragraph::new_empty());
                }
                cells.push(cell);
            }
        }

        self.row_count = target_rows;
        self.col_count = target_cols;
        self.row_sizes = vec![target_cols as i16; target_rows as usize];
        self.cells = cells;
        self.zones.clear();
        self.update_ctrl_dimensions();
        self.rebuild_grid();
        self.dirty = true;

        Ok(self
            .cells
            .iter()
            .enumerate()
            .map(|(idx, cell)| (idx, cell.paragraphs.len()))
            .collect())
    }

    /// raw_ctrl_data 내 CommonObjAttr의 width/height를 재계산하여 갱신한다 (의도된 dual).
    ///
    /// **Dual maintenance 의 이유** (Picture/Shape 와 다른 점):
    /// - **Table**: `serializer/control.rs:461` 가 `table.raw_ctrl_data` 를 그대로 기록 →
    ///   `raw_ctrl_data` 가 **source-of-truth**. 본 함수가 cell 조절 후 갱신 필수.
    /// - **Picture/Shape**: `serializer/control.rs:895` 가 `&serialize_common_obj_attr(&pic.common)`
    ///   으로 매번 재생성 → `self.common` 이 source-of-truth, raw bytes 는 derived.
    ///
    /// Table 만 dual 인 것은 serializer 의 source-of-truth 정책 차이로 의도된 구조.
    /// 추후 모델 통일 (Picture/Shape 정합으로 Table 전환) 시 본 함수도 단순화 가능.
    ///
    /// raw_ctrl_data 레이아웃 (parse_common_obj_attr 정합):
    ///   [0..4] flags, [4..8] v_offset, [8..12] h_offset,
    ///   [12..16] width, [16..20] height, [20..24] z_order,
    ///   [24..32] outer_margin (i16×4), [32..36] instance_id
    pub fn update_ctrl_dimensions(&mut self) {
        let g = TableGrid::axes(self);
        let total_width: HwpUnit = g.col_x.last().copied().unwrap_or(0);
        let total_height: HwpUnit = g.row_y_eff.last().copied().unwrap_or(0);
        // (1) serialize source — raw_ctrl_data bytes (HWP 직렬화 시 사용). HWPX 로드 표는 raw 가 비어
        //     있다(parser/hwpx/section.rs `raw_ctrl_data: Vec::new()`) — 그때는 건너뛴다.
        //     [불변식 가드 2026-09-02] 종전엔 여기서 조기 반환해 (2)도 건너뛰었다 → HWPX 표는
        //     열 드래그·행 삽입·어긋내기 후에도 common.width/height 가 로드 시 값에 머물렀다.
        if self.raw_ctrl_data.len() >= common_obj_offsets::HEIGHT.end {
            self.raw_ctrl_data[common_obj_offsets::WIDTH]
                .copy_from_slice(&total_width.to_le_bytes());
            self.raw_ctrl_data[common_obj_offsets::HEIGHT]
                .copy_from_slice(&total_height.to_le_bytes());
        }
        // (2) [Task #1151 v6] paragraph_layout cache — self.common.width/height. 항상 갱신.
        // v3 helper (calc_sibling_topandbottom_table_reserved_hu) 가 self.common.height 사용.
        // dual maintenance 가 필수 — 한쪽만 갱신 시 stale 결함(S4 불변식이 잡는다).
        self.common.width = total_width;
        self.common.height = total_height;
    }

    fn sync_ctrl_height(&mut self, height: HwpUnit) {
        self.common.height = height;
        if self.raw_ctrl_data.len() >= common_obj_offsets::HEIGHT.end {
            self.raw_ctrl_data[common_obj_offsets::HEIGHT].copy_from_slice(&height.to_le_bytes());
        }
    }

    fn stretched_row_heights(&self) -> Option<Vec<HwpUnit>> {
        let mut heights = self.get_row_heights();
        let raw_sum: u64 = heights.iter().map(|h| *h as u64).sum();
        let target = self.common.height as u64;
        if heights.is_empty() || raw_sum == 0 || target <= raw_sum {
            return None;
        }

        let mut scaled_sum = 0u64;
        for height in &mut heights {
            let scaled = ((*height as u64 * target) + raw_sum / 2) / raw_sum;
            *height = scaled.max(1).min(u32::MAX as u64) as HwpUnit;
            scaled_sum += *height as u64;
        }

        if let Some(last) = heights.last_mut() {
            match target.cmp(&scaled_sum) {
                std::cmp::Ordering::Greater => {
                    let delta = (target - scaled_sum).min(u32::MAX as u64);
                    *last = last.saturating_add(delta as HwpUnit);
                }
                std::cmp::Ordering::Less => {
                    let delta = (scaled_sum - target).min(*last as u64);
                    *last = last.saturating_sub(delta as HwpUnit).max(1);
                }
                std::cmp::Ordering::Equal => {}
            }
        }

        Some(heights)
    }

    /// 열별 폭을 추출한다 (col_span==1인 셀 기준).
    /// TAC(글자처럼 취급) 표가 차지하는 **줄 높이**(HWPUNIT) = 표 높이 + 바깥여백 상하.
    ///
    /// [구조 정리 2026-08-12] 같은 식이 height_measurer·layout·typeset 4곳에 복제돼
    /// 있었다. 한 곳만 고치면 "표 줄인가?" 판정이 지점마다 갈린다(TAC 회귀의 단골 원인).
    /// i64 연산 그대로라 값은 종전과 동일하다.
    pub fn tac_line_height_hu(&self) -> i64 {
        self.common.height as i64 + self.outer_margin_top as i64 + self.outer_margin_bottom as i64
    }

    /// 열별 폭 — 격자 뷰(`TableGrid::col_widths`: span1 max → 스팬 제약 해소 → 0 은 1800).
    pub fn get_column_widths(&self) -> Vec<HwpUnit> {
        TableGrid::axes(self).col_widths
    }

    /// 열별 폭(HWPUNIT)을 절대값으로 설정한다.
    ///
    /// `widths.len()` 은 `col_count` 와 같아야 한다. 병합 셀(`col_span > 1`)은
    /// 걸친 열들의 폭 합으로 설정된다. 설정 후 표 전체 크기
    /// (`update_ctrl_dimensions`)와 그리드 인덱스(`rebuild_grid`)를 갱신한다.
    ///
    /// `insert_column` 이 기준 열 폭을 복제해 표를 넓히는 것과 달리, 이 메서드는
    /// 입력한 폭들의 합이 그대로 표 전체 폭이 된다. 페이지 폭에 맞추려면
    /// 합이 본문 폭 이하가 되도록 전달한다.
    pub fn set_column_widths(&mut self, widths: &[HwpUnit]) -> Result<(), String> {
        if widths.len() != self.col_count as usize {
            return Err(format!(
                "열 폭 개수 {} 가 표의 열 수 {} 와 다릅니다",
                widths.len(),
                self.col_count
            ));
        }
        for cell in &mut self.cells {
            let c = cell.col as usize;
            if c >= widths.len() {
                continue;
            }
            let end = (c + cell.col_span as usize).min(widths.len());
            cell.width = widths[c..end].iter().sum();
        }
        self.update_ctrl_dimensions();
        self.rebuild_grid();
        Ok(())
    }

    /// 행별 **실효** 높이 — 저장 행높이에 글줄 바닥(패딩 상하 + 1000HU)을 적용한다.
    ///
    /// 한컴 저장 규약: 빈 셀의 `cell.height` 는 **패딩만**이다(한컴 저장 실물
    /// officex_tac_mid_anchor.hwpx: 셀 284 = 142+142, common.height 2568 = 1284×2).
    /// 저장 합산만으로 `common.height` 를 만들면 표가 글줄만큼 납작해져, 열폭
    /// 조절 등 update_ctrl_dimensions 를 타는 순간 host lineseg 가 따라 무너지고
    /// TAC 옆 텍스트가 표 상단에 떠 보였다(2026-08-11 신고의 뿌리).
    /// ponytail: 글줄 바닥은 10pt 기준 1000HU 고정 — Table 은 doc_info(폰트)를
    /// 모르며, 큰 글자 셀은 편집 경로가 stored 높이를 이미 키워 max 가 지켜진다.
    /// 행별 **글줄 바닥**(HU) — max(패딩+10pt 하한, 행 내 셀 콘텐츠 바닥(lineseg 실측)).
    /// 조각 행(걸침 span 셀이 있는 행)은 0 — 저장 높이가 곧 실효라 바닥이 없다.
    /// effective_row_heights 와 resize 의 raw 델타 밑절미가 공유하는 단일 근거.
    /// ⚠ 다른 셀의 **저장** 높이는 포함하지 않는다 — 보상(±d) 조절이 있는 실파일에서
    /// 저장 높이까지 밑절미로 끌어올리면 행 max 가 커져 표가 자란다(issue_493 실측).
    /// 이 격자선(행 축)이 **어긋난 선**인가 — 내부 선인데 한 열만 경계로 쓴다. → `TableGrid::is_misaligned`.
    pub fn is_misaligned_line(&self, line: u16) -> bool {
        line > 0 && line < self.row_count && table_grid::line_info(self, Axis::Rows, line).misaligned()
    }

    /// 이 행이 **어긋내기 조각 행**인가 — 인접 선이 어긋선이고 행의 span1 셀이 모두 명시 높이.
    /// 조각 행은 저장 높이가 곧 실효 높이라 글줄 바닥·빈 lineseg 성장을 면제한다. → `TableGrid::is_piece_row`.
    /// 셀별 루프(렌더러 성장 면제)에서 불리므로 선 하나씩 O(cells) 로 판정한다 — 격자 전체 구성 금지.
    pub fn is_stagger_piece_row(&self, row: usize) -> bool {
        self.col_count >= 2
            && (self.is_misaligned_line(row as u16) || self.is_misaligned_line(row as u16 + 1))
            && table_grid::span1_cells_explicit(self, row)
    }

    /// 이 격자선(행 축)이 **부분 공유선**인가 — 일부 열은 경계로 쓰고 일부는 관통. → `TableGrid::is_partially_shared`.
    pub fn is_partially_shared_line(&self, line: u16) -> bool {
        line > 0
            && line < self.row_count
            && table_grid::line_info(self, Axis::Rows, line).partially_shared()
    }

    /// 어긋내기 **합류 산물 행**인가 — 인접 선이 부분 공유선. → `TableGrid::is_joined_row`.
    pub fn is_stagger_joined_row(&self, row: usize) -> bool {
        self.col_count >= 2
            && (self.is_partially_shared_line(row as u16)
                || self.is_partially_shared_line(row as u16 + 1))
    }

    /// 행별 **글줄 바닥**(HU) — `TableGrid::row_floors`(조각 행·명시 관통 행은 0).
    /// effective_row_heights 와 resize 의 raw 델타 밑절미가 공유하는 단일 근거.
    pub fn row_line_floors_hu(&self) -> Vec<u32> {
        TableGrid::axes(self).row_floors
    }

    /// 행별 **실효** 높이 = max(저장 행높이, 글줄 바닥) — `TableGrid::row_heights_eff`.
    pub fn effective_row_heights(&self) -> Vec<HwpUnit> {
        TableGrid::axes(self).row_heights_eff
    }

    /// [2026-08-16 → 11-a 양축 일반화] 선 `k`..`k+1` 사이 `off`(HU) 지점에 **새 격자선**을 삽입한다.
    ///
    /// 신규 어긋내기의 낙하점이 스팬 이웃의 내부 단위 구간에 떨어질 때 쓴다 — 종전
    /// split_cell_into(균등 분할)로는 낙하점 위치의 선을 만들 수 없어, 이웃이 다른
    /// 줄의 어긋 조각에 걸친 스팬이면 격자가 모순돼 트랜잭션이 거부했다(신고
    /// "오른쪽 어긋내기가 기준(첫 어긋선)을 못 넘어간다").
    ///
    /// 규약: `band`(직교 축 (start, span)) 안에서 k 에서 시작하는 span1 셀만 (off, size−off)
    /// 두 조각(빈 조각 + 내용 조각, `content_far` 면 내용은 **뒤** 조각 — "내용은 잔여에" 규약)
    /// 으로 실제 분할하고, k 를 덮는 나머지 셀은 전부 span+1 로 관통시킨다 — 어긋선(한 줄만 쓰는
    /// 내부 선) 모델 유지. 전 줄을 쪼개면 빈 조각 행에 글줄 바닥이 되살아나 실효 높이가 커진다.
    /// 행 축은 호출 전 연루 행을 물질화할 것(원시 284 규약 상태로 나누면 장부가 갈린다).
    /// 빈 조각은 `Cell::new_from_template`(문단 모양 유지) — 열 경로의 split_cell_into 와 같다.
    pub(crate) fn insert_line(
        &mut self,
        axis: Axis,
        k: u16,
        band: (u16, u16),
        off: HwpUnit,
        content_far: bool,
    ) -> Result<(), String> {
        if off == 0 {
            return Err("분할 오프셋이 0 이하".to_string());
        }
        let band = band.0..band.0 + band.1;
        let g = TableGrid::axes(self);
        let mut new_cells: Vec<Cell> = Vec::with_capacity(self.cells.len() + 1);
        for c in &self.cells {
            let (start, span, size) = axis_of(c, axis);
            let (ortho, _) = ortho_of(c, axis);
            if start > k {
                let mut c2 = c.clone();
                *axis_mut(&mut c2, axis).0 += 1;
                new_cells.push(c2);
            } else if start <= k && start + span > k && band.contains(&ortho) {
                // 밴드에서 k 를 덮는 셀(줄어드는 쪽) — 새 선 k+1 에서 실제 이분할해 두 조각 크기를 못박는다.
                // 관통만 시키면 새 선 위치가 격자에서 미결정(span1 목격자 없음)이라 솔버가 임의 해를 고른다.
                let size_a = g.interval_sum(axis, start, k).saturating_add(off);
                if size <= size_a {
                    return Err("분할 오프셋이 칸 크기 이상".to_string());
                }
                let (a, b) =
                    split_pieces(c, axis, k + 1, k + 1 - start, start + span - k, size_a, size - size_a, content_far);
                new_cells.push(a);
                new_cells.push(b);
            } else if start + span > k {
                // k 를 덮는 나머지 셀(다른 줄/스팬) — 관통 스팬으로
                let mut c2 = c.clone();
                *axis_mut(&mut c2, axis).1 += 1;
                new_cells.push(c2);
            } else {
                new_cells.push(c.clone());
            }
        }
        self.cells = new_cells;
        match axis {
            Axis::Cols => self.col_count += 1,
            Axis::Rows => self.row_count += 1,
        }
        self.finish_cells();
        Ok(())
    }

    /// 스팬 셀 `idx` 를 **기존** 내부 선 `line` 에서 이분할한다. 두 조각 크기 = 격자 구간 합 정확값,
    /// 내용은 `content_far` 면 뒤 조각, 아니면 앞 조각(다른 조각은 빈 템플릿 셀). 반환 = (앞, 뒤) 인덱스.
    pub(crate) fn split_cell_at_line(
        &mut self,
        idx: usize,
        axis: Axis,
        line: u16,
        content_far: bool,
    ) -> Result<(usize, usize), String> {
        let c = self
            .cells
            .get(idx)
            .ok_or_else(|| "셀 인덱스가 유효하지 않습니다".to_string())?
            .clone();
        let (start, span, _) = axis_of(&c, axis);
        if line <= start || line >= start + span {
            return Err(format!("선 {line} 은 셀 ({},{}) 의 내부 선이 아닙니다", c.row, c.col));
        }
        let g = TableGrid::axes(self);
        let (a, b) = split_pieces(
            &c,
            axis,
            line,
            line - start,
            start + span - line,
            g.interval_sum(axis, start, line),
            g.interval_sum(axis, line, start + span),
            content_far,
        );
        let (ka, kb) = ((a.row, a.col), (b.row, b.col));
        self.cells[idx] = a;
        self.cells.push(b);
        self.finish_cells();
        let ia = self.cell_index_at(ka.0, ka.1).ok_or("분할 조각(앞) 소실")?;
        let ib = self.cell_index_at(kb.0, kb.1).ok_or("분할 조각(뒤) 소실")?;
        Ok((ia, ib))
    }

    /// 셀 `cell_idx` 의 **끝선**을 `from` → `to` 로 옮긴다 — 합류·복원·통과·전달의 공통 연산.
    /// 사이 슬롯은 같은 직교 밴드의 이웃(from 에서 시작하는 셀)과 주고받는다: 크는 쪽이 흡수하는
    /// 조각은 `split_cell_at_line`(내용은 옮기는 선에서 먼 조각) 로 잘라 낸 **빈 조각**이라 문단은
    /// 절대 이동하지 않고, 크는 셀은 자기 속성·문단을 그대로 둔다(merge_cells 의 primary 규칙·선두
    /// 빈 문단 제거가 필요 없다). 두 셀 크기 = 절차 상단 격자의 구간 합 정확값. 이웃 전체 흡수
    /// (셀 수 감소)는 거부한다.
    pub(crate) fn join_to_line(
        &mut self,
        cell_idx: usize,
        axis: Axis,
        from: u16,
        to: u16,
    ) -> Result<(), String> {
        let c = self
            .cells
            .get(cell_idx)
            .ok_or_else(|| "셀 인덱스가 유효하지 않습니다".to_string())?
            .clone();
        let (start, span, _) = axis_of(&c, axis);
        if start + span != from {
            return Err(format!("셀 ({},{}) 의 끝선은 {} 이지 {from} 가 아닙니다", c.row, c.col, start + span));
        }
        if to == from {
            return Ok(());
        }
        let n_idx = match axis {
            Axis::Cols => self.cell_index_at(c.row, from),
            Axis::Rows => self.cell_index_at(from, c.col),
        }
        .ok_or_else(|| {
            if axis == Axis::Cols { "오른쪽 이웃 셀을 찾지 못했습니다" } else { "아래 이웃 셀을 찾지 못했습니다" }
                .to_string()
        })?;
        let n = self.cells[n_idx].clone();
        if ortho_of(&n, axis) != ortho_of(&c, axis) {
            return Err(if axis == Axis::Cols {
                "위아래 높이가 다른 칸과는 경계를 어긋낼 수 없습니다"
            } else {
                "좌우 폭이 다른 칸과는 경계를 어긋낼 수 없습니다"
            }
            .to_string());
        }
        let (n_start, n_span, _) = axis_of(&n, axis);
        let n_end = n_start + n_span;
        if to > n_end || to < start {
            return Err(if to > from {
                "낙하점이 이웃 칸을 넘어 셀을 삼키게 됩니다"
            } else {
                "낙하점이 대상 칸의 시작선을 넘습니다"
            }
            .to_string());
        }
        let has_text = |c: &Cell| c.paragraphs.iter().any(|p| p.text.chars().any(|ch| !ch.is_whitespace()));
        let g = TableGrid::axes(self);
        // (grower 키, shrinker 키(조각 전체 흡수면 None), 두 크기) — 절차 상단 격자의 구간 합.
        let (grow_key, shrink_key, grow_size, shrink_size) = if to > from {
            let sk = (to < n_end).then(|| key_at(axis, &n, to));
            ((c.row, c.col), sk, g.interval_sum(axis, start, to), g.interval_sum(axis, to, n_end))
        } else {
            let sk = (to > start).then_some((c.row, c.col));
            ((n.row, n.col), sk, g.interval_sum(axis, to, n_end), g.interval_sum(axis, start, to))
        };
        // 흡수 조각 = 잘라 낸 빈 조각(내용은 옮기는 선에서 먼 쪽에 남는다). 이웃/대상 **전체**는
        // 빈 조각(insert_line 산물)일 때만 흡수 — 내용 있는 셀을 삼키지 않는다(셀 수 규칙).
        let frag_key = if to > from {
            if to == n_end {
                if has_text(&n) {
                    return Err("낙하점이 이웃 칸을 넘어 셀을 삼키게 됩니다".to_string());
                }
                (n.row, n.col)
            } else {
                let (frag, _rest) = self.split_cell_at_line(n_idx, axis, to, true)?;
                (self.cells[frag].row, self.cells[frag].col)
            }
        } else if to == start {
            if has_text(&c) {
                return Err("낙하점이 대상 칸의 시작선을 넘습니다".to_string());
            }
            (c.row, c.col)
        } else {
            let (_keep, frag) = self.split_cell_at_line(cell_idx, axis, to, false)?;
            (self.cells[frag].row, self.cells[frag].col)
        };
        let frag_idx = self.cells.iter().position(|x| (x.row, x.col) == frag_key).ok_or("흡수 조각 소실")?;
        self.cells.remove(frag_idx);
        let gi = self.cells.iter().position(|x| (x.row, x.col) == grow_key).ok_or("합류 대상 소실")?;
        let (s, sp, sz) = axis_mut(&mut self.cells[gi], axis);
        if to > from {
            *sp = to - start;
        } else {
            *s = to;
            *sp = n_end - to;
        }
        *sz = grow_size;
        if let Some(shrink_key) = shrink_key {
            let si = self.cells.iter().position(|x| (x.row, x.col) == shrink_key).ok_or("합류 대상 소실")?;
            *axis_mut(&mut self.cells[si], axis).2 = shrink_size;
        }
        self.finish_cells();
        Ok(())
    }

    /// 프리미티브 공통 마무리 — 셀이 곧 진실: 정렬 → row_sizes → 격자 인덱스 → 표 치수.
    fn finish_cells(&mut self) {
        self.cells.sort_by_key(|c| (c.row, c.col));
        self.rebuild_row_sizes();
        self.rebuild_grid();
        self.update_ctrl_dimensions();
    }

    /// 어긋내기 등 **행 구조 편집 전 물질화** — [from..=to] 행의 span==1 셀 높이를 실효
    /// 높이(글줄 바닥 반영)로 확정한다. 빈 셀 저장 규약(패딩만 284)의 원시 값으로 행을
    /// 자르면 화면(바닥 1484)과 다른 세계에서 산술이 돌아 몰래 어긋나거나 다른 행
    /// 경계까지 밀린다(2026-08-13 신고). 편집에 연루된 행만 명시값으로 굳힌다.
    pub fn materialize_rows_effective(&mut self, from_row: usize, to_row: usize) {
        let eff = self.effective_row_heights();
        for r in from_row..=to_row {
            let Some(&row_eff) = eff.get(r) else { continue };
            for c in self.cells.iter_mut() {
                if c.row as usize == r && c.row_span <= 1 && c.height < row_eff {
                    c.height = row_eff;
                }
            }
        }
    }

    /// 셀별 **행 축소 한계**(HU) = 콘텐츠 글줄 범위(전 문단 lineseg 하단 최대) + 상하 패딩.
    ///
    /// 한컴 규약: 행은 글줄 밑으로 줄어들지 않는다. 측정기(height_measurer)가 행높이를
    /// max(기록, 콘텐츠+패딩)로 바닥 치는 것과 같은 근거 — 축소 클램프가 이 값보다 작게
    /// 허용하면 셀 격자(기록값)와 표 상자(측정 바닥)가 어긋나는 유령 공간이 생긴다
    /// (2026-08-12 실측 +2.7px). 인덱스 = cellIdx(= cells 순서).
    pub fn cell_content_floors_hu(&self) -> Vec<u32> {
        self.cells
            .iter()
            .map(|c| {
                let content: i32 = c
                    .paragraphs
                    .iter()
                    .flat_map(|p| p.line_segs.iter())
                    .map(|s| s.vertical_pos.saturating_add(s.line_height))
                    .max()
                    .unwrap_or(0);
                let p = c.effective_padding(&self.padding);
                (content.max(0) as u32) + (p.top.max(0) + p.bottom.max(0)) as u32
            })
            .collect()
    }

    /// [2026-08-16 신고 "3x3 전 경계 어긋내기 개판"] 어긋내기 **조각의 최소 크기**.
    ///
    /// 내용이 있는 조각은 글줄 바닥(내용이 잘리면 안 됨), **빈 조각은 MIN_CELL(200)** —
    /// 열 어긋내기와 대칭이다. 종전엔 빈 조각에도 글줄 바닥(1284)을 요구해, 모든 행이
    /// 바닥값인 신선한 표에서는 행 어긋내기가 어느 방향으로도 수학적으로 불가능했다
    /// (마우스·키보드 전 조합 무동작 실측). 빈 조각이 글줄보다 얇아지면 렌더는 셀
    /// 클립으로 처리한다(조각 행은 effective_row_heights 의 걸침 예외라 재부풀지 않는다).
    fn stagger_piece_floor_hu(&self, idx: usize, floors: &[u32], min_cell: i32) -> i32 {
        let has_text = self
            .cells
            .get(idx)
            .map(|c| {
                c.paragraphs
                    .iter()
                    .any(|p| p.text.chars().any(|ch| !ch.is_whitespace()))
            })
            .unwrap_or(false);
        if has_text {
            floors.get(idx).copied().unwrap_or(0).max(min_cell as u32) as i32
        } else {
            min_cell
        }
    }

    /// 행별 높이를 추출한다 (row_span==1인 셀 기준).
    /// 높이가 0인 행은 기본값 400으로 대체 (새 셀 생성용).
    pub fn get_row_heights(&self) -> Vec<HwpUnit> {
        TableGrid::axes(self).row_heights_stored
    }

    /// 행별 높이를 추출한다 (fallback 없이 원본 값 그대로).
    /// 병합 시 원본 height=0 (자동 맞춤) 보존용.
    pub fn get_raw_row_heights(&self) -> Vec<HwpUnit> {
        let mut heights = vec![0u32; self.row_count as usize];
        for cell in &self.cells {
            if cell.row_span == 1 && (cell.row as usize) < heights.len() {
                if cell.height > heights[cell.row as usize] {
                    heights[cell.row as usize] = cell.height;
                }
            }
        }
        heights
    }

    /// row_sizes를 행별 실제 셀 개수로 재계산한다.
    pub(crate) fn rebuild_row_sizes(&mut self) {
        self.row_sizes = (0..self.row_count)
            .map(|r| self.cells.iter().filter(|c| c.row == r).count() as i16)
            .collect();
    }

    /// 행을 삽입한다.
    ///
    /// `row_idx`: 기준 행 인덱스, `below`: true면 아래에, false면 위에 삽입.
    /// 반환: Ok(()) 또는 에러 메시지.
    pub fn insert_row(&mut self, row_idx: u16, below: bool) -> Result<(), String> {
        if row_idx >= self.row_count {
            return Err(format!(
                "행 인덱스 {} 범위 초과 (총 {}행)",
                row_idx, self.row_count
            ));
        }

        let stretched_new_row_height = self
            .stretched_row_heights()
            .and_then(|heights| heights.get(row_idx as usize).copied());
        let original_height = self.common.height;
        let target_row = if below { row_idx + 1 } else { row_idx };
        let col_widths = self.get_column_widths();

        // 셀 height용
        let row_heights = self.get_row_heights();
        let new_cell_height: HwpUnit = if (row_idx as usize) < row_heights.len() {
            row_heights[row_idx as usize]
        } else {
            400
        };

        // 새 행이 복사할 칸 모양 = 기준 행(row_idx)을 점유하되 삽입 지점을 걸치지 않는 셀의 (col, col_span, width).
        // 걸치는 셀은 아래에서 row_span 확장. 한컴은 마지막 셀 Tab·줄 삽입 모두 기준 행의 칸 모양을 그대로
        // 복사한다 — 격자 열마다 셀을 만들면 어긋낸 표(격자가 갈라진 표)에서 실오라기 칸이 생긴다 (2026-09-06).
        let shape: Vec<(u16, u16, HwpUnit)> = self
            .cells
            .iter()
            .filter(|c| c.row <= row_idx && row_idx < c.row + c.row_span)
            .filter(|c| !(c.row < target_row && c.row + c.row_span > target_row))
            .map(|c| (c.col, c.col_span, c.width))
            .collect();

        // 병합 셀 확장 + 기존 셀 시프트 (커버리지 맵 생성용으로 먼저 처리)
        // 삽입 지점을 걸치는 병합 셀 추적
        let mut covered_cols = vec![false; self.col_count as usize];

        for cell in &mut self.cells {
            // 병합 셀이 삽입 지점을 걸치는 경우: row_span 확장
            if cell.row < target_row && cell.row + cell.row_span > target_row {
                cell.row_span += 1;
                // 이 셀이 커버하는 열 표시
                for c in cell.col..(cell.col + cell.col_span).min(self.col_count) {
                    covered_cols[c as usize] = true;
                }
            }
            // target_row 이상의 셀은 1행 아래로 시프트
            if cell.row >= target_row {
                cell.row += 1;
            }
        }

        // 새 셀 목록: 기준 행 칸 모양 복사 → 그래도 비는 열(S1 위반 표)만 격자 열 단위로 채움
        let mut new_cells: Vec<(u16, u16, HwpUnit)> = Vec::new();
        for &(col, span, width) in &shape {
            let end = (col + span).min(self.col_count);
            if (col..end).any(|c| covered_cols[c as usize]) {
                continue;
            }
            for c in col..end {
                covered_cols[c as usize] = true;
            }
            new_cells.push((col, span, width));
        }
        for c in 0..self.col_count {
            if !covered_cols[c as usize] {
                new_cells.push((c, 1, col_widths[c as usize]));
            }
        }

        // 서식 템플릿: 삽입 지점 아래 행의 셀을 우선 사용 (헤더 행 대신 데이터 행)
        // target_row 아래(+1)의 셀이 원래 데이터 행이므로 먼저 시도, 없으면 위(-1), 그래도 없으면 아무 셀
        for (c, span, width) in new_cells {
            let template = self
                .cells
                .iter()
                .find(|cell| cell.col == c && cell.col_span == 1 && cell.row == target_row + 1)
                .or_else(|| {
                    if target_row > 0 {
                        self.cells.iter().find(|cell| {
                            cell.col == c && cell.col_span == 1 && cell.row == target_row - 1
                        })
                    } else {
                        None
                    }
                })
                .or_else(|| {
                    self.cells
                        .iter()
                        .find(|cell| cell.col == c && cell.col_span == 1)
                })
                // 열 c 가 전부 병합 셀이면 위 탐색이 모두 실패한다. 서식 0 짜리 셀을
                // 만드느니 표의 아무 셀이나 템플릿으로 쓴다 (주석의 "아무 셀").
                .or_else(|| self.cells.first());
            let mut new_cell = if let Some(tpl) = template {
                Cell::new_from_template(c, target_row, width, new_cell_height, tpl)
            } else {
                // 셀이 하나도 없는 표 — 상속원이 존재하지 않는 유일한 경우
                Cell::new_empty(c, target_row, width, new_cell_height, self.border_fill_id)
            };
            new_cell.col_span = span;
            self.cells.push(new_cell);
        }

        // row_count 갱신 및 row_sizes 재계산 (행별 셀 개수)
        self.row_count += 1;
        self.rebuild_row_sizes();

        // 행 우선 순서 정렬
        self.cells.sort_by_key(|c| (c.row, c.col));

        // CommonObjAttr 크기 갱신
        self.update_ctrl_dimensions();
        if let Some(new_row_height) = stretched_new_row_height {
            // 일반 표는 셀 저장 height보다 큰 표시 height를 별도로 가진다.
            // 행 추가 시 표시 기준 행 높이를 더해 표가 납작해지지 않도록 보존한다.
            self.sync_ctrl_height(original_height.saturating_add(new_row_height));
        }

        // 그리드 인덱스 재구축
        self.rebuild_grid();

        Ok(())
    }

    /// 열을 삽입한다.
    ///
    /// `col_idx`: 기준 열 인덱스, `right`: true면 오른쪽에, false면 왼쪽에 삽입.
    pub fn insert_column(&mut self, col_idx: u16, right: bool) -> Result<(), String> {
        if col_idx >= self.col_count {
            return Err(format!(
                "열 인덱스 {} 범위 초과 (총 {}열)",
                col_idx, self.col_count
            ));
        }

        let original_height = self.common.height;
        let target_col = if right { col_idx + 1 } else { col_idx };
        let col_widths = self.get_column_widths();
        let row_heights = self.get_row_heights();
        let new_col_width = col_widths[col_idx as usize];
        // [officex] 삽입 전 표 전체 폭 — 아래에서 이 값으로 되맞춘다.
        // 한컴은 열을 넣어도 표 폭을 보존하고 기존 열에서 폭을 나눠 온다.
        let original_total_width: u64 = col_widths.iter().map(|w| *w as u64).sum();

        // 병합 셀 확장 + 기존 셀 시프트
        let mut covered_rows = vec![false; self.row_count as usize];

        for cell in &mut self.cells {
            // 병합 셀이 삽입 지점을 걸치는 경우: col_span 확장
            if cell.col < target_col && cell.col + cell.col_span > target_col {
                cell.col_span += 1;
                cell.width += new_col_width;
                // 이 셀이 커버하는 행 표시
                for r in cell.row..(cell.row + cell.row_span).min(self.row_count) {
                    covered_rows[r as usize] = true;
                }
            }
            // target_col 이상의 셀은 1열 오른쪽으로 시프트
            if cell.col >= target_col {
                cell.col += 1;
            }
        }

        // 새 셀 생성: 병합 셀에 의해 커버되지 않는 행에만
        // 삽입 지점 오른쪽 열의 셀을 템플릿으로 우선 사용, 없으면 왼쪽, 그래도 없으면 아무 셀
        for r in 0..self.row_count {
            if !covered_rows[r as usize] {
                let height = row_heights[r as usize];
                let template = self
                    .cells
                    .iter()
                    .find(|cell| cell.row == r && cell.row_span == 1 && cell.col == target_col + 1)
                    .or_else(|| {
                        if target_col > 0 {
                            self.cells.iter().find(|cell| {
                                cell.row == r && cell.row_span == 1 && cell.col == target_col - 1
                            })
                        } else {
                            None
                        }
                    })
                    .or_else(|| {
                        self.cells
                            .iter()
                            .find(|cell| cell.row == r && cell.row_span == 1)
                    })
                    // 행 r 이 전부 병합 셀이면 위 탐색이 모두 실패한다. 서식 0 짜리 셀을
                    // 만드느니 표의 아무 셀이나 템플릿으로 쓴다 (주석의 "아무 셀").
                    .or_else(|| self.cells.first());
                let new_cell = if let Some(tpl) = template {
                    Cell::new_from_template(target_col, r, new_col_width, height, tpl)
                } else {
                    // 셀이 하나도 없는 표 — 상속원이 존재하지 않는 유일한 경우
                    Cell::new_empty(target_col, r, new_col_width, height, self.border_fill_id)
                };
                self.cells.push(new_cell);
            }
        }

        // col_count 갱신 및 row_sizes 재계산 (행별 셀 개수)
        self.col_count += 1;
        self.rebuild_row_sizes();

        // 행 우선 순서 정렬
        self.cells.sort_by_key(|c| (c.row, c.col));

        // [officex] 새 열 폭을 그냥 더하면 표가 본문·용지 밖으로 나간다
        // (QA "열 삽입: 폭 배분" — 559.4→745.8px, 오른쪽 끝이 쪽 폭 793.7 초과).
        // 한컴처럼 표 전체 폭을 보존한다: 새 열 포함 전 열을 원래 총폭 비율로 축소하고
        // 내림 잔여분은 가장 넓은 열에 몰아 합을 정확히 되돌린다. MIN_CELL 바닥에
        // 닿으면 더 줄이지 않는다 — 열이 지나치게 많을 때만 폭이 조금 는다(가독성 우선).
        if original_total_width > 0 {
            let mut widths = self.get_column_widths();
            let grown: u64 = widths.iter().map(|w| *w as u64).sum();
            if grown > original_total_width {
                for w in &mut widths {
                    let scaled = (*w as u64 * original_total_width) / grown;
                    *w = (scaled as u32).max(Self::MIN_CELL as u32);
                }
                let assigned: u64 = widths.iter().map(|w| *w as u64).sum();
                if assigned < original_total_width {
                    let delta = (original_total_width - assigned).min(u32::MAX as u64) as u32;
                    if let Some(w) = widths.iter_mut().max_by_key(|w| **w) {
                        *w = w.saturating_add(delta);
                    }
                }
                // set_column_widths 가 병합 셀 폭(걸친 열 폭 합)까지 정합하게 다시 쓴다.
                self.set_column_widths(&widths)?;
            }
        }

        // CommonObjAttr 크기 갱신
        self.update_ctrl_dimensions();
        if original_height > 0 {
            // 열 추가는 행 수를 바꾸지 않으므로 표 외곽 높이는 기존 값을 유지한다.
            self.common.height = original_height;
            if self.raw_ctrl_data.len() >= common_obj_offsets::HEIGHT.end {
                self.raw_ctrl_data[common_obj_offsets::HEIGHT]
                    .copy_from_slice(&original_height.to_le_bytes());
            }
        }

        // 그리드 인덱스 재구축
        self.rebuild_grid();

        Ok(())
    }

    /// 행을 삭제한다.
    ///
    /// `row_idx`: 삭제할 행 인덱스. 최소 1행은 유지 (row_count == 1이면 에러).
    pub fn delete_row(&mut self, row_idx: u16) -> Result<(), String> {
        if row_idx >= self.row_count {
            return Err(format!(
                "행 인덱스 {} 범위 초과 (총 {}행)",
                row_idx, self.row_count
            ));
        }
        if self.row_count <= 1 {
            return Err("최소 1행은 유지해야 합니다".to_string());
        }

        let stretched_deleted_row_height = self
            .stretched_row_heights()
            .and_then(|heights| heights.get(row_idx as usize).copied());
        let original_height = self.common.height;

        // [table-layout/삭제-세로병합] 삭제 행의 단일-행 높이(형제 row_span==1 셀 기준)를
        // 미리 잡아 둔다. 세로 병합 셀은 걸친 행 수만큼 height 를 합산해 갖고 있는데,
        // 걸친 행이 사라져 row_span 이 줄면 그만큼 height 도 줄여야 남은 행 높이가 실제
        // 행 수에 비례한다. 안 줄이면 병합 셀이 2행치 높이를 그대로 물고 있어 행은
        // 줄었는데 표 전체 높이(=행 높이 합)는 삭제 전과 같아지는 결함이 난다.
        // [2026-09-06] raw(빈 칸 규약 284)가 아니라 **실효** 높이를 뺀다 — 병합 칸은 실효 합을 물려받으므로
        // raw 를 빼면 남은 행이 지워진 행 높이만큼 부풀었다(2568 − 284 = 2284 ≠ 1284).
        let deleted_row_height = self
            .effective_row_heights()
            .get(row_idx as usize)
            .copied()
            .unwrap_or(0);

        // 삭제 행을 걸치는 병합 셀: row_span 축소 + 걸친 행 높이만큼 height 축소
        for cell in &mut self.cells {
            if cell.row < row_idx && cell.row + cell.row_span > row_idx {
                cell.row_span -= 1;
                if cell.height >= deleted_row_height {
                    cell.height -= deleted_row_height;
                }
            }
        }

        // 삭제 대상 행의 셀 제거 (해당 행에 앵커가 있고 row_span==1인 셀)
        self.cells
            .retain(|cell| !(cell.row == row_idx && cell.row_span == 1));

        // 삭제 행에 앵커가 있지만 row_span > 1인 병합 셀: 다음 행으로 이동, row_span 축소
        // 앵커 행이 사라지므로 그 행 높이만큼 병합 셀 height 도 함께 줄인다.
        for cell in &mut self.cells {
            if cell.row == row_idx && cell.row_span > 1 {
                cell.row_span -= 1;
                if cell.height >= deleted_row_height {
                    cell.height -= deleted_row_height;
                }
            }
        }

        // 삭제 행 아래 셀: row -= 1
        for cell in &mut self.cells {
            if cell.row > row_idx {
                cell.row -= 1;
            }
        }

        // row_count 갱신 및 row_sizes 재계산
        self.row_count -= 1;
        self.rebuild_row_sizes();

        // 행 우선 순서 정렬
        self.cells.sort_by_key(|c| (c.row, c.col));

        // CommonObjAttr 크기 갱신
        self.update_ctrl_dimensions();
        if let Some(deleted_row_height) = stretched_deleted_row_height {
            // 일반 표의 표시 높이는 셀 저장 height 합보다 크므로 삭제 행의 표시
            // 높이만큼 외곽 height를 줄여 한컴식 비례를 유지한다.
            let raw_sum: HwpUnit = self.get_row_heights().iter().sum();
            self.sync_ctrl_height(
                original_height
                    .saturating_sub(deleted_row_height)
                    .max(raw_sum),
            );
        }

        // 그리드 인덱스 재구축
        self.rebuild_grid();

        Ok(())
    }

    /// 열을 삭제한다.
    ///
    /// `col_idx`: 삭제할 열 인덱스. 최소 1열은 유지 (col_count == 1이면 에러).
    pub fn delete_column(&mut self, col_idx: u16) -> Result<(), String> {
        if col_idx >= self.col_count {
            return Err(format!(
                "열 인덱스 {} 범위 초과 (총 {}열)",
                col_idx, self.col_count
            ));
        }
        if self.col_count <= 1 {
            return Err("최소 1열은 유지해야 합니다".to_string());
        }

        let original_height = self.common.height;

        // 삭제 열의 폭 (셀 width 축소용)
        let col_widths = self.get_column_widths();
        let deleted_width = col_widths[col_idx as usize];

        // 삭제 열을 걸치는 병합 셀: col_span 축소, width 축소
        for cell in &mut self.cells {
            if cell.col < col_idx && cell.col + cell.col_span > col_idx {
                cell.col_span -= 1;
                if cell.width >= deleted_width {
                    cell.width -= deleted_width;
                }
            }
        }

        // 삭제 대상 열의 셀 제거 (해당 열에 앵커가 있고 col_span==1인 셀)
        self.cells
            .retain(|cell| !(cell.col == col_idx && cell.col_span == 1));

        // 삭제 열에 앵커가 있지만 col_span > 1인 병합 셀: 다음 열로 이동, col_span 축소
        for cell in &mut self.cells {
            if cell.col == col_idx && cell.col_span > 1 {
                cell.col_span -= 1;
                if cell.width >= deleted_width {
                    cell.width -= deleted_width;
                }
            }
        }

        // 삭제 열 오른쪽 셀: col -= 1
        for cell in &mut self.cells {
            if cell.col > col_idx {
                cell.col -= 1;
            }
        }

        // col_count 갱신 및 row_sizes 재계산
        self.col_count -= 1;
        self.rebuild_row_sizes();

        // 행 우선 순서 정렬
        self.cells.sort_by_key(|c| (c.row, c.col));

        // CommonObjAttr 크기 갱신
        self.update_ctrl_dimensions();
        if original_height > 0 {
            // 열 삭제는 행 수를 바꾸지 않으므로 표 외곽 높이는 기존 값을 유지한다.
            self.common.height = original_height;
            if self.raw_ctrl_data.len() >= common_obj_offsets::HEIGHT.end {
                self.raw_ctrl_data[common_obj_offsets::HEIGHT]
                    .copy_from_slice(&original_height.to_le_bytes());
            }
        }

        // 그리드 인덱스 재구축
        self.rebuild_grid();

        Ok(())
    }

    /// 직사각형 범위의 셀을 병합한다.
    ///
    /// 범위: (start_col, start_row) ~ (end_col, end_row) (모두 포함).
    /// 좌상단 셀이 병합 결과가 되고, 나머지 셀은 제거된다.
    pub fn merge_cells(
        &mut self,
        start_row: u16,
        start_col: u16,
        end_row: u16,
        end_col: u16,
    ) -> Result<(), String> {
        // 범위 유효성 검증
        if start_row > end_row || start_col > end_col {
            return Err("병합 범위가 유효하지 않습니다".to_string());
        }
        if end_row >= self.row_count || end_col >= self.col_count {
            return Err(format!(
                "병합 범위 ({},{})~({},{})가 표 크기 {}×{}를 초과합니다",
                start_row, start_col, end_row, end_col, self.row_count, self.col_count
            ));
        }

        // 범위 내 셀이 모두 범위 안에 들어오는지 확인 (부분 겹침 방지)
        for cell in &self.cells {
            let cell_end_row = cell.row + cell.row_span - 1;
            let cell_end_col = cell.col + cell.col_span - 1;

            // 셀이 범위와 겹치는지 확인
            let overlaps = cell.col <= end_col
                && cell_end_col >= start_col
                && cell.row <= end_row
                && cell_end_row >= start_row;

            if overlaps {
                // 겹치는 셀은 범위 안에 완전히 포함되어야 함
                let contained = cell.col >= start_col
                    && cell_end_col <= end_col
                    && cell.row >= start_row
                    && cell_end_row <= end_row;
                if !contained {
                    return Err(format!(
                        "셀 ({},{}) span ({},{})이 병합 범위를 벗어납니다",
                        cell.row, cell.col, cell.row_span, cell.col_span
                    ));
                }
            }
        }

        // 주 셀 존재 확인
        if !self
            .cells
            .iter()
            .any(|c| c.col == start_col && c.row == start_row)
        {
            return Err(format!(
                "주 셀 ({},{})을 찾을 수 없습니다",
                start_row, start_col
            ));
        }

        // 범위에 칸이 하나뿐이면 합칠 것이 없다 — 한컴과 같이 무변화(빈 칸 규약 높이를 실효값으로 굳히지 않는다).
        let in_range_count = self
            .cells
            .iter()
            .filter(|c| c.col >= start_col && c.col <= end_col && c.row >= start_row && c.row <= end_row)
            .count();
        if in_range_count <= 1 {
            return Ok(());
        }

        // 열폭/행높이 합산 — 병합 칸은 **실효** 기하(격자 열 폭 합, 실효 행 높이 합)를 물려받는다.
        // 종전엔 raw 행 높이(빈 칸 규약 284)를 더해 표 전체 병합이 3852 → 1284 로 주저앉았다(D2 거부, 2026-09-06).
        // 한컴도 병합 칸 높이 = 합쳐진 행들의 실제 높이 합.
        let col_widths = self.get_column_widths();
        let eff_row_heights = self.effective_row_heights();
        let new_width: HwpUnit = (start_col..=end_col)
            .map(|c| col_widths.get(c as usize).copied().unwrap_or(0))
            .sum();
        let new_height: HwpUnit = (start_row..=end_row)
            .map(|r| eff_row_heights.get(r as usize).copied().unwrap_or(0))
            .sum();

        // 비주 셀의 비어있지 않은 문단 수집 (모든 메타데이터 보존)
        let mut extra_paragraphs: Vec<Paragraph> = Vec::new();
        for cell in &self.cells {
            if cell.col == start_col && cell.row == start_row {
                continue; // 주 셀 스킵
            }
            let in_range = cell.col >= start_col
                && cell.col <= end_col
                && cell.row >= start_row
                && cell.row <= end_row;
            if in_range {
                for para in &cell.paragraphs {
                    if !para.text.is_empty() {
                        // [officex] 문단을 **통째로** 복제한다. 종전엔 필드를 골라 복사하고
                        // 나머지를 ..Default::default() 로 버려서, 흡수되는 셀의 누름틀
                        // (controls·field_ranges)·ctrl_data_records·중첩 표·그림·각주가
                        // 조용히 증발했다 — 글자만 살아남아 "값 111은 있는데 필드 officex.b 는
                        // 사라진" 문서가 ok:true 와 함께 남았다(엔진 단독 재현 확정).
                        // field_ranges[i].control_idx 는 같은 문단 안 controls 인덱스라
                        // 문단 단위 clone 이면 정합이 유지된다. 빈 문단만 버리는 정책은 그대로.
                        extra_paragraphs.push(para.clone());
                    }
                }
            }
        }

        // 비주 셀 제거 (한컴 오피스와 동일하게 셀을 실제로 제거)
        self.cells.retain(|cell| {
            if cell.col == start_col && cell.row == start_row {
                return true; // 주 셀 유지
            }
            let in_range = cell.col >= start_col
                && cell.col <= end_col
                && cell.row >= start_row
                && cell.row <= end_row;
            !in_range // 범위 밖 셀 유지, 범위 내 비주 셀 제거
        });

        // 주 셀 갱신
        let primary = self
            .cells
            .iter_mut()
            .find(|c| c.col == start_col && c.row == start_row)
            .expect("주 셀이 retain 후에도 존재해야 합니다");

        primary.col_span = end_col - start_col + 1;
        primary.row_span = end_row - start_row + 1;
        // raw_list_extra[0..4]에 참조 폭이 저장되어 있으면 갱신
        if primary.raw_list_extra.len() >= 4 {
            let old_ref_width =
                u32::from_le_bytes(primary.raw_list_extra[0..4].try_into().unwrap());
            if old_ref_width == primary.width {
                primary.raw_list_extra[0..4].copy_from_slice(&new_width.to_le_bytes());
            }
        }
        primary.width = new_width;
        primary.height = new_height;

        // 비어있지 않은 문단 추가
        for para in extra_paragraphs {
            primary.paragraphs.push(para);
        }

        // 행 우선 순서 정렬
        self.cells.sort_by_key(|c| (c.row, c.col));

        // row_sizes 갱신 (행별 실제 셀 개수)
        self.rebuild_row_sizes();

        // 그리드 인덱스 재구축
        self.rebuild_grid();

        // 병합으로 어떤 칸도 경계로 쓰지 않게 된 격자 선(어긋난 칸과 이웃을 합칠 때의 어긋선, 표 전체 병합의
        // 안쪽 선)을 접는다. 죽은 선이 남으면 열 폭 솔버가 그 양쪽 열을 나눌 근거를 잃어 1800 폴백으로
        // 흩어지고 D1(표 폭) 관문이 거부했다(2026-09-06: 41952 → 31568).
        self.collapse_dead_lines(Axis::Cols);
        self.collapse_dead_lines(Axis::Rows);

        Ok(())
    }

    /// 병합된 셀을 나눈다 (merge_cells의 역연산).
    ///
    /// 대상 셀의 col_span > 1 또는 row_span > 1이어야 한다.
    /// 원본 셀은 (target_col, target_row)에 col_span=1, row_span=1로 축소되고,
    /// 나머지 위치에 새 빈 셀이 생성된다.
    pub fn split_cell(&mut self, target_row: u16, target_col: u16) -> Result<(), String> {
        // 대상 셀 찾기 및 검증
        let cell_idx = self
            .cells
            .iter()
            .position(|c| c.col == target_col && c.row == target_row)
            .ok_or_else(|| format!("셀 ({},{})을 찾을 수 없습니다", target_row, target_col))?;

        let orig_col_span = self.cells[cell_idx].col_span;
        let orig_row_span = self.cells[cell_idx].row_span;
        let orig_width = self.cells[cell_idx].width;
        let orig_height = self.cells[cell_idx].height;

        if orig_col_span <= 1 && orig_row_span <= 1 {
            return Err("병합되지 않은 셀은 나눌 수 없습니다".to_string());
        }

        // 열폭 계산: 다른 행의 col_span==1 셀에서 실제 폭 추출, 없으면 균등 분배
        let col_widths = self.get_column_widths();
        let split_col_widths: Vec<HwpUnit> = {
            let has_real = (target_col..target_col + orig_col_span).all(|c| {
                self.cells.iter().any(|cell| {
                    cell.col == c
                        && cell.col_span == 1
                        && !(cell.col == target_col && cell.row == target_row)
                })
            });
            if has_real {
                (target_col..target_col + orig_col_span)
                    .map(|c| col_widths[c as usize])
                    .collect()
            } else {
                let each = orig_width / orig_col_span as u32;
                vec![each; orig_col_span as usize]
            }
        };

        // 행높이 계산: 다른 열의 row_span==1 셀에서 실제 높이 추출, 없으면 균등 분배
        let raw_row_heights = self.get_raw_row_heights();
        let split_row_heights: Vec<HwpUnit> = {
            let has_real = (target_row..target_row + orig_row_span).all(|r| {
                self.cells.iter().any(|cell| {
                    cell.row == r
                        && cell.row_span == 1
                        && !(cell.col == target_col && cell.row == target_row)
                })
            });
            if has_real {
                (target_row..target_row + orig_row_span)
                    .map(|r| raw_row_heights[r as usize])
                    .collect()
            } else {
                let each = orig_height / orig_row_span as u32;
                vec![each; orig_row_span as usize]
            }
        };

        // 주 셀 축소
        let new_width = split_col_widths[0];
        let primary = &mut self.cells[cell_idx];
        primary.col_span = 1;
        primary.row_span = 1;
        if primary.raw_list_extra.len() >= 4 {
            let old_ref = u32::from_le_bytes(primary.raw_list_extra[0..4].try_into().unwrap());
            if old_ref == primary.width {
                primary.raw_list_extra[0..4].copy_from_slice(&new_width.to_le_bytes());
            }
        }
        primary.width = new_width;
        primary.height = split_row_heights[0];

        // 새 셀 생성: 범위 내 (target_col, target_row) 제외한 모든 위치
        for ri in 0..orig_row_span {
            for ci in 0..orig_col_span {
                let r = target_row + ri;
                let c = target_col + ci;
                if r == target_row && c == target_col {
                    continue; // 주 셀 위치 스킵
                }
                let w = split_col_widths[ci as usize];
                let h = split_row_heights[ri as usize];
                let new_cell = Cell::new_from_template(c, r, w, h, &self.cells[cell_idx]);
                self.cells.push(new_cell);
            }
        }

        // 행 우선 순서 정렬
        self.cells.sort_by_key(|c| (c.row, c.col));

        // row_sizes 갱신 (행별 실제 셀 개수)
        self.rebuild_row_sizes();

        // 그리드 인덱스 재구축
        self.rebuild_grid();

        Ok(())
    }

    /// 셀을 N줄 × M칸으로 분할한다.
    ///
    /// 기존 `split_cell()`은 병합 해제만 지원하지만, 이 메서드는 임의 셀을
    /// 지정한 행/열 수로 분할한다. 테이블 그리드에 새 행/열이 추가되고,
    /// 인접 셀은 col_span/row_span이 확장되어 기존 형태를 유지한다.
    pub fn split_cell_into(
        &mut self,
        target_row: u16,
        target_col: u16,
        n_rows: u16,
        m_cols: u16,
        equal_row_height: bool,
        merge_first: bool,
    ) -> Result<(), String> {
        if n_rows < 1 || m_cols < 1 {
            return Err("분할 행/열 수는 1 이상이어야 합니다".to_string());
        }
        if n_rows == 1 && m_cols == 1 {
            return Ok(()); // no-op
        }

        // 대상 셀 찾기
        let cell_idx = self
            .cells
            .iter()
            .position(|c| c.col == target_col && c.row == target_row)
            .ok_or_else(|| format!("셀 ({},{})을 찾을 수 없습니다", target_row, target_col))?;

        let cs = self.cells[cell_idx].col_span;
        let rs = self.cells[cell_idx].row_span;

        // 병합 셀이면서 merge_first 옵션 → 먼저 병합 해제
        if merge_first && (cs > 1 || rs > 1) {
            self.split_cell(target_row, target_col)?;
            // split_cell 후 셀 인덱스 변경됨 → 재탐색
        }

        // 대상 셀 재탐색 (병합 해제 후 span=1x1)
        let cell_idx = self
            .cells
            .iter()
            .position(|c| c.col == target_col && c.row == target_row)
            .ok_or_else(|| {
                format!(
                    "분할 대상 셀 ({},{})을 찾을 수 없습니다",
                    target_row, target_col
                )
            })?;

        let target_width = self.cells[cell_idx].width;
        let target_height = self.cells[cell_idx].height;
        let cs = self.cells[cell_idx].col_span;
        let rs = self.cells[cell_idx].row_span;

        // 현재 span 기준으로 추가 열/행 계산
        // (다중 셀 분할 시 이전 분할로 span이 확장된 경우 extra=0)
        let extra_cols = if m_cols > cs { m_cols - cs } else { 0 };
        let extra_rows = if n_rows > rs { n_rows - rs } else { 0 };

        // 서브셀이 차지할 그리드 열/행 수
        let grid_cols = cs + extra_cols; // = max(m_cols, cs)
        let grid_rows = rs + extra_rows; // = max(n_rows, rs)

        // 폭 분배: 균등 분배 (나머지는 첫 셀에 가산)
        let base_w = target_width / m_cols as u32;
        let remainder_w = target_width - base_w * m_cols as u32;
        let sub_widths: Vec<HwpUnit> = (0..m_cols)
            .map(|i| base_w + if i == 0 { remainder_w } else { 0 })
            .collect();

        // 높이 분배 — equal_row_height 가 의미를 갖는 유일한 지점.
        //  · true  : 원래 셀 높이를 n_rows 로 균등 분배 → 표 전체 높이 보존
        //            (한컴 "줄 높이를 같게 나누기" 체크 상태)
        //  · false : 첫 서브행이 원래 높이를 유지하고 나머지는 최소 줄높이 →
        //            표가 (n_rows-1)×최소높이 만큼 자란다(체크 해제 상태)
        // ⚠ [officex] 옛 조건은 `equal_row_height || n_rows > 1` 이라 2행 이상 분할에선
        // 플래그가 무시됐다(false 분기는 n_rows==1 일 때만 도달) — QA에서 true/false 결과가
        // 완전히 동일했던 이유. 두 분기 모두 길이 n_rows 를 돌려줘야 한다(아래 sub_heights[ri] 인덱싱).
        let sub_heights: Vec<HwpUnit> = if equal_row_height {
            let base_h = target_height / n_rows as u32;
            let remainder_h = target_height - base_h * n_rows as u32;
            (0..n_rows)
                .map(|i| base_h + if i == 0 { remainder_h } else { 0 })
                .collect()
        } else {
            // 한 줄(1000HU) + 셀 상하 여백 = 빈 행의 최소 높이
            let (pad_top, pad_bottom) = {
                let p = &self.cells[cell_idx].padding;
                (p.top.max(0) as u32, p.bottom.max(0) as u32)
            };
            let min_h: HwpUnit = pad_top + 1000 + pad_bottom;
            (0..n_rows)
                .map(|i| if i == 0 { target_height } else { min_h })
                .collect()
        };

        // 서브셀의 col_span/row_span 분배 (grid_cols를 m_cols개에 분배)
        let base_cspan = grid_cols / m_cols;
        let cspan_rem = grid_cols - base_cspan * m_cols;
        let sub_cspans: Vec<u16> = (0..m_cols)
            .map(|i| base_cspan + if i < cspan_rem { 1 } else { 0 })
            .collect();
        let base_rspan = grid_rows / n_rows;
        let rspan_rem = grid_rows - base_rspan * n_rows;
        let sub_rspans: Vec<u16> = (0..n_rows)
            .map(|i| base_rspan + if i < rspan_rem { 1 } else { 0 })
            .collect();

        // 서브셀의 그리드 col 오프셋 계산 (col_span 누적)
        let mut sub_col_offsets: Vec<u16> = vec![0; m_cols as usize];
        for i in 1..m_cols as usize {
            sub_col_offsets[i] = sub_col_offsets[i - 1] + sub_cspans[i - 1];
        }
        let mut sub_row_offsets: Vec<u16> = vec![0; n_rows as usize];
        for i in 1..n_rows as usize {
            sub_row_offsets[i] = sub_row_offsets[i - 1] + sub_rspans[i - 1];
        }

        // 기존 셀 조정 (대상 셀 제외)
        for i in 0..self.cells.len() {
            if i == cell_idx {
                continue;
            }
            let cell = &mut self.cells[i];

            // --- 열 방향 조정 ---
            if extra_cols > 0 {
                if cell.col > target_col {
                    cell.col += extra_cols;
                } else if cell.col == target_col {
                    cell.col_span += extra_cols;
                } else if cell.col < target_col && cell.col + cell.col_span > target_col {
                    cell.col_span += extra_cols;
                }
            }

            // --- 행 방향 조정 ---
            if extra_rows > 0 {
                if cell.row > target_row {
                    cell.row += extra_rows;
                } else if cell.row == target_row {
                    cell.row_span += extra_rows;
                } else if cell.row < target_row && cell.row + cell.row_span > target_row {
                    cell.row_span += extra_rows;
                }
            }
        }

        // 주 셀(0,0) 축소
        let template = self.cells[cell_idx].clone();
        let primary = &mut self.cells[cell_idx];
        primary.width = sub_widths[0];
        primary.height = sub_heights[0];
        primary.col_span = sub_cspans[0];
        primary.row_span = sub_rspans[0];
        if primary.raw_list_extra.len() >= 4 {
            primary.raw_list_extra[0..4].copy_from_slice(&sub_widths[0].to_le_bytes());
        }

        // 나머지 서브셀 생성
        for ri in 0..n_rows {
            for ci in 0..m_cols {
                if ri == 0 && ci == 0 {
                    continue;
                } // 주 셀 스킵
                let r = target_row + sub_row_offsets[ri as usize];
                let c = target_col + sub_col_offsets[ci as usize];
                let w = sub_widths[ci as usize];
                let h = sub_heights[ri as usize];
                let mut new_cell = Cell::new_from_template(c, r, w, h, &template);
                new_cell.col_span = sub_cspans[ci as usize];
                new_cell.row_span = sub_rspans[ri as usize];
                if new_cell.raw_list_extra.len() >= 4 {
                    new_cell.raw_list_extra[0..4].copy_from_slice(&w.to_le_bytes());
                }
                self.cells.push(new_cell);
            }
        }

        // 테이블 메타 갱신
        self.col_count += extra_cols;
        self.row_count += extra_rows;

        self.cells.sort_by_key(|c| (c.row, c.col));
        self.rebuild_row_sizes();
        self.update_ctrl_dimensions();
        self.rebuild_grid();

        Ok(())
    }

    /// 범위 내 셀들을 각각 N줄 × M칸으로 분할한다.
    ///
    /// 우측→좌측, 하단→상단 순서로 처리하여 그리드 시프트가
    /// 아직 처리되지 않은 셀에 영향을 주지 않도록 한다.
    /// [경계선 재설계 2026-08-04] 한 칸의 아래/오른쪽 경계를 어긋낸다 — 격자 재구성 정본.
    ///
    /// 렌더 흉내(renderHeight 힌트)가 아니라 진짜 격자를 다시 짠다: 경계가 파고드는 쪽 칸을
    /// `split_cell_into` 로 둘로 나눠 격자선을 만들고, 가까운 조각을 대상 칸에 `merge_cells` 로
    /// 흡수시킨다. 두 연산 모두 스팬 재계산·직렬화 왕복이 검증돼 있어 파일에 그대로 저장되고
    /// 한컴에서도 동일하게 열린다. 표 바깥 크기는 불변(규칙 docs/table-border-rules.md #1·#5).
    ///
    /// * `edge_right` false = 아래 경계(세로 이동), true = 오른쪽 경계(가로 이동)
    /// * `delta` > 0 = 아래/오른쪽으로(대상이 커짐), < 0 = 위/왼쪽으로(대상이 줄어듦)
    ///
    /// ponytail: 경계가 파고드는 쪽 칸이 이미 병합(스팬>1)이면 v1 은 오류 — 스냅으로 기존
    /// 격자선에 맞춰 되돌리는 치유는 ⌘Z(스냅숏 undo)가 담당한다. 필요해지면 내부 격자선
    /// 탐색으로 확장.
    /// 이 경계(대상 셀의 우변/하변에 해당하는 격자선)가 **정렬선**인가 —
    /// 대상 줄 밖의 다른 셀도 이 격자선을 자기 경계로 쓰면 정렬, 아무도 안 쓰면
    /// 어긋난 선이다. 어긋내기가 "신규"인지 "재이동"인지 가르는 단일 판정.
    /// [격자 뷰 2026-09-02] pub — check_corpus_invariants `predicate_mismatch` 의 오라클(8-b 위임 전까지).
    pub fn is_boundary_aligned(&self, t: &Cell, boundary: u16, edge_right: bool) -> bool {
        let (axis, band) = if edge_right { (Axis::Cols, t.row) } else { (Axis::Rows, t.col) };
        table_grid::line_info(self, axis, boundary).aligned_for(band)
    }

    /// [2026-08-16 신고 "절대 표 깨지지 않게"] 어긋내기 **트랜잭션 안전망**.
    ///
    /// 어긋낸 격자는 span 연립이 미결정이 되는 상태(겹치는 선 재분할, 다른 열이 핀 잡은
    /// 스팬 이웃 재이동 등)가 존재하고, 그때 솔버가 임의 해를 고르면 표가 자라거나
    /// 찢어진다(3×3 전수 프로브 실측: 연속 어긋내기에서 3852→5052). 사례를 전부
    /// 열거하는 대신 **결과를 검증**한다 — 실행 전 상태를 통째로 붙들고, 실행 후
    /// 불변식(실효 높이 합·폭 합·격자 건전성)이 깨져 있으면 롤백하고 거부한다.
    /// 표를 깨뜨리는 조작은 어떤 경로로도 커밋되지 않는다.
    pub fn offset_cell_boundary(
        &mut self,
        cell_idx: usize,
        edge_right: bool,
        delta: i32,
    ) -> Result<(), String> {
        self.transact(CmdClass::KeepWidthHeight, Some(0), |t| t.offset_cell_boundary_core(cell_idx, edge_right, delta))
    }

    /// [11-b] 어긋내기 **단일 절차** — 격자(TableGrid)로 판정하고 셀 프리미티브 4개로 쓴다.
    ///
    /// 축 = edge_right ? Cols : Rows. 행은 거리·여유를 실효 공간(row_heights_eff)으로 판정하고
    /// 쓰기 직전 `materialize_rows_effective` 로 연루 행을 굳힌다(종전 규약).
    /// (0) 바깥 테두리·이웃 없음·직교 불일치 → 종전 메시지 Err(`boundary_move_range`).
    /// (1) 컨텍스트 R — L 이 어긋선: A = 대상 시작 뒤 첫 정렬선, cur_off = pos(L)−pos(A).
    ///     R1 승격(table.rs:2089 규약: cur_off≠0 ∧ (next_off==0 ∨ 부호 반전)) 또는
    ///     R3 통과(±d 이전이 인접 단위 구간을 붕괴시키거나 |d−delta|>MIN_CELL) → join_to_line(t, L→A)
    ///     + collapse, 잔여 |next_off| ≤ MIN_CELL 이면 스냅 종료, 아니면 A 에서 컨텍스트 N 을 1회 계속.
    ///     R2 재이동: 두 셀 크기 ±d(격자 불변).
    /// (2) 컨텍스트 N — L 정렬선(신규): 줄어드는 쪽 구간에서 낙하점을 찾아 기존 선 합류(off≤MIN_CELL
    ///     ∧ acc>0 → 가까운 선 / size−off≤MIN_CELL ∧ acc+size≤room → 먼 선) 아니면 insert_line(양축
    ///     MIN_CELL 클램프) → join_to_line(t, L→J).
    /// 재귀·프로브 클론 없음; 클론은 래퍼의 transact 1회.
    fn offset_cell_boundary_core(&mut self, cell_idx: usize, edge_right: bool, delta: i32) -> Result<(), String> {
        if delta == 0 {
            return Ok(());
        }
        let axis = if edge_right { Axis::Cols } else { Axis::Rows };
        let (dmin, dmax) = self.boundary_move_range(cell_idx, edge_right)?; // (0) 검사 포함
        let t = self.cells[cell_idx].clone();
        let (t_start, t_span, _) = axis_of(&t, axis);
        let band = ortho_of(&t, axis);
        let line = t_start + t_span;
        let n_idx = self.neighbour_at(&t, axis, line).ok_or("이웃 소실")?;
        let n = self.cells[n_idx].clone();
        let (n_start, n_span, _) = axis_of(&n, axis);
        let n_end = n_start + n_span;
        let g = TableGrid::axes(self);

        let mut delta = delta;
        let mut line = line;
        if !g.is_aligned_for(axis, line, band.0) {
            // ── 컨텍스트 R ──
            // 승격 판정선 A = 대상 시작 뒤 첫 정렬선(2089 규약). 합류 목표선 J = 종전 복원 규약(restore_cell_boundary
            // span≥2 → L−1: 마지막 조각을 이웃에 / extend span1 → L+1: 이웃 첫 조각 흡수, 이웃 span≥2 필요) — 병합
            // 셀을 어긋낸 경우 A≠J 라 A 로 합류하면 원래 병합까지 풀린다(s4b 실측). 잔여는 J 기준.
            let a = g.next_aligned_line(axis, t_start, band.0);
            let join = if t_span >= 2 {
                Some(line - 1)
            } else if n_span >= 2 {
                Some(line + 1)
            } else {
                None
            };
            let d = delta.clamp(dmin, dmax);
            // 재이동이 인접 단위 구간(delta>0: [L,L+1) / delta<0: [L−1,L))을 MIN_CELL 밑으로 붕괴시키면
            // ±d 로는 표현 불가(종전 프로브의 thin·불변식 불합격) → 통과.
            let adj = if delta > 0 { line } else { line - 1 };
            let adj_size = g.line_pos(axis, adj + 1, true) - g.line_pos(axis, adj, true);
            // 종전 thin(probe) ≤ thin0 매핑: 이미 얇은(<MIN_CELL, 로드된 실물) 구간은 더 줄어도 얇은 수가 늘지 않는다.
            let adj_floor = if adj_size < Self::MIN_CELL { 1 } else { Self::MIN_CELL };
            let fits = d != 0 && (d - delta).abs() <= Self::MIN_CELL && adj_size - d.abs() >= adj_floor;
            let promote = a.is_some_and(|a| {
                let cur_off = g.line_pos(axis, line, true) - g.line_pos(axis, a, true);
                let next_off = cur_off + delta;
                cur_off != 0 && (next_off == 0 || cur_off.signum() != next_off.signum())
            });
            // 합류 목표가 없으면(종전 복원 불가 = 승격 프로브 실패) 재이동으로 계속한다.
            if join.is_none() || (!promote && fits) {
                if d == 0 {
                    return Err(Self::no_room_msg(delta, edge_right));
                }
                if axis == Axis::Rows {
                    self.materialize_rows_effective(t.row as usize, (n_end - 1) as usize);
                }
                let t_now = *axis_mut(&mut self.cells[cell_idx], axis).2 as i32;
                let n_now = *axis_mut(&mut self.cells[n_idx], axis).2 as i32;
                *axis_mut(&mut self.cells[cell_idx], axis).2 = (t_now + d).max(Self::MIN_CELL) as HwpUnit;
                *axis_mut(&mut self.cells[n_idx], axis).2 = (n_now - d).max(Self::MIN_CELL) as HwpUnit;
                self.finish_cells();
                return Ok(());
            }
            // R1 승격 / R3 통과: A 로 합류 후 잔여를 A 에서 1회 신규 어긋내기. d==0(이웃 소진)도 통과로 —
            // 종전 프로브 실패 → 복원+재어긋의 순효과(같은 자리 재착지, Ok)와 같다.
            let j = join.ok_or("정렬선 없음")?;
            let remainder = g.line_pos(axis, line, true) - g.line_pos(axis, j, true) + delta;
            if axis == Axis::Rows {
                self.materialize_rows_effective(t.row as usize, (n_end - 1) as usize); // 행은 쓰기 직전 물질화
            }
            self.join_to_line(cell_idx, axis, line, j)?;
            self.collapse_dead_lines(axis);
            if remainder.abs() <= Self::MIN_CELL {
                return Ok(()); // 정렬선 ±MIN_CELL 이내 복귀 = 치유(스냅)
            }
            delta = remainder;
            line = self.cells.iter().position(|c| (c.row, c.col) == (t.row, t.col)).map(|i| {
                let (s, sp, _) = axis_of(&self.cells[i], axis);
                s + sp
            }).ok_or("복원 후 대상 소실")?;
        }

        // ── 컨텍스트 N — L 정렬선, 신규 어긋내기 ──
        let t_idx = self.cells.iter().position(|c| (c.row, c.col) == (t.row, t.col)).ok_or("대상 소실")?;
        let t = self.cells[t_idx].clone();
        let (t_start, _, _) = axis_of(&t, axis);
        let n_idx = self.neighbour_at(&t, axis, line).ok_or("이웃 소실")?;
        let n = self.cells[n_idx].clone();
        let (n_start, n_span, _) = axis_of(&n, axis);
        let n_end = n_start + n_span;
        let g = TableGrid::axes(self);
        let (s_idx, far) = if delta > 0 { (n_idx, n_end) } else { (t_idx, t_start) };
        // 여유: 열 = 셀 폭 − MIN_CELL, 행 = Σ실효(delta>0: 이웃 스팬 구간) / max(저장, 실효[첫 행])(delta<0) − 조각 바닥
        let floor = if edge_right {
            Self::MIN_CELL
        } else {
            self.stagger_piece_floor_hu(s_idx, &g.cell_floors, Self::MIN_CELL)
        };
        let space = match (axis, delta > 0) {
            (Axis::Cols, _) => axis_of(&self.cells[s_idx], axis).2 as i32,
            (Axis::Rows, true) => g.line_pos(axis, far, true) - g.line_pos(axis, line, true),
            (Axis::Rows, false) => {
                let eff = g.row_heights_eff.get(t.row as usize).copied().unwrap_or(t.height);
                t.height.max(eff) as i32
            }
        };
        let room = space - floor;
        let d = delta.abs().min(room);
        if d <= 0 {
            return Err(Self::no_room_msg(delta, edge_right));
        }
        if axis == Axis::Rows {
            let hi = if delta > 0 { n_end - 1 } else { n_start };
            self.materialize_rows_effective(t.row as usize, hi as usize);
        }
        let g = TableGrid::axes(self);
        // L 에서 줄어드는 쪽으로 걸어 낙하점이 든 단위 구간 (off = 구간의 L 쪽 끝에서의 거리), acc = L→가까운 선 거리
        let Located { line: k, off, size } = g.locate(axis, line, far, d);
        let (near, far_line) = if delta > 0 { (k, k + 1) } else { (k + 1, k) };
        let acc = (g.line_pos(axis, near, true) - g.line_pos(axis, line, true)).abs();
        let j = if off <= Self::MIN_CELL && acc > 0 {
            near
        } else if size - off <= Self::MIN_CELL && acc + size <= room {
            far_line
        } else {
            if size < Self::MIN_CELL * 2 {
                return Err("낙하점 주변에 분할 여유가 없습니다".to_string());
            }
            let off = off.clamp(Self::MIN_CELL, size - Self::MIN_CELL);
            let off_k = if delta > 0 { off } else { size - off };
            self.insert_line(axis, k, band, off_k as HwpUnit, delta > 0)?;
            if delta > 0 {
                k + 1
            } else {
                line += 1; // 삽입선이 L 앞 → L 인덱스 +1
                k + 1
            }
        };
        // 전달: 밴드에서 L 로 끝나는 셀(t 또는 삽입 빈 조각)의 끝선을 J 로
        let from_idx = self.neighbour_at(&t, axis, line - 1).ok_or("대상 소실")?;
        self.join_to_line(from_idx, axis, line, j)?;
        Ok(())
    }

    /// 밴드(직교 좌표 = t) 에서 축 위치 `pos` 슬롯을 점유하는 셀.
    fn neighbour_at(&self, t: &Cell, axis: Axis, pos: u16) -> Option<usize> {
        match axis {
            Axis::Cols => self.cell_index_at(t.row, pos),
            Axis::Rows => self.cell_index_at(pos, t.col),
        }
    }

    fn no_room_msg(delta: i32, edge_right: bool) -> String {
        match (delta > 0, edge_right) {
            (true, true) => "이웃 칸에 남는 폭이 없습니다",
            (true, false) => "이웃 칸에 남는 높이가 없습니다",
            (false, true) => "대상 칸에 남는 폭이 없습니다",
            (false, false) => "대상 칸에 남는 높이가 없습니다",
        }
        .to_string()
    }

    /// 경계 이동 **허용 델타** [min, max](HU) — 셀 `cell_idx` 의 우변(edge_right)/하변을 옮길 때
    /// 대상·이웃이 자기 바닥 밑으로 줄지 않는 범위. 종전 shift_offset_boundary 의 floor 산식 한 곳:
    /// 열 바닥 = MIN_CELL, 행 바닥 = `stagger_piece_floor_hu`(내용 있으면 글줄 바닥, 빈 조각은 MIN_CELL),
    /// 행 여유는 **실효 공간** max(저장, 실효 행높이). (0) 검사도 여기서: 바깥 테두리·이웃 없음·직교
    /// 불일치는 종전 메시지 그대로 Err.
    /// ponytail: 스팬 셀의 행 여유는 종전대로 max(height, eff[첫 행]) — Σeff 로 바꾸면 legacy 와 갈린다.
    pub fn boundary_move_range(&self, cell_idx: usize, edge_right: bool) -> Result<(i32, i32), String> {
        let t = self
            .cells
            .get(cell_idx)
            .ok_or_else(|| "셀 인덱스가 유효하지 않습니다".to_string())?;
        let axis = if edge_right { Axis::Cols } else { Axis::Rows };
        let (start, span, _) = axis_of(t, axis);
        let boundary = start + span;
        if boundary >= if edge_right { self.col_count } else { self.row_count } {
            return Err("바깥 테두리는 어긋낼 수 없습니다".to_string());
        }
        let n_idx = match axis {
            Axis::Cols => self.cell_index_at(t.row, boundary),
            Axis::Rows => self.cell_index_at(boundary, t.col),
        }
        .ok_or_else(|| {
            if edge_right { "오른쪽 이웃 셀을 찾지 못했습니다" } else { "아래 이웃 셀을 찾지 못했습니다" }.to_string()
        })?;
        let n = &self.cells[n_idx];
        if ortho_of(n, axis) != ortho_of(t, axis) {
            return Err(if edge_right {
                "위아래 높이가 다른 칸과는 경계를 어긋낼 수 없습니다"
            } else {
                "좌우 폭이 다른 칸과는 경계를 어긋낼 수 없습니다"
            }
            .to_string());
        }
        let g = TableGrid::axes(self);
        let floor_of = |idx: usize| -> i32 {
            if edge_right {
                Self::MIN_CELL
            } else {
                self.stagger_piece_floor_hu(idx, &g.cell_floors, Self::MIN_CELL)
            }
        };
        let space_of = |c: &Cell| -> i32 {
            if edge_right {
                c.width as i32
            } else {
                let eff = g.row_heights_eff.get(c.row as usize).copied().unwrap_or(c.height);
                c.height.max(eff) as i32
            }
        };
        let max = (space_of(n) - floor_of(n_idx)).max(0);
        let min = -(space_of(t) - floor_of(cell_idx)).max(0);
        Ok((min, max))
    }

    /// 모델층 **트랜잭션 관문** — saved=clone → f → check_invariants → check_deltas(class) → 실패 시
    /// `*self = saved` 롤백 + Err. 표를 깨뜨리는 조작은 어떤 경로로도 커밋되지 않는다.
    pub fn transact<T>(
        &mut self,
        class: CmdClass,
        expected_cell_delta: Option<i64>,
        f: impl FnOnce(&mut Table) -> Result<T, String>,
    ) -> Result<T, String> {
        let saved = self.clone();
        let out = f(self).and_then(|v| {
            self.check_invariants()
                .map_err(|why| format!("이 조작은 표 격자를 깨뜨려 취소했습니다 — {why}"))?;
            let bad = self.check_deltas(&saved, class, expected_cell_delta, false);
            if !bad.is_empty() {
                return Err(format!("표 크기 규약을 깨뜨려 취소했습니다 — {}", bad.join(", ")));
            }
            Ok(v)
        });
        if out.is_err() {
            *self = saved;
        }
        out
    }

    /// [불변식 가드 2026-09-02] 표 **구조** 불변식 — 위반이면 Err(사유).
    ///
    /// 공개 명령 종료 시점에만 검사한다 — insert_line·split_cells_in_range 루프 등
    /// 트랜잭션 중간 상태는 의도적으로 위반한다. 항목은 한컴 실물(HWPX 2791표·HWP5 1221표)
    /// 전수 위반 0건인 것만: 크기 절대 규칙("행 폭 합 == 표 폭" 등)은 실물이 위반하므로
    /// 여기 넣지 않는다(명령 전후 델타 검사 몫).
    ///
    /// - S1 격자: 모든 슬롯이 정확히 한 셀의 span 영역(빈 슬롯·겹침·범위 초과 없음), span ≥ 1
    /// - S2 row_sizes: 길이 == row_count, 값 == 행별 셀 수(HWP5 스펙)
    /// - S3 순서: cells 가 (row,col) 행 우선 오름차순·유일 (LIST_HEADER 직렬화 순서)
    /// - S4 치수: raw_ctrl_data 가 있으면 WIDTH/HEIGHT 바이트 == common.width/height (이중 장부)
    /// - S5 개수: 행·열·셀 수 ≥ 1, span ≥ 1. 셀 크기 상한은 두지 않는다 — 실물 HWP5(hwpspec.hwp 등
    ///   4파일 175셀)가 음수 높이를 u32 로 랩해 저장하고 있어(4294962496 = −4800) 절대 규칙이 아니다.
    ///   명령이 새로 만드는 언더플로(ed0bc5c94)는 델타 검사 "과대 셀 수 비증가" 몫.
    pub fn check_invariants(&self) -> Result<(), String> {
        let rc = self.row_count as usize;
        let cc = self.col_count as usize;
        if rc == 0 || cc == 0 || self.cells.is_empty() {
            return Err(format!("표 구조 손상: 행 {rc}×열 {cc}, 셀 {}개", self.cells.len()));
        }
        let mut cover = vec![0u8; rc * cc];
        for (i, c) in self.cells.iter().enumerate() {
            if c.col_span == 0 || c.row_span == 0 {
                return Err(format!("표 구조 손상: 셀 {i} 의 span 이 0"));
            }
            let r1 = c.row as usize + c.row_span as usize;
            let c1 = c.col as usize + c.col_span as usize;
            if r1 > rc || c1 > cc {
                return Err(format!(
                    "표 구조 손상: 셀 {i} ({},{}) span {}×{} 이 격자 {rc}×{cc} 밖",
                    c.row, c.col, c.row_span, c.col_span
                ));
            }
            for r in c.row as usize..r1 {
                for k in c.col as usize..c1 {
                    cover[r * cc + k] = cover[r * cc + k].saturating_add(1);
                }
            }
        }
        if let Some(gi) = cover.iter().position(|&n| n != 1) {
            let what = if cover[gi] == 0 { "빈 슬롯" } else { "겹침" };
            return Err(format!("표 구조 손상: 격자 ({},{}) {what}", gi / cc, gi % cc));
        }
        for w in self.cells.windows(2) {
            if (w[0].row, w[0].col) >= (w[1].row, w[1].col) {
                return Err(format!(
                    "표 구조 손상: 셀 순서 ({},{}) 뒤에 ({},{})",
                    w[0].row, w[0].col, w[1].row, w[1].col
                ));
            }
        }
        if self.row_sizes.len() != rc {
            return Err(format!("표 구조 손상: row_sizes 길이 {} ≠ 행 수 {rc}", self.row_sizes.len()));
        }
        for (r, &n) in self.row_sizes.iter().enumerate() {
            let actual = self.cells.iter().filter(|c| c.row as usize == r).count();
            if n as usize != actual {
                return Err(format!("표 구조 손상: row_sizes[{r}]={n} ≠ 행 셀 수 {actual}"));
            }
        }
        if self.raw_ctrl_data.len() >= common_obj_offsets::HEIGHT.end {
            let rd = |r: std::ops::Range<usize>| {
                u32::from_le_bytes(self.raw_ctrl_data[r].try_into().unwrap())
            };
            let (w, h) = (rd(common_obj_offsets::WIDTH), rd(common_obj_offsets::HEIGHT));
            if w != self.common.width || h != self.common.height {
                return Err(format!(
                    "표 구조 손상: 저장 치수 {w}×{h} ≠ 표시 치수 {}×{}",
                    self.common.width, self.common.height
                ));
            }
        }
        Ok(())
    }

    /// [불변식 가드 2026-09-02] 경고 수준 점검 — 거부하지 않고 사유 목록만 돌려준다.
    /// - S6 죽은 선: 어떤 셀도 start/end 경계로 쓰지 않는 내부 격자선(목격자 없는 열/행 → 폴백 성장의 뿌리)
    /// - S7 폴백 도달: get_column_widths/get_row_heights 가 1800/400 기본값을 쓰는 열/행
    /// - S8 영역: zones 의 start/end 가 격자 밖
    pub fn lint_invariants(&self) -> Vec<String> {
        let rc = self.row_count as usize;
        let cc = self.col_count as usize;
        let mut out = Vec::new();
        for line in 1..cc {
            let used = self.cells.iter().any(|c| {
                c.col as usize == line || c.col as usize + c.col_span as usize == line
            });
            if !used {
                out.push(format!("S6 죽은 세로선 {line}"));
            }
        }
        for line in 1..rc {
            let used = self.cells.iter().any(|c| {
                c.row as usize == line || c.row as usize + c.row_span as usize == line
            });
            if !used {
                out.push(format!("S6 죽은 가로선 {line}"));
            }
        }
        out.extend(TableGrid::axes(self).fallback_bands.iter().map(|&(axis, k)| match axis {
            Axis::Cols => format!("S7 열 {k} 폭 폴백(1800)"),
            Axis::Rows => format!("S7 행 {k} 높이 폴백(400)"),
        }));
        for (i, z) in self.zones.iter().enumerate() {
            if z.start_col > z.end_col
                || z.start_row > z.end_row
                || z.end_col as usize >= cc
                || z.end_row as usize >= rc
            {
                out.push(format!(
                    "S8 영역 {i} ({},{})-({},{}) 이 격자 {rc}×{cc} 밖",
                    z.start_row, z.start_col, z.end_row, z.end_col
                ));
            }
        }
        out
    }

    /// [불변식 가드 2026-09-02] 명령 전후 **델타** 불변식 — 위반 사유 목록(빈 벡터면 통과).
    ///
    /// 절대값("행 폭 합 == 표 폭" 등)은 한컴 실물이 위반하므로(말뭉치 2417/3080/1013표) 규칙이 될 수
    /// 없고, "이 명령이 무엇을 보존해야 하는가"만 명령 클래스로 검사한다. 경고 모드로 먼저 돌려
    /// 현재 위반하는 조작을 열거한 뒤 산술을 고치고 Err 로 승격한다.
    ///
    /// - D1 폭 보존: Σget_column_widths 변화 ≤ 4HU (KeepWidthHeight·KeepWidth)
    /// - D2 높이 보존: Σeffective_row_heights 변화 ≤ 4HU (KeepWidthHeight·KeepHeight)
    /// - D3 셀 수: `expected_cell_delta` 가 주어지면 정확히 그만큼
    /// - D4 내용 보존: 비어 있지 않은 문단의 총 문자 수 불변 (`deletes_content` 가 아닐 때)
    /// - D5 슬리버 비증가: 폭 < MIN_CELL 열 수·실효 높이 < MIN_CELL 행 수가 늘지 않음
    /// - S5' 과대 셀 비증가: 크기 ≥ 1_000_000 셀 수가 늘지 않음 (u32 언더플로 ed0bc5c94; 실물에
    ///   음수 높이 랩 셀이 있어 절대 규칙은 불가)
    ///
    /// ±4HU 근거: split_cell 균등 분할 잔여 손실 상한 span−1 HU.
    pub fn check_deltas(
        &self,
        before: &Table,
        class: CmdClass,
        expected_cell_delta: Option<i64>,
        deletes_content: bool,
    ) -> Vec<String> {
        let mut out = Vec::new();
        let sum_w = |t: &Table| t.get_column_widths().iter().map(|&w| w as u64).sum::<u64>();
        let sum_h = |t: &Table| t.effective_row_heights().iter().map(|&h| h as u64).sum::<u64>();
        if matches!(class, CmdClass::KeepWidthHeight | CmdClass::KeepWidth) {
            let (a, b) = (sum_w(before), sum_w(self));
            if a.abs_diff(b) > 4 {
                out.push(format!("D1 표 폭 변화 {a} → {b}"));
            }
        }
        if matches!(class, CmdClass::KeepWidthHeight | CmdClass::KeepHeight) {
            let (a, b) = (sum_h(before), sum_h(self));
            if a.abs_diff(b) > 4 {
                out.push(format!("D2 표 높이 변화 {a} → {b}"));
            }
        }
        if let Some(exp) = expected_cell_delta {
            let got = self.cells.len() as i64 - before.cells.len() as i64;
            if got != exp {
                out.push(format!("D3 셀 수 변화 {got} (기대 {exp})"));
            }
        }
        if !deletes_content {
            let chars = |t: &Table| {
                t.cells
                    .iter()
                    .flat_map(|c| c.paragraphs.iter())
                    .map(|p| p.text.chars().count())
                    .sum::<usize>()
            };
            let (a, b) = (chars(before), chars(self));
            if a != b {
                out.push(format!("D4 문자 수 변화 {a} → {b}"));
            }
        }
        let thin_cols = |t: &Table| t.get_column_widths().iter().filter(|&&w| w < Self::MIN_CELL as u32).count();
        let thin_rows =
            |t: &Table| t.effective_row_heights().iter().filter(|&&h| h < Self::MIN_CELL as u32).count();
        if thin_cols(self) > thin_cols(before) {
            out.push(format!("D5 슬리버 열 {} → {}", thin_cols(before), thin_cols(self)));
        }
        if thin_rows(self) > thin_rows(before) {
            out.push(format!("D5 슬리버 행 {} → {}", thin_rows(before), thin_rows(self)));
        }
        let huge = |t: &Table| {
            t.cells.iter().filter(|c| c.width >= 1_000_000 || c.height >= 1_000_000).count()
        };
        if huge(self) > huge(before) {
            out.push(format!("S5 과대 셀 {} → {}", huge(before), huge(self)));
        }
        out
    }

    /// [경계선 재설계 2026-08-04] 어긋낸 경계를 정렬로 복원(치유) — offset 의 역연산.
    ///
    /// 스냅 가이드가 원래 경계선 위치에 캐치했을 때 스튜디오가 호출한다. 대상 칸이
    /// 어긋나며 흡수했던 조각을 병합 해제로 되찾아 이웃에 되돌리고, 크기는 같은 구간을
    /// 공유하는 다른 열/행 셀(목격자)에서 복사한다. 그 결과 아무 셀 경계도 쓰지 않게 된
    /// 격자 줄은 접어서 원래의 단순한 격자로 되돌린다.
    /// [2026-08-16] 복원도 트랜잭션 — 어긋난 적 없는 셀에 대한 오발 호출(스튜디오 CATCH
    /// 치유 판정 오류 등)이 격자를 키우는 사고가 실측됐다(키보드 연속 +283HU/회).
    /// offset_cell_boundary 와 동일하게 결과 불변식 위반 시 롤백 + 거부한다.
    pub fn restore_cell_boundary(
        &mut self,
        cell_idx: usize,
        edge_right: bool,
    ) -> Result<(), String> {
        self.transact(CmdClass::KeepWidthHeight, Some(0), |t| t.restore_cell_boundary_core(cell_idx, edge_right))
    }

    /// [11-b] 복원 = 컨텍스트 R 에 p := pos(A) 강제 — join_to_line(t, L→A) + collapse_dead_lines.
    /// t 가 span1 이어도 같은 식(종전 extend_cell_to_offset_line: A 는 이웃의 첫 내부선).
    fn restore_cell_boundary_core(&mut self, cell_idx: usize, edge_right: bool) -> Result<(), String> {
        let axis = if edge_right { Axis::Cols } else { Axis::Rows };
        let t = self
            .cells
            .get(cell_idx)
            .ok_or_else(|| "셀 인덱스가 유효하지 않습니다".to_string())?
            .clone();
        let (t_start, t_span, _) = axis_of(&t, axis);
        let band = ortho_of(&t, axis);
        let line = t_start + t_span;
        // 종전 메시지·검사 순서 유지: span1(옛 extend_cell_to_offset_line) 은 '정렬' 문구, 정렬 검사 → 직교 검사.
        let span1 = t_span < 2;
        if line >= if edge_right { self.col_count } else { self.row_count } {
            return Err(if span1 { "바깥 테두리는 정렬 대상이 아닙니다" } else { "바깥 테두리는 복원 대상이 아닙니다" }.to_string());
        }
        let n_idx = self.neighbour_at(&t, axis, line).ok_or_else(|| {
            if edge_right { "오른쪽 이웃 셀을 찾지 못했습니다" } else { "아래 이웃 셀을 찾지 못했습니다" }.to_string()
        })?;
        // 종전 규약 그대로 정렬선 검사는 없다 — span1 은 이웃 span≥2 만 요구(옛 extend), span≥2 는 무조건
        // 마지막 조각을 이웃에 넘긴다(옛 split·재병합). L 이 정렬선이어도 legacy 가 Ok 였으므로 거부하지 않는다.
        if span1 && axis_of(&self.cells[n_idx], axis).1 < 2 {
            return Err("어긋난 경계가 아닙니다".to_string());
        }
        if ortho_of(&self.cells[n_idx], axis) != band {
            return Err(match (edge_right, span1) {
                (true, true) => "위아래 높이가 다른 칸과는 정렬할 수 없습니다",
                (true, false) => "위아래 높이가 다른 칸과는 복원할 수 없습니다",
                (false, true) => "좌우 폭이 다른 칸과는 정렬할 수 없습니다",
                (false, false) => "좌우 폭이 다른 칸과는 복원할 수 없습니다",
            }
            .to_string());
        }
        // 합류 목표 = 종전 규약: span≥2 → L−1(마지막 조각을 이웃에), span1 → L+1(이웃 첫 조각 흡수).
        // 대상 시작 뒤 첫 정렬선이 아니다 — 병합 셀을 어긋낸 경우 그 선으로 가면 원래 병합까지 풀린다.
        // 행도 물질화하지 않는다 — 종전 복원(split_cell·merge_cells)은 저장 층 그대로 다뤘고, 어긋내기가
        // 먼저 물질화한 행은 이미 명시값이라 결과가 같다(물질화하면 정렬선 복원에서 저장 높이가 갈린다).
        let a = if !span1 { line - 1 } else { line + 1 };
        self.join_to_line(cell_idx, axis, line, a)?;
        self.collapse_dead_lines(axis);
        Ok(())
    }

    /// 죽은 내부 선(어떤 셀도 경계로 쓰지 않는 선, `TableGrid::dead_lines`)을 접는다 — 복원·합류·통과 뒷정리.
    /// 접을 선이 없어질 때까지 반복(선 수 단조 감소).
    pub(crate) fn collapse_dead_lines(&mut self, axis: Axis) {
        let cols = axis == Axis::Cols;
        loop {
            let Some(line) = TableGrid::lines_only(self).dead_lines(axis).first().copied() else { break };
            // 줄 `line` 과 `line-1` 사이 경계선이 미사용 → 줄 line 을 접는다
            for c in self.cells.iter_mut() {
                let (start, span) = if cols {
                    (&mut c.col, &mut c.col_span)
                } else {
                    (&mut c.row, &mut c.row_span)
                };
                if *start >= line {
                    *start -= 1;
                } else if *start + *span > line {
                    *span -= 1;
                }
            }
            if cols {
                self.col_count -= 1;
            } else {
                self.row_count -= 1;
            }
            self.rebuild_row_sizes();
            self.rebuild_grid();
        }
        // 접힌 뒤 표 치수 재계산 — join_to_line 의 finish_cells 는 죽은 선이 살아 있을 때 돌아 stale 치수를 남긴다.
        self.update_ctrl_dimensions();
    }

    pub fn split_cells_in_range(
        &mut self,
        start_row: u16,
        start_col: u16,
        end_row: u16,
        end_col: u16,
        n_rows: u16,
        m_cols: u16,
        equal_row_height: bool,
    ) -> Result<(), String> {
        if n_rows < 1 || m_cols < 1 {
            return Err("분할 행/열 수는 1 이상이어야 합니다".to_string());
        }
        if n_rows == 1 && m_cols == 1 {
            return Ok(());
        }

        // 열 우선 순서: 우측→좌측 열, 각 열 내에서 하단→상단
        // 같은 열 내 분할은 col을 시프트하지 않고 col_span만 확장하므로 안전.
        // 우측 열 처리 후 좌측 열의 셀 col은 아직 원래 값을 유지한다.
        for c in (start_col..=end_col).rev() {
            // 행 분할: 하단→상단 (같은 행 내 분할은 row_span만 확장)
            for r in (start_row..=end_row).rev() {
                if !self.cells.iter().any(|cell| cell.col == c && cell.row == r) {
                    continue;
                }
                self.split_cell_into(r, c, n_rows, m_cols, equal_row_height, false)?;
            }
        }

        Ok(())
    }
}
