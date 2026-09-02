//! [격자 뷰 2026-09-02] 표 셀에서 매 호출 유도되는 **읽기 전용 격자**.
//!
//! 진실은 항상 `cell.(row,col,row_span,col_span,width,height)` 다. 이 뷰는 캐시·저장·역기록이
//! 없고, 옛 getter(`get_column_widths`/`get_row_heights`/`effective_row_heights`)와 옛 술어
//! (`is_misaligned_line`·`is_stagger_piece_row` 등)를 한 벌의 선 소유 정보(`LineInfo`)로
//! 재현한다. 8-a 단계에서는 어떤 생산 경로도 부르지 않으며 `check_corpus_invariants` 의
//! `solver_disagreement`/`predicate_mismatch` lint 가 옛 구현과의 동일성을 검증한다.
//!
//! 규약: 좌표는 HU 정수. px 변환은 렌더러가 밴드별 hwpunit_to_px 후 현행 순서로 누적한다.
//! cell_spacing 은 선 위치에 포함하지 않는다(layout_table 규약; A1 결정 시 재검토).
use super::table::{Cell, Table};
use super::HwpUnit;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Axis {
    Cols,
    Rows,
}

/// 격자선 하나의 소유 정보. `owners` = 이 선을 start 또는 end 경계로 쓰는 셀의 **앵커 줄**
/// (x선이면 row, y선이면 col), 정렬·유일. `crossers` = span 으로 관통하는 셀 수.
/// 정렬선 = owners.any(≠band) (`is_boundary_aligned` 의 `c.row != t.row` 와 동치),
/// 어긋선 = 내부 ∧ owners.len()==1, 부분공유선 = 내부 ∧ owners≠∅ ∧ crossers>0,
/// 죽은 선(S6) = 내부 ∧ owners=∅.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LineInfo {
    pub owners: Vec<u16>,
    pub crossers: u16,
}

/// 낙하점 탐색 결과 — 단위 구간 [line, line+1) 과 구간 내 오프셋(HU), 구간 크기.
#[derive(Clone, Copy, Debug)]
pub struct Located {
    pub line: u16,
    pub off: i32,
    pub size: i32,
}

/// 0.5px@96dpi 의 HU 판(37.5 → `>= 38`). `row_col_x` 수용 문턱.
pub const X_TOL_HU: i32 = 38;

/// cells 에서 매 호출 유도되는 읽기 전용 격자. 캐시·저장 없음.
/// ponytail: 프로파일에 build 가 잡히면 Table 에 OnceCell<TableGrid> + rebuild_grid() 무효화 5줄.
pub struct TableGrid<'a> {
    table: &'a Table,
    pub col_count: usize,
    pub row_count: usize,
    // ── 열 축 ──
    /// 모델 규칙: span1 max → solve_axis(Cols, &[]) → 0 은 1800. == 종전 get_column_widths.
    pub col_widths: Vec<HwpUnit>,
    /// 누적 x선, len col_count+1, [0]=0.
    pub col_x: Vec<HwpUnit>,
    /// 행별 x선 오버라이드(None = 전역). border_rendering::build_row_col_x 의 HU 이식:
    ///  (a) inferred_local_resize_rows 행: span1 폭 타일링, 스팬 내부선 비례 보간,
    ///      잔여 = target − 합: ≤ −X_TOL 무효, ≥ +X_TOL 마지막 선 가산; 어느 행이든 전역과 다르면 채택;
    ///  (b) 그 외(전역 폭과 다른 span1 셀 존재 시): 열별 span1 폭 or 전역 폭 누적, |합 − target| ≥ X_TOL 면 None.
    ///  target = common.width>0 ? common.width : Σcol_widths. treat_as_char 표는 전부 None.
    pub row_col_x: Vec<Option<Vec<HwpUnit>>>,
    // ── 행 축 두 층 ──
    /// 저장 층: span1 max → solve_axis(Rows) → 0 은 400. == get_row_heights.
    pub row_heights_stored: Vec<HwpUnit>,
    pub row_y_stored: Vec<HwpUnit>,
    /// 실효 층: max(저장, pad+1000, 셀 콘텐츠 바닥); 조각 행과 "span1 셀이 모두 명시 높이인" 관통 행은
    /// 저장값(빈 셀 284 규약 행은 관통되어도 바닥 적용). == effective_row_heights(8-a 는 옛 구현 호출).
    pub row_heights_eff: Vec<HwpUnit>,
    pub row_y_eff: Vec<HwpUnit>,
    /// 셀별 콘텐츠 바닥(HU), idx = cell_idx. == cell_content_floors_hu.
    pub cell_floors: Vec<u32>,
    // ── 선 소유 ──
    pub col_lines: Vec<LineInfo>, // len col_count+1
    pub row_lines: Vec<LineInfo>, // len row_count+1
    /// 1800/400 폴백에 도달한 (axis, band) — S7 lint 용, 값 계산엔 무영향.
    pub fallback_bands: Vec<(Axis, u16)>,
}

impl Table {
    /// 격자 뷰 유도의 유일 진입점. 함수 상단 1회 만들어 내려준다(셀별 루프 안 호출 금지).
    pub fn grid(&self) -> TableGrid<'_> {
        TableGrid::build(self)
    }
}

/// 셀 하나의 선 소유 기록: start·end 선에 앵커 줄, 내부 선에 관통 +1.
fn own(lines: &mut [LineInfo], start: usize, span: usize, band: u16) {
    let n = lines.len();
    if start < n {
        lines[start].owners.push(band);
    }
    let end = start + span;
    if end < n {
        lines[end].owners.push(band);
    }
    for k in (start + 1)..end.min(n) {
        lines[k].crossers = lines[k].crossers.saturating_add(1);
    }
}

fn cumulative(sizes: &[HwpUnit]) -> Vec<HwpUnit> {
    let mut out = Vec::with_capacity(sizes.len() + 1);
    out.push(0);
    for &s in sizes {
        let last: HwpUnit = *out.last().unwrap();
        out.push(last.saturating_add(s));
    }
    out
}

fn as_i32(v: HwpUnit) -> i32 {
    v.min(i32::MAX as u32) as i32
}

impl<'a> TableGrid<'a> {
    fn build(t: &'a Table) -> Self {
        let cc = t.col_count as usize;
        let rc = t.row_count as usize;
        let mut col_lines = vec![LineInfo::default(); cc + 1];
        let mut row_lines = vec![LineInfo::default(); rc + 1];
        for c in &t.cells {
            own(&mut col_lines, c.col as usize, c.col_span as usize, c.row);
            own(&mut row_lines, c.row as usize, c.row_span as usize, c.col);
        }
        for l in col_lines.iter_mut().chain(row_lines.iter_mut()) {
            l.owners.sort_unstable();
            l.owners.dedup();
        }
        let mut g = TableGrid {
            table: t,
            col_count: cc,
            row_count: rc,
            col_widths: Vec::new(),
            col_x: Vec::new(),
            row_col_x: Vec::new(),
            row_heights_stored: Vec::new(),
            row_y_stored: Vec::new(),
            row_heights_eff: Vec::new(),
            row_y_eff: Vec::new(),
            cell_floors: Vec::new(),
            col_lines,
            row_lines,
            fallback_bands: Vec::new(),
        };
        g.col_widths = g.solve_axis(Axis::Cols, &[]);
        g.row_heights_stored = g.solve_axis(Axis::Rows, &[]);
        for (axis, sizes, default) in [
            (Axis::Cols, &mut g.col_widths, 1800u32),
            (Axis::Rows, &mut g.row_heights_stored, 400u32),
        ] {
            for (i, s) in sizes.iter_mut().enumerate() {
                if *s == 0 {
                    *s = default;
                    g.fallback_bands.push((axis, i as u16));
                }
            }
        }
        g.col_x = cumulative(&g.col_widths);
        g.row_y_stored = cumulative(&g.row_heights_stored);
        // 실효 층·콘텐츠 바닥은 문단 lineseg 에 의존 — 옛 구현이 곧 정의(단계 8-b 에서 위임 방향 확정).
        g.row_heights_eff = t.effective_row_heights();
        g.row_y_eff = cumulative(&g.row_heights_eff);
        g.cell_floors = t.cell_content_floors_hu();
        g.row_col_x = g.build_row_col_x();
        g
    }

    fn count(&self, axis: Axis) -> usize {
        match axis {
            Axis::Cols => self.col_count,
            Axis::Rows => self.row_count,
        }
    }

    fn interior(&self, axis: Axis, line: u16) -> bool {
        line > 0 && (line as usize) < self.count(axis)
    }

    /// 한 축 크기 해소 — 렌더러 산법(table_layout::resolve_column_widths 1-2단계) HU판:
    /// exclude_rows 밖 span1 max → (start,span) 별 max 로 dedup·span 오름차순 → 미지수 1개 제약
    /// 반복 해소 (total−known).max(0). **미결정은 0 으로 남긴다**(폴백·균등분할 없음).
    /// 모델 getter 는 &[] + 1800/400 채움, 렌더러는 &inferred 로 받아 px 후처리를 자기 코드에서 계속한다.
    pub fn solve_axis(&self, axis: Axis, exclude_rows: &[u16]) -> Vec<HwpUnit> {
        let count = self.count(axis);
        let mut sizes = vec![0u32; count];
        let mut constraints: Vec<(usize, usize, HwpUnit)> = Vec::new();
        for c in &self.table.cells {
            if exclude_rows.contains(&c.row) {
                continue;
            }
            let (start, span, total) = match axis {
                Axis::Cols => (c.col as usize, c.col_span as usize, c.width),
                Axis::Rows => (c.row as usize, c.row_span as usize, c.height),
            };
            if span == 1 && start < count {
                sizes[start] = sizes[start].max(total);
            } else if span > 1 && start + span <= count {
                match constraints.iter_mut().find(|x| x.0 == start && x.1 == span) {
                    Some(e) => e.2 = e.2.max(total),
                    None => constraints.push((start, span, total)),
                }
            }
        }
        constraints.sort_by_key(|&(_, span, _)| span);
        let max_iter = count + constraints.len();
        for _ in 0..max_iter {
            let mut progress = false;
            for &(start, span, total) in &constraints {
                let known: u64 = sizes[start..start + span].iter().map(|&s| s as u64).sum();
                let mut unknown = (start..start + span).filter(|&i| sizes[i] == 0);
                if let (Some(u), None) = (unknown.next(), unknown.next()) {
                    sizes[u] = (total as u64).saturating_sub(known).min(u32::MAX as u64) as u32;
                    progress = true;
                }
            }
            if !progress {
                break;
            }
        }
        sizes
    }

    /// build_row_col_x 경로 (a) 의 한 행: span1 셀 폭 타일링 + 스팬 내부선 비례 보간 + 잔여 규칙.
    fn tile_row(&self, r: usize, target: i64) -> Option<Vec<HwpUnit>> {
        let cc = self.col_count;
        let mut cells: Vec<&Cell> = self
            .table
            .cells
            .iter()
            .filter(|c| c.row as usize == r && c.row_span == 1)
            .collect();
        if cells.is_empty() {
            return None;
        }
        cells.sort_by_key(|c| c.col);
        let mut cand = vec![0i64; cc + 1];
        let mut cursor = 0i64;
        let mut next = 0usize;
        for c in cells {
            let s = c.col as usize;
            let span = c.col_span.max(1) as usize;
            let end = (s + span).min(cc);
            if s != next || end <= s {
                return None;
            }
            cand[s] = cursor;
            let w = c.width as i64;
            for inner in s + 1..end {
                cand[inner] = cursor + w * (inner - s) as i64 / span as i64;
            }
            cand[end] = cursor + w;
            cursor += w;
            next = end;
        }
        if next != cc {
            return None;
        }
        let residual = target - cursor;
        if residual <= -(X_TOL_HU as i64) {
            return None;
        }
        if residual >= X_TOL_HU as i64 {
            cand[cc] += residual;
        }
        Some(cand.into_iter().map(|v| v.clamp(0, u32::MAX as i64) as u32).collect())
    }

    fn build_row_col_x(&self) -> Vec<Option<Vec<HwpUnit>>> {
        let (cc, rc, t) = (self.col_count, self.row_count, self.table);
        let mut out: Vec<Option<Vec<HwpUnit>>> = vec![None; rc];
        if t.common.treat_as_char || cc == 0 {
            return out;
        }
        let target: i64 = if t.common.width > 0 {
            t.common.width as i64
        } else {
            self.col_x.last().copied().unwrap_or(0) as i64
        };
        // (a) 행 단위 resize 추론 행 — 타일링 결과가 어느 행이든 전역과 다르면 채택.
        let inferred = t.inferred_local_resize_rows();
        if !inferred.is_empty() {
            let mut any_diff = false;
            for &r in &inferred {
                let Some(cand) = self.tile_row(r as usize, target) else { continue };
                any_diff |= cand != self.col_x;
                if let Some(slot) = out.get_mut(r as usize) {
                    *slot = Some(cand);
                }
            }
            if any_diff {
                return out;
            }
            out.iter_mut().for_each(|o| *o = None);
        }
        // (b) 독립 폭 행 — span1 폭이 전역과 다른 셀이 있을 때만, 행 합이 target ±X_TOL 안이면 채택.
        let mut grid: Vec<Option<HwpUnit>> = vec![None; rc * cc];
        for c in &t.cells {
            if c.col_span == 1 && c.width > 0 && (c.col as usize) < cc && (c.row as usize) < rc {
                grid[c.row as usize * cc + c.col as usize] = Some(c.width);
            }
        }
        let independent = grid
            .iter()
            .enumerate()
            .any(|(i, w)| w.is_some_and(|w| w != self.col_widths[i % cc]));
        if !independent {
            return out;
        }
        for r in 0..rc {
            let mut x = vec![0u32; cc + 1];
            for c in 0..cc {
                x[c + 1] = x[c].saturating_add(grid[r * cc + c].unwrap_or(self.col_widths[c]));
            }
            if (x[cc] as i64 - target).abs() < X_TOL_HU as i64 {
                out[r] = Some(x);
            }
        }
        out
    }

    pub fn lines(&self, axis: Axis) -> &[LineInfo] {
        match axis {
            Axis::Cols => &self.col_lines,
            Axis::Rows => &self.row_lines,
        }
    }

    /// 선 위치(HU). Rows 는 effective 로 실효/저장 층 선택.
    pub fn line_pos(&self, axis: Axis, line: u16, effective: bool) -> i32 {
        let v = match (axis, effective) {
            (Axis::Cols, _) => &self.col_x,
            (Axis::Rows, true) => &self.row_y_eff,
            (Axis::Rows, false) => &self.row_y_stored,
        };
        v.get(line as usize).or(v.last()).map_or(0, |&p| as_i32(p))
    }

    /// 정렬선인가 — `band` 밖의 줄이 이 선을 경계로 쓴다(is_boundary_aligned).
    pub fn is_aligned_for(&self, axis: Axis, line: u16, band: u16) -> bool {
        self.lines(axis)
            .get(line as usize)
            .is_some_and(|l| l.owners.iter().any(|&o| o != band))
    }

    /// 어긋선 — 내부 선인데 한 줄만 경계로 쓴다(is_misaligned_line).
    pub fn is_misaligned(&self, axis: Axis, line: u16) -> bool {
        self.interior(axis, line) && self.lines(axis)[line as usize].owners.len() == 1
    }

    /// 부분 공유선 — 내부 선을 일부는 경계로 쓰고 일부는 관통한다(is_partially_shared_line).
    pub fn is_partially_shared(&self, axis: Axis, line: u16) -> bool {
        self.interior(axis, line) && {
            let l = &self.lines(axis)[line as usize];
            !l.owners.is_empty() && l.crossers > 0
        }
    }

    /// 셀 저장 높이가 명시값인가(≠ 빈 셀 패딩 규약 ±8HU, 음수 랩 아님).
    fn stored_explicit(&self, c: &Cell) -> bool {
        let p = c.effective_padding(&self.table.padding);
        let pad = p.top.max(0) as i64 + p.bottom.max(0) as i64;
        c.height < 0x8000_0000 && (c.height as i64 - pad).abs() > 8
    }

    /// 행의 span1 셀이 **모두** 명시 저장 높이인가(span1 셀이 없는 행은 참).
    fn row_span1_cells_explicit(&self, row: usize) -> bool {
        self.table
            .cells
            .iter()
            .filter(|c| c.row as usize == row && c.row_span <= 1)
            .all(|c| self.stored_explicit(c))
    }

    /// 어긋내기 조각 행 — 위/아래 선이 어긋선이고 행의 span1 셀이 모두 명시 높이(is_stagger_piece_row).
    /// 1열 표는 제외.
    pub fn is_piece_row(&self, row: usize) -> bool {
        if self.col_count < 2 {
            return false;
        }
        let (up, down) = (row as u16, (row + 1) as u16);
        (self.is_misaligned(Axis::Rows, up) || self.is_misaligned(Axis::Rows, down))
            && self.row_span1_cells_explicit(row)
    }

    /// 어긋내기 합류 산물 행 — 인접 선이 부분 공유선(is_stagger_joined_row). 1열 표는 제외.
    pub fn is_joined_row(&self, row: usize) -> bool {
        if self.col_count < 2 {
            return false;
        }
        let (up, down) = (row as u16, (row + 1) as u16);
        self.is_partially_shared(Axis::Rows, up) || self.is_partially_shared(Axis::Rows, down)
    }

    /// 렌더러 1-b/2단계 성장 면제: cell_is_empty && (piece_row || (stored_explicit && joined_row)).
    /// table_layout::resolve_row_heights 와 height_measurer 의 같은 식. 2-b/2-c 는 대상 아님.
    pub fn growth_exempt(&self, row: usize, cell: &Cell) -> bool {
        let empty = cell
            .paragraphs
            .iter()
            .all(|p| p.text.chars().all(|ch| ch.is_whitespace()));
        empty && (self.is_piece_row(row) || (self.stored_explicit(cell) && self.is_joined_row(row)))
    }

    /// 죽은 내부 선(S6) — 어떤 셀도 경계로 쓰지 않는다.
    pub fn dead_lines(&self, axis: Axis) -> Vec<u16> {
        (1..self.count(axis))
            .filter(|&k| self.lines(axis)[k].owners.is_empty())
            .map(|k| k as u16)
            .collect()
    }

    /// `after` 뒤 첫 정렬선(band 밖 셀이 쓰는 선, 바깥 선 포함).
    pub fn next_aligned_line(&self, axis: Axis, after: u16, band: u16) -> Option<u16> {
        (after as usize + 1..=self.count(axis))
            .map(|k| k as u16)
            .find(|&k| self.is_aligned_for(axis, k, band))
    }

    /// [from,to) 구간에서 from 선 기준 dist(HU) 가 든 단위 구간·오프셋. 열은 col_widths, 행은 row_heights_eff.
    /// dist 가 구간을 넘으면 마지막 단위 구간에 off > size 로 돌려준다.
    pub fn locate(&self, axis: Axis, from: u16, to: u16, dist: i32) -> Located {
        let sizes = match axis {
            Axis::Cols => &self.col_widths,
            Axis::Rows => &self.row_heights_eff,
        };
        let to = (to as usize).min(sizes.len());
        let mut k = from as usize;
        let mut acc = 0i32;
        while k + 1 < to {
            let size = as_i32(sizes[k]);
            if dist < acc.saturating_add(size) {
                break;
            }
            acc = acc.saturating_add(size);
            k += 1;
        }
        let size = sizes.get(k).copied().map_or(0, as_i32);
        Located { line: k as u16, off: dist - acc, size }
    }

    /// 셀 사각형 (x, y, w, h) HU — 전역 x선, y 는 effective_y 로 층 선택. 격자 밖 셀은 None.
    pub fn cell_rect_hu(&self, cell_idx: usize, effective_y: bool) -> Option<(i32, i32, i32, i32)> {
        let c = self.table.cells.get(cell_idx)?;
        let (c0, c1) = (c.col as usize, c.col as usize + c.col_span as usize);
        let (r0, r1) = (c.row as usize, c.row as usize + c.row_span as usize);
        if c1 > self.col_count || r1 > self.row_count {
            return None;
        }
        let x0 = self.line_pos(Axis::Cols, c0 as u16, false);
        let x1 = self.line_pos(Axis::Cols, c1 as u16, false);
        let y0 = self.line_pos(Axis::Rows, r0 as u16, effective_y);
        let y1 = self.line_pos(Axis::Rows, r1 as u16, effective_y);
        Some((x0, y0, x1 - x0, y1 - y0))
    }

    /// (row, col) 슬롯을 점유하는 셀 인덱스(cell_index_at 재노출).
    pub fn cell_at(&self, row: u16, col: u16) -> Option<usize> {
        self.table.cell_index_at(row, col)
    }

    /// rowspan 블록 폐포 [start,end) — 관통자 없는 y선이 경계.
    pub fn row_blocks(&self) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        let mut start = 0usize;
        for line in 1..self.row_count {
            if self.row_lines[line].crossers == 0 {
                out.push((start, line));
                start = line;
            }
        }
        if self.row_count > 0 {
            out.push((start, self.row_count));
        }
        out
    }
}
