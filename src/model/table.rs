//! 표 (Table, Cell, Row)

use super::paragraph::Paragraph;
use super::shape::{common_obj_offsets, Caption};
use super::*;

pub const CELL_FLAG_HAS_MARGIN: u16 = 0x0001;
pub const CELL_FLAG_PROTECT: u16 = 0x0002;
pub const CELL_FLAG_HEADER: u16 = 0x0004;
pub const CELL_FLAG_EDITABLE_IN_FORM: u16 = 0x0008;

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
    /// Studio 보상 resize로 행별 독립 가로 경계를 보존해야 하는 행.
    #[doc(hidden)]
    pub local_resize_rows: Vec<u16>,
    /// Studio 보상 resize로 열별 독립 세로 경계를 보존해야 하는 열.
    #[doc(hidden)]
    pub local_resize_cols: Vec<u16>,
    /// Studio 로컬 가로 resize 후 셀별 목표 표시 폭(HWPUNIT).
    #[doc(hidden)]
    pub local_resize_cell_widths: Vec<(usize, u32)>,
    /// Studio 로컬 세로 resize 후 셀별 목표 표시 높이(HWPUNIT).
    #[doc(hidden)]
    pub local_resize_cell_heights: Vec<(usize, u32)>,
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

impl Table {
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

    /// 저장/복구 후 Studio 런타임 힌트가 사라진 행 단위 가로 resize를 보수적으로 추론한다.
    ///
    /// 한컴 HWP5에는 `local_resize_rows` 같은 rhwp 내부 힌트를 저장할 곳이 없다. 따라서
    /// 같은 셀 배치 패턴을 공유하는 행들 중 다수의 폭 벡터와 다른 소수 행만 행 단위
    /// resize 결과로 간주한다. 병합 패턴이 유일한 행은 원본 문서 구조일 가능성이 높아
    /// 추론 대상에서 제외한다.
    pub fn inferred_local_resize_rows(&self) -> Vec<u16> {
        let col_count = self.col_count as usize;
        if col_count == 0 || self.row_count == 0 {
            return Vec::new();
        }

        let explicit_rows = self
            .local_resize_rows
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>();
        let mut grouped_rows =
            std::collections::BTreeMap::<Vec<(u16, u16)>, Vec<(u16, Vec<u32>)>>::new();

        for row in 0..self.row_count {
            if explicit_rows.contains(&row) {
                continue;
            }

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
        self.local_resize_rows.clear();
        self.local_resize_cols.clear();
        self.local_resize_cell_widths.clear();
        self.local_resize_cell_heights.clear();
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
        if self.raw_ctrl_data.len() < common_obj_offsets::HEIGHT.end {
            return;
        }
        let total_width: HwpUnit = self.get_column_widths().iter().sum();
        let total_height: HwpUnit = self.effective_row_heights().iter().sum();
        // (1) serialize source — raw_ctrl_data bytes (HWP 직렬화 시 사용).
        self.raw_ctrl_data[common_obj_offsets::WIDTH].copy_from_slice(&total_width.to_le_bytes());
        self.raw_ctrl_data[common_obj_offsets::HEIGHT].copy_from_slice(&total_height.to_le_bytes());
        // (2) [Task #1151 v6] paragraph_layout cache — self.common.width/height.
        // v3 helper (calc_sibling_topandbottom_table_reserved_hu) 가 self.common.height 사용.
        // dual maintenance 가 필수 — 한쪽만 갱신 시 stale 결함.
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

    pub fn get_column_widths(&self) -> Vec<HwpUnit> {
        let mut widths = vec![0u32; self.col_count as usize];
        for cell in &self.cells {
            if cell.col_span == 1 && (cell.col as usize) < widths.len() {
                if cell.width > widths[cell.col as usize] {
                    widths[cell.col as usize] = cell.width;
                }
            }
        }
        // [경계선 재설계 2026-08-04] 어긋낸 표의 조각 열은 단독(span1) 목격자가 없다 —
        // 기본값으로 채우면 update_ctrl_dimensions 를 부르는 다음 연산(일반 드래그 등)이
        // 표를 슬쩍 키운다(신고: 오른쪽 끝이 커짐). 병합 셀 제약으로 먼저 푼다.
        self.solve_span_gaps(&mut widths, true);
        // 그래도 폭이 0인 열은 기본값 1800 HWPUNIT (약 6.35mm)
        for w in &mut widths {
            if *w == 0 {
                *w = 1800;
            }
        }
        widths
    }

    /// 목격자 없는 열/행 크기를 병합 셀 제약(구간 합 = 셀 크기)으로 채운다.
    /// 정확히 한 구간만 미지수인 병합 셀부터 반복 해소 — 레이아웃 솔버의 모델판.
    fn solve_span_gaps(&self, sizes: &mut [HwpUnit], cols: bool) {
        loop {
            let mut progressed = false;
            for cell in &self.cells {
                let (start, span, total) = if cols {
                    (cell.col as usize, cell.col_span as usize, cell.width)
                } else {
                    (cell.row as usize, cell.row_span as usize, cell.height)
                };
                if span < 2 || start + span > sizes.len() {
                    continue;
                }
                let unknown: Vec<usize> =
                    (start..start + span).filter(|&i| sizes[i] == 0).collect();
                if unknown.len() == 1 {
                    let known: u32 = (start..start + span).map(|i| sizes[i]).sum();
                    if total > known {
                        sizes[unknown[0]] = total - known;
                        progressed = true;
                    }
                }
            }
            if !progressed {
                break;
            }
        }
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
    /// [2026-08-16] 이 격자선이 **어긋난 선**인가 — 내부 선인데 한 열만 경계로 쓴다.
    fn is_misaligned_line(&self, line: u16) -> bool {
        if line == 0 || line >= self.row_count {
            return false; // 바깥 테두리
        }
        let mut first_col: Option<u16> = None;
        for c in &self.cells {
            if c.row == line || c.row + c.row_span == line {
                match first_col {
                    None => first_col = Some(c.col),
                    Some(fc) if fc != c.col => return false,
                    _ => {}
                }
            }
        }
        first_col.is_some()
    }

    /// [2026-08-16] 이 행이 **어긋내기 조각 행**인가 — 위 또는 아래 격자선이 어긋난
    /// 선(한 열 전용)이면 조각이다. 조각 행은 저장 높이가 곧 실효 높이라 글줄 바닥·
    /// 빈 lineseg 성장(1000HU)을 적용하면 줄인 조각이 도로 부풀어 표가 자란다
    /// (3×3 전수 실측: 모델 합은 보존인데 렌더만 +2.7px). 정상 병합 행은 인접 선을
    /// 여러 열이 쓰므로 종전대로 바닥·성장이 적용된다. 1열 표는 제외.
    pub(crate) fn is_stagger_piece_row(&self, row: usize) -> bool {
        if self.col_count < 2 {
            return false;
        }
        self.is_misaligned_line(row as u16) || self.is_misaligned_line(row as u16 + 1)
    }

    /// [2026-08-16 합류] 이 격자선이 **부분 공유선**인가 — 내부 선을 일부 열은
    /// 경계로 쓰고 일부 열은 스팬으로 관통한다. 두 열이 같은 낙하점에 합류하면
    /// 어긋선이 공유선이 되어 is_misaligned_line 에서 빠지는데, 그 행 역시
    /// 어긋내기 산물이라 빈 셀 글줄 바닥을 강제하면 표가 자란다(합류 실측 +7.6px).
    fn is_partially_shared_line(&self, line: u16) -> bool {
        if line == 0 || line >= self.row_count {
            return false;
        }
        let bordered = self
            .cells
            .iter()
            .any(|c| c.row == line || c.row + c.row_span == line);
        let spanned = self
            .cells
            .iter()
            .any(|c| c.row < line && c.row + c.row_span > line);
        bordered && spanned
    }

    /// [2026-08-16 합류] 어긋내기 **합류 산물 행**인가 — 인접 선이 부분 공유선.
    /// 사용자 세로 병합 곁 행도 잡히므로, 호출부는 저장 높이가 명시(> 패딩 규약)인
    /// 빈 셀에만 이 판정으로 글줄 바닥을 면제한다(빈 셀 규약 284 는 종전대로 성장).
    pub(crate) fn is_stagger_joined_row(&self, row: usize) -> bool {
        if self.col_count < 2 {
            return false;
        }
        self.is_partially_shared_line(row as u16) || self.is_partially_shared_line(row as u16 + 1)
    }

    pub fn row_line_floors_hu(&self) -> Vec<u32> {
        const DEFAULT_LINE_HU: u32 = 1000;
        let floors = self.cell_content_floors_hu();
        let row_count = self.row_count as usize;
        let mut out = vec![0u32; row_count];
        for (row, slot) in out.iter_mut().enumerate() {
            let spanned_through = self
                .cells
                .iter()
                .any(|c| (c.row as usize) < row && (c.row as usize + c.row_span as usize) > row);
            if spanned_through {
                continue;
            }
            if self.is_stagger_piece_row(row) {
                continue;
            }
            let pad_vert: u32 = self
                .cells
                .iter()
                .filter(|c| c.row as usize == row && c.row_span <= 1)
                .map(|c| {
                    let p = c.effective_padding(&self.padding);
                    (p.top.max(0) + p.bottom.max(0)) as u32
                })
                .max()
                .unwrap_or((self.padding.top.max(0) + self.padding.bottom.max(0)) as u32);
            let content_floor: u32 = self
                .cells
                .iter()
                .enumerate()
                .filter(|(_, c)| c.row as usize == row && c.row_span <= 1)
                .map(|(i, _)| floors.get(i).copied().unwrap_or(0))
                .max()
                .unwrap_or(0);
            *slot = (pad_vert + DEFAULT_LINE_HU).max(content_floor);
        }
        out
    }

    pub fn effective_row_heights(&self) -> Vec<HwpUnit> {
        const DEFAULT_LINE_HU: u32 = 1000;
        let floors = self.cell_content_floors_hu();
        let mut heights = self.get_row_heights();
        for (row, h) in heights.iter_mut().enumerate() {
            // [경계선 어긋내기 예외 2026-08-12] **걸침 셀**(위 행에서 시작해 이 행에
            // 걸치는 row_span 셀)이 있는 행은 어긋내기(offset_cell_boundary)가 만든
            // 조각 행이다 — 저장 높이가 곧 실효 높이이므로 글줄 바닥을 적용하지
            // 않는다. 종전엔 조각 행(예: 800/284HU)까지 1284 로 부풀려 어긋낸 표의
            // common.height 가 실제(2852)보다 크게(5136) 기록됐다(신고 2026-08-12).
            // 빈 규약 행(전 셀이 이 행에서 시작)은 종전대로 바닥 적용.
            let spanned_through = self
                .cells
                .iter()
                .any(|c| (c.row as usize) < row && (c.row as usize + c.row_span as usize) > row);
            if spanned_through {
                continue;
            }
            if self.is_stagger_piece_row(row) {
                continue;
            }
            let pad_vert: u32 = self
                .cells
                .iter()
                .filter(|c| c.row as usize == row && c.row_span <= 1)
                .map(|c| {
                    let p = c.effective_padding(&self.padding);
                    (p.top.max(0) + p.bottom.max(0)) as u32
                })
                .max()
                .unwrap_or((self.padding.top.max(0) + self.padding.bottom.max(0)) as u32);
            // [2026-08-13] 바닥 = max(10pt 고정 하한, 행 내 셀 콘텐츠 바닥(lineseg 실측)).
            // 10pt 고정(pad+1000)만 쓰면 12pt 문서에서 측정기(콘텐츠 바닥 1484)와 200HU
            // 어긋나 common.height 가 시각 합과 달라졌다(어긋내기 후 균등 폴백의 한 뿌리).
            let content_floor: u32 = self
                .cells
                .iter()
                .enumerate()
                .filter(|(_, c)| c.row as usize == row && c.row_span <= 1)
                .map(|(i, _)| floors.get(i).copied().unwrap_or(0))
                .max()
                .unwrap_or(0);
            *h = (*h).max((pad_vert + DEFAULT_LINE_HU).max(content_floor));
        }
        heights
    }

    /// [2026-08-16] 행 `r` 를 위에서 `off`(HU) 지점에서 **이분할**해 새 격자선을 만든다.
    ///
    /// 신규 어긋내기의 낙하점이 스팬 이웃의 내부 행에 떨어질 때 쓴다 — 종전
    /// split_cell_into(균등 분할)로는 낙하점 위치의 선을 만들 수 없어, 이웃이 다른
    /// 열의 어긋 조각에 걸친 스팬이면 격자가 모순돼 트랜잭션이 거부했다(신고
    /// "오른쪽 어긋내기가 기준(첫 어긋선)을 못 넘어간다").
    ///
    /// 규약: **col 밴드**의 r 시작 span1 셀만 (off, h−off) 두 조각(내용은 **아래**
    /// 조각 — "내용은 잔여에" 규약)으로 실제 분할하고, 나머지 셀은 전부 span+1 로
    /// 관통시킨다 — 어긋선(한 열만 쓰는 내부 선) 모델 유지. 전 열을 쪼개면 빈 조각
    /// 행에 글줄 바닥이 되살아나 실효 높이가 커진다. 호출 전 연루 행을 물질화할
    /// 것(원시 284 규약 상태로 나누면 장부가 갈린다).
    fn insert_row_line(&mut self, r: u16, off: i32, col: u16, col_span: u16) -> Result<(), String> {
        if off <= 0 {
            return Err("분할 오프셋이 0 이하".to_string());
        }
        let band = col..col + col_span;
        let mut new_cells: Vec<Cell> = Vec::with_capacity(self.cells.len() + 1);
        for c in &self.cells {
            if c.row > r {
                let mut c2 = c.clone();
                c2.row += 1;
                new_cells.push(c2);
            } else if c.row == r && c.row_span == 1 && band.contains(&c.col) {
                if (c.height as i32) <= off {
                    return Err("행 분할 오프셋이 행 높이 이상".to_string());
                }
                // 위 조각(빈) + 아래 조각(내용 유지)
                let mut top = c.clone();
                top.height = off as HwpUnit;
                top.paragraphs = vec![crate::model::paragraph::Paragraph::default()];
                let mut bot = c.clone();
                bot.row = r + 1;
                bot.height = (c.height as i32 - off) as HwpUnit;
                new_cells.push(top);
                new_cells.push(bot);
            } else if c.row + c.row_span > r {
                // r 을 덮는 나머지 셀(다른 열/스팬) — 관통 스팬으로
                let mut c2 = c.clone();
                c2.row_span += 1;
                new_cells.push(c2);
            } else {
                new_cells.push(c.clone());
            }
        }
        self.cells = new_cells;
        self.row_count += 1;
        self.rebuild_grid();
        Ok(())
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
        let mut heights = self.get_raw_row_heights();
        // [경계선 재설계 2026-08-04] 어긋낸 표의 조각 행 — 열과 같은 제약 해소
        self.solve_span_gaps(&mut heights, false);
        // 그래도 높이가 0인 행은 기본값 400 HWPUNIT
        for h in &mut heights {
            if *h == 0 {
                *h = 400;
            }
        }
        heights
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
    fn rebuild_row_sizes(&mut self) {
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

        // 새 셀 생성: 병합 셀에 의해 커버되지 않는 열에만
        // 삽입 지점 아래 행의 셀을 템플릿으로 우선 사용 (헤더 행 대신 데이터 행)
        // target_row 아래(+1)의 셀이 원래 데이터 행이므로 먼저 시도, 없으면 위(-1), 그래도 없으면 아무 셀
        for c in 0..self.col_count {
            if !covered_cols[c as usize] {
                let width = col_widths[c as usize];
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
                let new_cell = if let Some(tpl) = template {
                    Cell::new_from_template(c, target_row, width, new_cell_height, tpl)
                } else {
                    // 셀이 하나도 없는 표 — 상속원이 존재하지 않는 유일한 경우
                    Cell::new_empty(c, target_row, width, new_cell_height, self.border_fill_id)
                };
                self.cells.push(new_cell);
            }
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
        // 내림 잔여분은 가장 넓은 열에 몰아 합을 정확히 되돌린다. MIN_COLUMN_WIDTH 바닥에
        // 닿으면 더 줄이지 않는다 — 열이 지나치게 많을 때만 폭이 조금 는다(가독성 우선).
        const MIN_COLUMN_WIDTH: u32 = 200;
        if original_total_width > 0 {
            let mut widths = self.get_column_widths();
            let grown: u64 = widths.iter().map(|w| *w as u64).sum();
            if grown > original_total_width {
                for w in &mut widths {
                    let scaled = (*w as u64 * original_total_width) / grown;
                    *w = (scaled as u32).max(MIN_COLUMN_WIDTH);
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
        let deleted_row_height = self
            .get_raw_row_heights()
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

        // 열폭/행높이 합산 (원본 값 보존: 0은 fallback 없이 그대로 유지)
        let col_widths = self.get_column_widths();
        let raw_row_heights = self.get_raw_row_heights();
        let new_width: HwpUnit = (start_col..=end_col)
            .map(|c| col_widths.get(c as usize).copied().unwrap_or(0))
            .sum();
        let new_height: HwpUnit = (start_row..=end_row)
            .map(|r| raw_row_heights.get(r as usize).copied().unwrap_or(0))
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
    fn is_boundary_aligned(&self, t: &Cell, boundary: u16, edge_right: bool) -> bool {
        self.cells.iter().any(|c| {
            if edge_right {
                c.row != t.row && (c.col == boundary || c.col + c.col_span == boundary)
            } else {
                c.col != t.col && (c.row == boundary || c.row + c.row_span == boundary)
            }
        })
    }

    /// 이미 어긋난 경계의 **재이동** — 격자(행·열 수, span)를 그대로 두고 대상/이웃의
    /// 크기만 delta 만큼 주고받는다. 열 폭·행 높이는 span 제약으로 정확히 유도되므로
    /// 표 전체 크기가 보존된다(신고 ③ 성장 종결). 이동 결과가 원래 정렬선에 닿거나
    /// 넘으면 restore(치유)로 승격해 격자를 원래대로 접는다(신고 ① 복귀).
    fn shift_offset_boundary(
        &mut self,
        cell_idx: usize,
        n_idx: usize,
        boundary: u16,
        edge_right: bool,
        delta: i32,
    ) -> Result<(), String> {
        const MIN_CELL: i32 = 200;
        let t = self.cells[cell_idx].clone();
        let n = self.cells[n_idx].clone();
        let size = |c: &Cell| if edge_right { c.width } else { c.height } as i32;

        // 원래 정렬선까지의 대상 크기 — 다른 줄이 쓰는 격자선 중 대상 시작 뒤 첫 번째.
        let sizes: Vec<u32> = if edge_right {
            self.get_column_widths()
        } else {
            self.get_row_heights()
        };
        let start = if edge_right { t.col } else { t.row };
        let mut aligned_lines: Vec<u16> = self
            .cells
            .iter()
            .filter(|c| {
                if edge_right {
                    c.row != t.row
                } else {
                    c.col != t.col
                }
            })
            .flat_map(|c| {
                if edge_right {
                    [c.col, c.col + c.col_span]
                } else {
                    [c.row, c.row + c.row_span]
                }
            })
            .filter(|&l| l > start)
            .collect();
        aligned_lines.sort_unstable();
        let aligned_size: Option<i32> = aligned_lines.first().map(|&line| {
            sizes[start as usize..(line as usize).min(sizes.len())]
                .iter()
                .sum::<u32>() as i32
        });

        let new_t = size(&t) + delta;
        // 정렬선 통과 → 치유(복원)로 승격. 판정은 **어긋남의 부호**로 한다 — 크기 비교
        // (new_t >= aligned)로 하면 이미 오른쪽으로 어긋난 상태에서 더 오른쪽으로 갈 때도
        // 항상 참이 되어 두 번째 스텝마다 제자리로 튕겼다(2026-08-13 실측 R3-2).
        if let Some(aw) = aligned_size {
            let cur_off = size(&t) - aw;
            let next_off = new_t - aw;
            if cur_off != 0 && (next_off == 0 || cur_off.signum() != next_off.signum()) {
                // [2026-08-16] 승격은 **실제로 복원 가능할 때만**. 병합이 섞인 표는 격자상
                // 어긋남과 구분되지 않아 여기 오는데, 복원(조각 되접기)이 불가능하면 즉시
                // Err 로 드래그가 통째로 거부됐다. 클론에 시험해 되면 채택, 안 되면 일반
                // 재이동으로 계속한다(표는 작아 클론이 싸다).
                let mut probe = self.clone();
                if probe.restore_cell_boundary(cell_idx, edge_right).is_ok() {
                    *self = probe;
                    return Ok(());
                }
            }
        }

        // 최소 크기 클램프 — 남는 쪽이 규약 밑으로 못 간다.
        //
        // [실효 공간 2026-08-16] 여유는 **실효 높이**로 잰다. 원시 stored 로 재면 한컴
        // 저장 규약(빈 셀=패딩만 284)에서 바닥이 더 커서 여유가 음수가 되고, delta<0
        // 분기의 부호 반전이 그 음수를 양수 d 로 뒤집어 — 드래그가 반대 방향으로 가고
        // 상대 셀 높이가 u32 언더플로(4.29e9)했다(3×3 재드래그 실측). 조각 최소는
        // 신규 어긋내기와 같은 내용 인지 기준(stagger_piece_floor_hu).
        let floors = self.cell_content_floors_hu();
        let eff_rows = if edge_right {
            Vec::new()
        } else {
            self.effective_row_heights()
        };
        let floor_of = |idx: usize| -> i32 {
            if edge_right {
                MIN_CELL
            } else {
                self.stagger_piece_floor_hu(idx, &floors, MIN_CELL)
            }
        };
        let space_of = |c: &Cell| -> i32 {
            if edge_right {
                c.width as i32
            } else {
                let eff = eff_rows.get(c.row as usize).copied().unwrap_or(c.height);
                c.height.max(eff) as i32
            }
        };
        let d = if delta > 0 {
            delta.min((space_of(&n) - floor_of(n_idx)).max(0))
        } else {
            -((-delta).min((space_of(&t) - floor_of(cell_idx)).max(0)))
        };
        // 클램프가 부호를 뒤집었으면(여유 음수) 움직이지 않는다 — 반대 방향 이동 금지
        if d != 0 && d.signum() != delta.signum() {
            return Err("이 방향으로는 더 옮길 수 없습니다".to_string());
        }
        if d == 0 {
            return Err(if delta > 0 {
                if edge_right {
                    "이웃 칸에 남는 폭이 없습니다".to_string()
                } else {
                    "이웃 칸에 남는 높이가 없습니다".to_string()
                }
            } else if edge_right {
                "대상 칸에 남는 폭이 없습니다".to_string()
            } else {
                "대상 칸에 남는 높이가 없습니다".to_string()
            });
        }
        let _ = boundary;
        if edge_right {
            self.cells[cell_idx].width = (size(&t) + d) as HwpUnit;
            self.cells[n_idx].width = (size(&n) - d) as HwpUnit;
        } else {
            // 판정은 실효 공간에서 끝냈다(무부작용). 통과했으니 연루 행을 물질화하고
            // **같은 공간의 값**으로 주고받는다 — 안 그러면 판정은 실효, 쓰기는 원시라
            // 두 장부가 갈린다.
            let lo = (t.row as usize).min(n.row as usize);
            let hi = ((t.row + t.row_span) as usize).max((n.row + n.row_span) as usize);
            self.materialize_rows_effective(lo, hi.saturating_sub(1));
            let t_now = self.cells[cell_idx].height as i32;
            let n_now = self.cells[n_idx].height as i32;
            self.cells[cell_idx].height = (t_now + d).max(MIN_CELL) as HwpUnit;
            self.cells[n_idx].height = (n_now - d).max(MIN_CELL) as HwpUnit;
        }
        self.rebuild_grid();
        self.update_ctrl_dimensions();
        Ok(())
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
        let saved = self.clone();
        let w0: u64 = self.get_column_widths().iter().map(|&w| w as u64).sum();
        let h0: u64 = self.effective_row_heights().iter().map(|&h| h as u64).sum();
        let r = self.offset_cell_boundary_core(cell_idx, edge_right, delta);
        if r.is_ok() && !self.stagger_invariants_hold(w0, h0) {
            *self = saved;
            return Err("이 조작은 표 격자를 깨뜨려 취소했습니다".to_string());
        }
        if r.is_err() {
            *self = saved; // 부작용 없는 실패 보장
        }
        r
    }

    /// 어긋내기 후 표가 온전한가 — 폭 합·실효 높이 합 보존(±4HU), 셀 크기 건전.
    fn stagger_invariants_hold(&self, w0: u64, h0: u64) -> bool {
        let w1: u64 = self.get_column_widths().iter().map(|&w| w as u64).sum();
        let h1: u64 = self.effective_row_heights().iter().map(|&h| h as u64).sum();
        if w1.abs_diff(w0) > 4 || h1.abs_diff(h0) > 4 {
            return false;
        }
        self.cells
            .iter()
            .all(|c| c.width < 1_000_000 && c.height < 1_000_000)
    }

    fn offset_cell_boundary_core(
        &mut self,
        cell_idx: usize,
        edge_right: bool,
        delta: i32,
    ) -> Result<(), String> {
        const MIN_CELL: i32 = 200; // resize_table_cells 와 같은 최소 크기
        if delta == 0 {
            return Ok(());
        }
        let t = self
            .cells
            .get(cell_idx)
            .ok_or_else(|| "셀 인덱스가 유효하지 않습니다".to_string())?
            .clone();

        if edge_right {
            // ── 오른쪽 경계 (열 방향) ──
            let boundary = t.col + t.col_span;
            if boundary >= self.col_count {
                return Err("바깥 테두리는 어긋낼 수 없습니다".to_string());
            }
            let n_idx = self
                .cell_index_at(t.row, boundary)
                .ok_or_else(|| "오른쪽 이웃 셀을 찾지 못했습니다".to_string())?;
            let n = self.cells[n_idx].clone();
            if n.row != t.row || n.row_span != t.row_span {
                return Err("위아래 높이가 다른 칸과는 경계를 어긋낼 수 없습니다".to_string());
            }
            // ── [재이동 2026-08-13] 이 경계가 **이미 어긋나 있으면** 격자를 다시 쪼개지
            // 않는다. 종전엔 매번 split_cell_into 로 새 격자선을 만들어 ① 죽은 선이 쌓이고
            // (열 폭 미해소 → 1800 기본값 → 표 폭 성장, 신고 ③) ② 반대 방향은 span 가드로
            // 전면 거부돼 제자리로 못 돌아왔다(신고 ①). 격자는 그대로 두고 **조각 폭만
            // 이전**하면 열 폭이 span 제약으로 정확히 유도되어 표 폭이 보존되고, 양방향
            // 재이동이 자유로워진다. 정렬선에 닿으면 restore(치유)로 자동 승격한다.
            if !self.is_boundary_aligned(&t, boundary, true) {
                return self.shift_offset_boundary(cell_idx, n_idx, boundary, true, delta);
            }
            if delta > 0 {
                // [2026-08-17 저녁 철회] "낙하점 도달 시 이웃 조각 전체 흡수(병합)"를
                // 같은 날 넣었다가 되돌렸다 — 어긋내기가 셀을 **합쳐 버리면** 사용자
                // 규칙("합치기를 하지 않는 한 셀 9개 유지, 순회 1..9") 위반이고, 얇은
                // 행 표에선 작은 드래그도 흡수 문턱을 넘어 a2 가 a1 으로 사라졌다
                // (사용자 신고). 신규 어긋내기는 이웃 최소에서 멈추는 게 정본.
                // 이웃 왼쪽 조각을 잘라 대상에 흡수.
                // [신고 ② 2026-08-13] 이웃이 **남의 어긋남**으로 span>1 인 경우(다른 행이
                // 어긋나며 이 이웃이 여러 격자열을 걸치게 된 상태)도 어긋낼 수 있어야 한다 —
                // 종전 `n.col_span != 1` 거부는 "한 행을 어긋내면 다른 행의 같은 경계가
                // 전부 막히는" 과잉 차단이었다. 이웃 span 만큼 조각을 낸 뒤 첫 조각만
                // 대상이 흡수하고 나머지는 도로 합친다(이웃 겉모습 불변).
                let d = delta.min(n.width as i32 - MIN_CELL);
                if d <= 0 {
                    return Err("이웃 칸에 남는 폭이 없습니다".to_string());
                }
                let n_span = n.col_span;
                self.split_cell_into(n.row, n.col, 1, n_span + 1, true, false)?;
                let left = self
                    .cell_index_at(t.row, boundary)
                    .ok_or("분할 조각(좌) 소실")?;
                self.cells[left].width = d as HwpUnit;
                if n_span > 1 {
                    // 나머지 조각을 원래 한 칸으로 복구(겉모습 불변)
                    self.merge_cells(
                        t.row,
                        boundary + 1,
                        t.row + t.row_span - 1,
                        boundary + n_span,
                    )?;
                }
                let right = self
                    .cell_index_at(t.row, boundary + 1)
                    .ok_or("분할 조각(우) 소실")?;
                self.cells[right].width = (n.width as i32 - d) as HwpUnit;
                // split 은 내용을 첫 조각에 남긴다 — 흡수될 조각은 비우고 이웃 조각에 내용을 되돌린다
                if left != right {
                    let (a, b) = if left < right {
                        let (x, y) = self.cells.split_at_mut(right);
                        (&mut x[left].paragraphs, &mut y[0].paragraphs)
                    } else {
                        let (x, y) = self.cells.split_at_mut(left);
                        (&mut y[0].paragraphs, &mut x[right].paragraphs)
                    };
                    std::mem::swap(a, b);
                }
                self.merge_cells(t.row, t.col, t.row + t.row_span - 1, boundary)?;
                let merged = self.cell_index_at(t.row, t.col).ok_or("병합 결과 소실")?;
                // merge 는 목격자 없는 열(raw 0)을 합산해 폭을 어림한다 — 정확값으로 못박는다
                self.cells[merged].width = (t.width as i32 + d) as HwpUnit;
            } else {
                // 대상 오른쪽 조각을 잘라 이웃에 넘김 (여기는 정렬된 경계 = 신규 어긋내기.
                // 이미 어긋난 경계의 역방향은 위 재이동 경로가 처리한다)
                let d = (-delta).min(t.width as i32 - MIN_CELL);
                if d <= 0 {
                    return Err("대상 칸에 남는 폭이 없습니다".to_string());
                }
                self.split_cell_into(t.row, t.col, 1, 2, true, false)?;
                let left = self
                    .cell_index_at(t.row, t.col)
                    .ok_or("분할 조각(좌) 소실")?;
                self.cells[left].width = (t.width as i32 - d) as HwpUnit;
                let strip = self
                    .cell_index_at(t.row, t.col + 1)
                    .ok_or("분할 조각(우) 소실")?;
                self.cells[strip].width = d as HwpUnit;
                // 이웃은 새 열 삽입으로 한 칸 밀렸다: boundary+1 에서 시작, 스팬 유지
                self.merge_cells(
                    t.row,
                    t.col + 1,
                    t.row + t.row_span - 1,
                    boundary + n.col_span,
                )?;
                let merged = self
                    .cell_index_at(t.row, t.col + 1)
                    .ok_or("병합 결과 소실")?;
                self.cells[merged].width = (n.width as i32 + d) as HwpUnit;
                // 빈 조각이 병합 기준(primary)이라 이웃 내용이 뒤로 밀린다 — 선두 빈 문단 제거
                if self.cells[merged].paragraphs.len() > 1
                    && self.cells[merged].paragraphs[0].text.is_empty()
                {
                    self.cells[merged].paragraphs.remove(0);
                }
            }
        } else {
            // ── 아래 경계 (행 방향) ── (열 코드와 대칭)
            let boundary = t.row + t.row_span;
            if boundary >= self.row_count {
                return Err("바깥 테두리는 어긋낼 수 없습니다".to_string());
            }
            // [실효 공간 2026-08-13] 행은 빈 셀 저장 규약(height=패딩만 284)이라 원시 모델로
            // 자르면 화면(글줄 바닥 1484)과 다른 세계에서 산술이 돈다 — ① 화면 무동작인데
            // 모델만 몰래 어긋나고(한계 84) ② 조각·바닥이 얽혀 다른 행 경계까지 밀렸다
            // (신고 "어긋내기하면 다른 경계선마저 커져"). 한계는 실효값으로 **먼저** 판정해
            // 실패 시 부작용을 남기지 않고, 통과 시에만 연루 행을 실효 높이로 물질화한 뒤
            // 갱신값으로 자른다. 조각 한계 = 콘텐츠 글줄 바닥(내용이 남는 조각 기준).
            let n_idx = self
                .cell_index_at(boundary, t.col)
                .ok_or_else(|| "아래 이웃 셀을 찾지 못했습니다".to_string())?;
            let n = self.cells[n_idx].clone();
            if n.col != t.col || n.col_span != t.col_span {
                return Err("좌우 폭이 다른 칸과는 경계를 어긋낼 수 없습니다".to_string());
            }
            // [재이동 2026-08-13] 열과 동일 — 이미 어긋난 경계는 격자 불변·조각 높이 이전
            if !self.is_boundary_aligned(&t, boundary, false) {
                // [2026-08-16 신고 "기준선을 못 넘어감"] 재이동이 다른 열의 선을 **통과**
                // 해야 하면(두 선 사이 행이 0 이하로 붕괴) 조각 이전으로는 표현 불가 —
                // 클론 프로브로 선판정하고(슬리버 행 신설도 불합격), 실패 시 복원 후
                // 낙하점 재어긋내기로 폴백해 통과를 지원한다.
                let w0: u64 = self.get_column_widths().iter().map(|&w| w as u64).sum();
                let h0: u64 = self.effective_row_heights().iter().map(|&h| h as u64).sum();
                let thin = |tb: &Table| {
                    tb.effective_row_heights()
                        .iter()
                        .filter(|&&h| (h as i32) < MIN_CELL)
                        .count()
                };
                let thin0 = thin(self);
                let eff_span = |tb: &Table, row: u16, span: u16| -> i32 {
                    let e = tb.effective_row_heights();
                    (row..row + span)
                        .map(|r| e.get(r as usize).copied().unwrap_or(0) as i32)
                        .sum()
                };
                let span0 = eff_span(self, t.row, t.row_span);
                let mut probe = self.clone();
                let r = probe.shift_offset_boundary(cell_idx, n_idx, boundary, false, delta);
                // 프로브 합격 조건에 **전달 거리**도 본다 — shift 는 인접 행 창으로
                // 델타를 클램프하므로, 큰 드래그가 다음 선에서 "정상 커밋"으로 잘리면
                // 통과 이동이 조용히 증발한다(col2 실측 2250→566). 못 채우면 폴백.
                let delivered = eff_span(&probe, t.row, t.row_span) - span0;
                if r.is_ok()
                    && probe.stagger_invariants_hold(w0, h0)
                    && thin(&probe) <= thin0
                    && (delivered - delta).abs() <= MIN_CELL
                {
                    *self = probe;
                    return Ok(());
                }
                // cum(현재 어긋 오프셋)은 **실효 공간**으로 잰다 — 빈 셀 저장 규약
                // (height=패딩만 284) 상태의 열은 원시 저장으로 재면 값이 갈려,
                // "정렬 복귀"로 오판해 복원만 커밋되고 나머지 이동이 증발했다
                // (col2 실측: +30px 드래그가 +7px 합류에서 정지).
                let h_before = span0;
                self.restore_cell_boundary_core(cell_idx, false)?;
                let t_idx = self
                    .cell_index_at(t.row, t.col)
                    .ok_or("복원 후 대상 소실")?;
                let t2 = self.cells[t_idx].clone();
                let cum = h_before - eff_span(self, t2.row, t2.row_span);
                #[cfg(not(target_arch = "wasm32"))]
                eprintln!(
                    "[fallback] cell=({},{}) span{} boundary={} delta={} cum={} shift_err={:?}",
                    t.row,
                    t.col,
                    t.row_span,
                    boundary,
                    delta,
                    cum,
                    r.err()
                );
                if (cum + delta).abs() <= MIN_CELL {
                    return Ok(()); // 정렬선 ±MIN_CELL 이내 복귀 = 치유(스냅)
                }
                return self.offset_cell_boundary_core(t_idx, false, cum + delta);
            }
            let eff_rows = self.effective_row_heights();
            let floors = self.cell_content_floors_hu();
            if delta > 0 {
                // [2026-08-16 재작성] **낙하점 기반** — 이웃이 다른 열의 어긋 조각에 걸친
                // 스팬이어도, 낙하점이 그 스팬의 몇 번째 행이든 정확히 그 자리에 선을
                // 만든다(기존 내부 선과 일치하면 재사용 = 합류). 종전 split_cell_into
                // (균등 분할)는 낙하점 위치를 표현 못 해 격자가 모순 → 트랜잭션 거부 →
                // "오른쪽 어긋내기가 기준을 못 넘어감"(신고).
                //
                // 내용 규약: 대상이 흡수하는 구간(그 열의 L..새 선)의 내용은 **아래
                // 잔여 조각**으로 옮긴다 — "내용은 잔여에 남는다".
                let n_floor = self.stagger_piece_floor_hu(n_idx, &floors, MIN_CELL);
                // 이 열이 다음으로 만나는 자기 경계(스팬 끝) — 이동 한계의 기준
                let span_end = n.row + n.row_span; // exclusive line
                let region: i32 = (boundary..span_end)
                    .map(|r| eff_rows.get(r as usize).copied().unwrap_or(0) as i32)
                    .sum();
                // [2026-08-17 저녁 철회] 열 방향과 동일 — "끝선 도달 시 이웃 전체
                // 흡수" 철회(사용자 신고: a1 아래 어긋에 a2 가 합쳐져 사라짐). 이웃
                // 조각은 바닥(n_floor)까지만 밀리고 셀 수는 보존된다.
                let d = delta.min(region - n_floor);
                if d <= 0 {
                    return Err("이웃 칸에 남는 높이가 없습니다".to_string());
                }
                self.materialize_rows_effective(t.row as usize, (span_end - 1) as usize);
                // 낙하점이 속한 행 r*(행 내 오프셋 off) — 기존 선 ±MIN_CELL 이내면
                // 그 선에 **합류**(스냅: 슬리버 조각 방지), 아니면 r* 를 분할해 새 선.
                let rows_now = self.get_row_heights();
                let mut acc = 0i32;
                let mut r_star = boundary;
                let mut off = d;
                let mut row_h = 0i32;
                for r in boundary..span_end {
                    let h = rows_now.get(r as usize).copied().unwrap_or(0) as i32;
                    if acc + h >= d {
                        r_star = r;
                        off = d - acc;
                        row_h = h;
                        break;
                    }
                    acc += h;
                }
                let t = self.cells[cell_idx].clone();
                let (join_line, d) = if off <= MIN_CELL && acc > 0 {
                    (r_star, acc) // 윗선 합류
                } else if row_h - off <= MIN_CELL && acc + row_h <= region - n_floor {
                    (r_star + 1, acc + row_h) // 아랫선 합류
                } else {
                    if row_h < MIN_CELL * 2 {
                        return Err("낙하점 주변에 분할 여유가 없습니다".to_string());
                    }
                    let off = off.clamp(MIN_CELL, row_h - MIN_CELL);
                    self.insert_row_line(r_star, off, t.col, t.col_span)?;
                    (r_star + 1, acc + off)
                };
                // 이웃이 join_line 을 가로지르면(스팬) 그 자리에서 이분할 —
                // 위 조각 = 흡수분 d(빈 내용), 아래 = 잔여(내용 유지)
                let n_idx2 = self.cell_index_at(boundary, t.col).ok_or("이웃 소실")?;
                let n2 = self.cells[n_idx2].clone();
                if n2.row + n2.row_span > join_line {
                    let mut top = n2.clone();
                    top.row_span = join_line - n2.row;
                    top.height = d as HwpUnit;
                    top.paragraphs = vec![crate::model::paragraph::Paragraph::default()];
                    let mut bot = n2.clone();
                    bot.row = join_line;
                    bot.row_span = n2.row + n2.row_span - join_line;
                    bot.height = (n2.height as i32 - d) as HwpUnit;
                    self.cells[n_idx2] = top;
                    self.cells.insert(n_idx2 + 1, bot);
                    self.rebuild_grid();
                }
                self.merge_cells(t.row, t.col, join_line - 1, t.col + t.col_span - 1)?;
                let merged = self.cell_index_at(t.row, t.col).ok_or("병합 결과 소실")?;
                self.cells[merged].height = (t.height as i32 + d) as HwpUnit;
            } else {
                // 정렬된 경계의 신규 위쪽 어긋내기(이미 어긋난 경계는 재이동 경로가 처리)
                // 내용은 위 조각(대상 잔여)에 남는다 — 내용 있으면 글줄 바닥, 빈 조각은 MIN_CELL
                let t_floor = self.stagger_piece_floor_hu(cell_idx, &floors, MIN_CELL);
                let t_eff = (t.height)
                    .max(eff_rows.get(t.row as usize).copied().unwrap_or(t.height))
                    as i32;
                let d = (-delta).min(t_eff - t_floor);
                if d <= 0 {
                    return Err("대상 칸에 남는 높이가 없습니다".to_string());
                }
                self.materialize_rows_effective(t.row as usize, boundary as usize);
                let t = self.cells[cell_idx].clone();
                let n = self.cells[n_idx].clone();
                self.split_cell_into(t.row, t.col, 2, 1, true, false)?;
                let top = self
                    .cell_index_at(t.row, t.col)
                    .ok_or("분할 조각(상) 소실")?;
                self.cells[top].height = (t.height as i32 - d) as HwpUnit;
                let strip = self
                    .cell_index_at(t.row + 1, t.col)
                    .ok_or("분할 조각(하) 소실")?;
                self.cells[strip].height = d as HwpUnit;
                self.merge_cells(
                    t.row + 1,
                    t.col,
                    boundary + n.row_span,
                    t.col + t.col_span - 1,
                )?;
                let merged = self
                    .cell_index_at(t.row + 1, t.col)
                    .ok_or("병합 결과 소실")?;
                self.cells[merged].height = (n.height as i32 + d) as HwpUnit;
                // 빈 조각이 병합 기준(primary)이라 이웃 내용이 뒤로 밀린다 — 선두 빈 문단 제거
                if self.cells[merged].paragraphs.len() > 1
                    && self.cells[merged].paragraphs[0].text.is_empty()
                {
                    self.cells[merged].paragraphs.remove(0);
                }
            }
        }
        self.rebuild_grid();
        // [2026-08-12] 격자 재구성 후 common.width/height 동기화 — 종전엔 어긋내기/복원
        // 후 스테일이 남아 표 높이가 이전 값(부풀림 포함)에 고정됐다(신고 재현 probe).
        self.update_ctrl_dimensions();
        Ok(())
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
        let saved = self.clone();
        let w0: u64 = self.get_column_widths().iter().map(|&w| w as u64).sum();
        let h0: u64 = self.effective_row_heights().iter().map(|&h| h as u64).sum();
        let r = self.restore_cell_boundary_core(cell_idx, edge_right);
        if r.is_ok() && !self.stagger_invariants_hold(w0, h0) {
            *self = saved;
            return Err("이 조작은 표 격자를 깨뜨려 취소했습니다".to_string());
        }
        if r.is_err() {
            *self = saved;
        }
        r
    }

    fn restore_cell_boundary_core(
        &mut self,
        cell_idx: usize,
        edge_right: bool,
    ) -> Result<(), String> {
        let t = self
            .cells
            .get(cell_idx)
            .ok_or_else(|| "셀 인덱스가 유효하지 않습니다".to_string())?
            .clone();

        if edge_right {
            if t.col_span < 2 {
                return self.extend_cell_to_offset_line(cell_idx, true);
            }
            let boundary = t.col + t.col_span;
            if boundary >= self.col_count {
                return Err("바깥 테두리는 복원 대상이 아닙니다".to_string());
            }
            let n = self
                .cell_at(t.row, boundary)
                .ok_or_else(|| "오른쪽 이웃 셀을 찾지 못했습니다".to_string())?
                .clone();
            if n.row != t.row || n.row_span != t.row_span {
                return Err("위아래 높이가 다른 칸과는 복원할 수 없습니다".to_string());
            }
            // 병합 해제 → 왼쪽 (span-1)개 재병합 → 마지막 조각을 이웃에 붙인다
            self.split_cell(t.row, t.col)?;
            if t.col_span > 2 {
                self.merge_cells(t.row, t.col, t.row + t.row_span - 1, boundary - 2)?;
            }
            self.merge_cells(
                t.row,
                boundary - 1,
                t.row + t.row_span - 1,
                boundary + n.col_span - 1,
            )?;
            // 크기 목격자 복사: 같은 (col, col_span) 구간을 쓰는 다른 행의 셀
            let copy_w = |cells: &[Cell], col: u16, span: u16, skip_row: u16| -> Option<HwpUnit> {
                cells
                    .iter()
                    .find(|c| c.col == col && c.col_span == span && c.row != skip_row)
                    .map(|c| c.width)
            };
            if let Some(w) = copy_w(&self.cells, t.col, t.col_span - 1, t.row) {
                if let Some(i) = self.cell_index_at(t.row, t.col) {
                    self.cells[i].width = w;
                }
            }
            if let Some(w) = copy_w(&self.cells, boundary - 1, n.col_span + 1, t.row) {
                if let Some(i) = self.cell_index_at(t.row, boundary - 1) {
                    self.cells[i].width = w;
                }
            }
            // 병합 결과 선두 빈 문단 정리(빈 조각이 primary)
            if let Some(i) = self.cell_index_at(t.row, boundary - 1) {
                if self.cells[i].paragraphs.len() > 1 && self.cells[i].paragraphs[0].text.is_empty()
                {
                    self.cells[i].paragraphs.remove(0);
                }
            }
            self.collapse_unused_lines(true);
        } else {
            if t.row_span < 2 {
                return self.extend_cell_to_offset_line(cell_idx, false);
            }
            let boundary = t.row + t.row_span;
            if boundary >= self.row_count {
                return Err("바깥 테두리는 복원 대상이 아닙니다".to_string());
            }
            let n = self
                .cell_at(boundary, t.col)
                .ok_or_else(|| "아래 이웃 셀을 찾지 못했습니다".to_string())?
                .clone();
            if n.col != t.col || n.col_span != t.col_span {
                return Err("좌우 폭이 다른 칸과는 복원할 수 없습니다".to_string());
            }
            self.split_cell(t.row, t.col)?;
            if t.row_span > 2 {
                self.merge_cells(t.row, t.col, boundary - 2, t.col + t.col_span - 1)?;
            }
            self.merge_cells(
                boundary - 1,
                t.col,
                boundary + n.row_span - 1,
                t.col + t.col_span - 1,
            )?;
            let copy_h = |cells: &[Cell], row: u16, span: u16, skip_col: u16| -> Option<HwpUnit> {
                cells
                    .iter()
                    .find(|c| c.row == row && c.row_span == span && c.col != skip_col)
                    .map(|c| c.height)
            };
            if let Some(h) = copy_h(&self.cells, t.row, t.row_span - 1, t.col) {
                if let Some(i) = self.cell_index_at(t.row, t.col) {
                    self.cells[i].height = h;
                }
            }
            if let Some(h) = copy_h(&self.cells, boundary - 1, n.row_span + 1, t.col) {
                if let Some(i) = self.cell_index_at(boundary - 1, t.col) {
                    self.cells[i].height = h;
                }
            }
            if let Some(i) = self.cell_index_at(boundary - 1, t.col) {
                if self.cells[i].paragraphs.len() > 1 && self.cells[i].paragraphs[0].text.is_empty()
                {
                    self.cells[i].paragraphs.remove(0);
                }
            }
            self.collapse_unused_lines(false);
        }
        self.rebuild_grid();
        self.update_ctrl_dimensions();
        Ok(())
    }

    /// 치유 반대 방향: 정렬된 칸의 경계를 **어긋난 선 쪽으로** 끌어 맞춘다.
    /// 이웃이 어긋나(스팬≥2) 있을 때, 이웃의 첫 조각을 대상이 흡수해 전 열/행이
    /// 어긋난 선 위치로 정렬되고, 못 쓰게 된 원래 격자 줄은 접힌다.
    fn extend_cell_to_offset_line(
        &mut self,
        cell_idx: usize,
        edge_right: bool,
    ) -> Result<(), String> {
        let t = self
            .cells
            .get(cell_idx)
            .ok_or_else(|| "셀 인덱스가 유효하지 않습니다".to_string())?
            .clone();

        if edge_right {
            let boundary = t.col + t.col_span;
            if boundary >= self.col_count {
                return Err("바깥 테두리는 정렬 대상이 아닙니다".to_string());
            }
            let n = self
                .cell_at(t.row, boundary)
                .ok_or_else(|| "오른쪽 이웃 셀을 찾지 못했습니다".to_string())?
                .clone();
            if n.col_span < 2 {
                return Err("어긋난 경계가 아닙니다".to_string());
            }
            if n.row != t.row || n.row_span != t.row_span {
                return Err("위아래 높이가 다른 칸과는 정렬할 수 없습니다".to_string());
            }
            self.split_cell(n.row, n.col)?;
            // split 은 내용을 첫 조각에 남긴다 — 흡수될 조각은 비우고 남는 이웃 조각에 되돌린다
            if let (Some(a), Some(b)) = (
                self.cell_index_at(t.row, boundary),
                self.cell_index_at(t.row, boundary + 1),
            ) {
                if a != b {
                    let (lo, hi) = if a < b { (a, b) } else { (b, a) };
                    let (x, y) = self.cells.split_at_mut(hi);
                    std::mem::swap(&mut x[lo].paragraphs, &mut y[0].paragraphs);
                }
            }
            self.merge_cells(t.row, t.col, t.row + t.row_span - 1, boundary)?;
            if n.col_span > 2 {
                self.merge_cells(
                    t.row,
                    boundary + 1,
                    t.row + t.row_span - 1,
                    boundary + n.col_span - 1,
                )?;
            }
            let copy_w = |cells: &[Cell], col: u16, span: u16, skip_row: u16| -> Option<HwpUnit> {
                cells
                    .iter()
                    .find(|c| c.col == col && c.col_span == span && c.row != skip_row)
                    .map(|c| c.width)
            };
            if let Some(w) = copy_w(&self.cells, t.col, t.col_span + 1, t.row) {
                if let Some(i) = self.cell_index_at(t.row, t.col) {
                    self.cells[i].width = w;
                }
            }
            if let Some(w) = copy_w(&self.cells, boundary + 1, n.col_span - 1, t.row) {
                if let Some(i) = self.cell_index_at(t.row, boundary + 1) {
                    self.cells[i].width = w;
                }
            }
            self.collapse_unused_lines(true);
        } else {
            let boundary = t.row + t.row_span;
            if boundary >= self.row_count {
                return Err("바깥 테두리는 정렬 대상이 아닙니다".to_string());
            }
            let n = self
                .cell_at(boundary, t.col)
                .ok_or_else(|| "아래 이웃 셀을 찾지 못했습니다".to_string())?
                .clone();
            if n.row_span < 2 {
                return Err("어긋난 경계가 아닙니다".to_string());
            }
            if n.col != t.col || n.col_span != t.col_span {
                return Err("좌우 폭이 다른 칸과는 정렬할 수 없습니다".to_string());
            }
            self.split_cell(n.row, n.col)?;
            // split 은 내용을 첫 조각에 남긴다 — 흡수될 조각은 비우고 남는 이웃 조각에 되돌린다
            if let (Some(a), Some(b)) = (
                self.cell_index_at(boundary, t.col),
                self.cell_index_at(boundary + 1, t.col),
            ) {
                if a != b {
                    let (lo, hi) = if a < b { (a, b) } else { (b, a) };
                    let (x, y) = self.cells.split_at_mut(hi);
                    std::mem::swap(&mut x[lo].paragraphs, &mut y[0].paragraphs);
                }
            }
            self.merge_cells(t.row, t.col, boundary, t.col + t.col_span - 1)?;
            if n.row_span > 2 {
                self.merge_cells(
                    boundary + 1,
                    t.col,
                    boundary + n.row_span - 1,
                    t.col + t.col_span - 1,
                )?;
            }
            let copy_h = |cells: &[Cell], row: u16, span: u16, skip_col: u16| -> Option<HwpUnit> {
                cells
                    .iter()
                    .find(|c| c.row == row && c.row_span == span && c.col != skip_col)
                    .map(|c| c.height)
            };
            if let Some(h) = copy_h(&self.cells, t.row, t.row_span + 1, t.col) {
                if let Some(i) = self.cell_index_at(t.row, t.col) {
                    self.cells[i].height = h;
                }
            }
            if let Some(h) = copy_h(&self.cells, boundary + 1, n.row_span - 1, t.col) {
                if let Some(i) = self.cell_index_at(boundary + 1, t.col) {
                    self.cells[i].height = h;
                }
            }
            self.collapse_unused_lines(false);
        }
        self.rebuild_grid();
        self.update_ctrl_dimensions();
        Ok(())
    }

    /// 아무 셀 경계도 쓰지 않는 격자 줄(경계선)을 접는다 — 복원(치유) 뒷정리.
    /// edge_right=false 면 행, true 면 열. 접을 수 있는 줄이 없어질 때까지 반복.
    fn collapse_unused_lines(&mut self, cols: bool) {
        loop {
            let count = if cols { self.col_count } else { self.row_count };
            let mut target: Option<u16> = None;
            for line in 1..count {
                let used = self.cells.iter().any(|c| {
                    let (start, span) = if cols {
                        (c.col, c.col_span)
                    } else {
                        (c.row, c.row_span)
                    };
                    start == line || start + span == line
                });
                if !used {
                    target = Some(line);
                    break;
                }
            }
            let Some(line) = target else { break };
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
