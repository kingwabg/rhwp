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

impl LineInfo {
    /// 정렬선 — `band` 밖의 줄이 이 선을 경계로 쓴다(is_boundary_aligned 의 `c.row != t.row`).
    pub fn aligned_for(&self, band: u16) -> bool {
        self.owners.iter().any(|&o| o != band)
    }
    /// 어긋선(내부 선일 때) — 한 줄만 경계로 쓴다.
    pub fn misaligned(&self) -> bool {
        self.owners.len() == 1
    }
    /// 부분 공유선(내부 선일 때) — 일부는 경계로 쓰고 일부는 관통한다.
    pub fn partially_shared(&self) -> bool {
        !self.owners.is_empty() && self.crossers > 0
    }
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
    /// 행별 x선 오버라이드(None = 전역). 종전 border_rendering::build_row_col_x 경로 (a) 의 HU 이식:
    ///  inferred_local_resize_rows 행: span1 폭 타일링, 스팬 내부선 비례 보간,
    ///  잔여 = target − 합: ≤ −X_TOL 무효, ≥ +X_TOL 마지막 선 가산; 어느 행이든 전역과 다르면 채택.
    ///  target = common.width>0 ? common.width : Σcol_widths. treat_as_char 표는 전부 None.
    ///  경로 (b)(독립 폭 행: 열별 span1 폭 or **렌더러 전역 폭** 누적) 는 폴백이 렌더러 후처리(균등분할·
    ///  deficit·1800·잔여) px 값이라 격자가 대신 채울 수 없다 → table_layout::row_col_x_px 가 px 로 수행.
    pub row_col_x: Vec<Option<Vec<HwpUnit>>>,
    // ── 행 축 두 층 ──
    /// 저장 층: span1 max → solve_axis(Rows) → 0 은 400. == get_row_heights.
    pub row_heights_stored: Vec<HwpUnit>,
    pub row_y_stored: Vec<HwpUnit>,
    /// 행별 글줄 바닥(HU) = max(pad+1000, 행 내 span1 셀 콘텐츠 바닥). 조각 행과 "span1 셀이 모두
    /// 명시 높이인" 관통 행은 0(저장 높이가 곧 실효 — 빈 셀 284 규약 행은 관통되어도 바닥 적용).
    /// == row_line_floors_hu.
    pub row_floors: Vec<u32>,
    /// 실효 층: max(저장, row_floors). == effective_row_heights.
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

/// 선 **하나**의 소유 정보 — 격자를 만들지 않고 O(cells) 한 패스(할당은 owners 뿐).
/// Table 의 술어 위임이 렌더러 셀별 루프에서 불리므로 `lines_only` 의 선별 Vec 할당도 피한다
/// (52k 셀 표에서 lines_only 는 셀당 수백 Vec 할당 → 렌더 2배 느려짐 실측).
pub(crate) fn line_info(t: &Table, axis: Axis, line: u16) -> LineInfo {
    let line = line as usize;
    let mut info = LineInfo::default();
    for c in &t.cells {
        let (start, span, band) = match axis {
            Axis::Cols => (c.col as usize, c.col_span as usize, c.row),
            Axis::Rows => (c.row as usize, c.row_span as usize, c.col),
        };
        let end = start + span;
        if start == line || end == line {
            info.owners.push(band);
        } else if start < line && line < end {
            info.crossers = info.crossers.saturating_add(1);
        }
    }
    info.owners.sort_unstable();
    info.owners.dedup();
    info
}

/// 셀 저장 높이가 명시값인가(≠ 빈 셀 패딩 규약 ±8HU, 음수 랩 아님).
pub(crate) fn stored_explicit(t: &Table, c: &Cell) -> bool {
    let p = c.effective_padding(&t.padding);
    let pad = p.top.max(0) as i64 + p.bottom.max(0) as i64;
    c.height < 0x8000_0000 && (c.height as i64 - pad).abs() > 8
}

/// 행의 span1 셀이 **모두** 명시 저장 높이인가(span1 셀이 없는 행은 참). 조각 행·관통 행에서 글줄
/// 바닥을 면제할 자격 — 빈 셀 규약 284 는 한컴이 항상 한 줄로 그리므로 면제하면 표가 줄어든다.
pub(crate) fn span1_cells_explicit(t: &Table, row: usize) -> bool {
    t.cells
        .iter()
        .filter(|c| c.row as usize == row && c.row_span <= 1)
        .all(|c| stored_explicit(t, c))
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
    /// 1단계 — 선 소유(`col_lines`/`row_lines`)만, O(cells·span). 크기 벡터는 비어 있다.
    /// Table 의 술어 위임(`is_misaligned_line` 등)이 셀별 루프에서 불려도 솔버·실효 층을 돌리지 않게.
    pub(crate) fn lines_only(t: &'a Table) -> Self {
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
        TableGrid {
            table: t,
            col_count: cc,
            row_count: rc,
            col_widths: Vec::new(),
            col_x: Vec::new(),
            row_col_x: Vec::new(),
            row_heights_stored: Vec::new(),
            row_y_stored: Vec::new(),
            row_floors: Vec::new(),
            row_heights_eff: Vec::new(),
            row_y_eff: Vec::new(),
            cell_floors: Vec::new(),
            col_lines,
            row_lines,
            fallback_bands: Vec::new(),
        }
    }

    /// 2단계 — 선 소유 + 양축 크기(저장·실효·바닥). `row_col_x` 는 비어 있다(모델 getter 용).
    /// Table 로 되돌아 부르지 않는다(cell_content_floors_hu 는 문단 lineseg 만 본다) — 위임 재귀 금지.
    pub(crate) fn axes(t: &'a Table) -> Self {
        let mut g = Self::lines_only(t);
        g.col_widths = g.solve_axis(Axis::Cols, &[]);
        // 행 축은 옛 모델 솔버 의미를 유지한다 — 렌더러가 행 1단계 입력으로 get_row_heights 를 써 왔으므로
        // 여기서 산법을 바꾸면 화면이 바뀐다(8-b 실측: A2 위반 26표, hwpspec 표 −95px·387쪽 문서 385쪽).
        // 두 산법은 모순 제약(병합 셀 ≠ 걸친 합)에서만 다르고 어느 쪽이 한컴과 같은지 오라클이 없다.
        // 통일(렌더러식으로)은 그 26표 캡처 판정 뒤 별도 커밋으로.
        g.row_heights_stored = g.solve_axis_legacy(Axis::Rows);
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
        g.cell_floors = t.cell_content_floors_hu();
        g.row_floors = g.compute_row_floors();
        g.row_heights_eff = g
            .row_heights_stored
            .iter()
            .zip(&g.row_floors)
            .map(|(&h, &f)| h.max(f))
            .collect();
        g.row_y_eff = cumulative(&g.row_heights_eff);
        g
    }

    /// 3단계 — 전부(행별 x선 포함). `Table::grid()`.
    fn build(t: &'a Table) -> Self {
        let mut g = Self::axes(t);
        g.row_col_x = g.build_row_col_x();
        g
    }

    /// 행별 글줄 바닥 — 종전 `effective_row_heights`/`row_line_floors_hu` 루프 본문의 한 패스판.
    ///
    /// 한컴 저장 규약: 빈 셀의 `cell.height` 는 **패딩만**이다(officex_tac_mid_anchor.hwpx: 셀 284 =
    /// 142+142, common.height 2568 = 1284×2). 저장 합산만으로 common.height 를 만들면 표가 글줄만큼
    /// 납작해져 TAC 옆 텍스트가 표 상단에 떠 보였다(2026-08-11). 바닥 = max(pad+1000(10pt 하한),
    /// 행 내 셀 콘텐츠 바닥(lineseg 실측)) — 12pt 문서에서 측정기(1484)와 200HU 어긋나던 뿌리.
    /// 면제: 조각 행(is_piece_row) — 저장 높이가 곧 실효(조각 800/284 를 1284 로 부풀리면 common.height
    /// 5136≠2852); 관통 행(crossers>0)도 span1 셀이 **모두** 명시 높이일 때만 면제 — 사용자 세로 병합
    /// 아래 행의 빈 셀(284 규약)은 렌더러처럼 바닥을 받는다(3×3 2×2 병합: 1행 284→1284).
    /// ⚠ 다른 셀의 **저장** 높이는 포함하지 않는다 — 보상(±d) 조절이 있는 실파일에서 행 max 가 커져
    /// 표가 자란다(issue_493).
    fn compute_row_floors(&self) -> Vec<u32> {
        const DEFAULT_LINE_HU: u32 = 1000;
        let (t, rc) = (self.table, self.row_count);
        let mut pad_max: Vec<Option<u32>> = vec![None; rc];
        let mut content_max = vec![0u32; rc];
        let mut all_explicit = vec![true; rc];
        for (i, c) in t.cells.iter().enumerate() {
            let r = c.row as usize;
            if c.row_span > 1 || r >= rc {
                continue;
            }
            let p = c.effective_padding(&t.padding);
            let pad = (p.top.max(0) + p.bottom.max(0)) as u32;
            pad_max[r] = Some(pad_max[r].map_or(pad, |m| m.max(pad)));
            content_max[r] = content_max[r].max(self.cell_floors.get(i).copied().unwrap_or(0));
            all_explicit[r] &= self.stored_explicit(c);
        }
        let table_pad = (t.padding.top.max(0) + t.padding.bottom.max(0)) as u32;
        (0..rc)
            .map(|r| {
                let spanned_through = self.row_lines[r].crossers > 0;
                if (spanned_through && all_explicit[r]) || self.piece_row_with(r, all_explicit[r]) {
                    return 0;
                }
                (pad_max[r].unwrap_or(table_pad) + DEFAULT_LINE_HU).max(content_max[r])
            })
            .collect()
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

    /// 솔버 입력 — (span1 max 크기, (start,span,total) 제약: (start,span) 별 max·첫 등장 순서·span 오름차순).
    fn span_constraints(
        &self,
        axis: Axis,
        exclude_rows: &[u16],
    ) -> (Vec<HwpUnit>, Vec<(usize, usize, HwpUnit)>) {
        let count = self.count(axis);
        let mut sizes = vec![0u32; count];
        let mut constraints: Vec<(usize, usize, HwpUnit)> = Vec::new();
        let mut index: std::collections::HashMap<(usize, usize), usize> = std::collections::HashMap::new();
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
                // (start,span) 별 max — 첫 등장 순서 유지(안정 정렬이 그 순서를 지킨다). 선형 탐색은
                // 병합 셀이 많은 큰 표에서 O(merges²) 라 인덱스 맵으로.
                match index.entry((start, span)) {
                    std::collections::hash_map::Entry::Occupied(e) => {
                        let slot: &mut (usize, usize, HwpUnit) = &mut constraints[*e.get()];
                        slot.2 = slot.2.max(total);
                    }
                    std::collections::hash_map::Entry::Vacant(e) => {
                        e.insert(constraints.len());
                        constraints.push((start, span, total));
                    }
                }
            }
        }
        constraints.sort_by_key(|&(_, span, _)| span);
        (sizes, constraints)
    }

    /// 한 축 크기 해소 — 렌더러 산법(table_layout::resolve_column_widths 1-2단계) HU판:
    /// exclude_rows 밖 span1 max → (start,span) 별 max 로 dedup·span 오름차순 → 미지수 1개 제약
    /// 반복 해소 (total−known).max(0). **미결정은 0 으로 남긴다**(폴백·균등분할 없음).
    /// 모델 getter 는 &[] + 1800/400 채움, 렌더러는 &inferred 로 받아 px 후처리를 자기 코드에서 계속한다.
    pub fn solve_axis(&self, axis: Axis, exclude_rows: &[u16]) -> Vec<HwpUnit> {
        self.solve_axis_with_constraints(axis, exclude_rows).0
    }

    /// `solve_axis` + 사용한 스팬 제약 목록(span 오름차순) — 렌더러의 px 후처리(균등분할·deficit) 입력.
    pub(crate) fn solve_axis_with_constraints(
        &self,
        axis: Axis,
        exclude_rows: &[u16],
    ) -> (Vec<HwpUnit>, Vec<(usize, usize, HwpUnit)>) {
        let (mut sizes, constraints) = self.span_constraints(axis, exclude_rows);
        let count = sizes.len();
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
        (sizes, constraints)
    }

    /// 옛 모델 솔버(`Table::solve_span_gaps`, 2026-08-04)의 HU판 — 셀 순서대로, (start,span) dedup 없이,
    /// 미지수 1개이고 total > known 일 때만 대입, 진전 없을 때까지 반복. 미결정은 0.
    /// 렌더러식 `solve_axis` 와는 모순 제약 표에서만 다르다(첫 등장 셀 vs max, 음수 잔여 skip vs 0 대입).
    pub fn solve_axis_legacy(&self, axis: Axis) -> Vec<HwpUnit> {
        let count = self.count(axis);
        let mut sizes = vec![0u32; count];
        for c in &self.table.cells {
            let (start, span, total) = match axis {
                Axis::Cols => (c.col as usize, c.col_span as usize, c.width),
                Axis::Rows => (c.row as usize, c.row_span as usize, c.height),
            };
            if span == 1 && start < count {
                sizes[start] = sizes[start].max(total);
            }
        }
        loop {
            let mut progressed = false;
            for c in &self.table.cells {
                let (start, span, total) = match axis {
                    Axis::Cols => (c.col as usize, c.col_span as usize, c.width),
                    Axis::Rows => (c.row as usize, c.row_span as usize, c.height),
                };
                if span < 2 || start + span > count {
                    continue;
                }
                let mut unknown = (start..start + span).filter(|&i| sizes[i] == 0);
                if let (Some(u), None) = (unknown.next(), unknown.next()) {
                    let known: u32 = sizes[start..start + span].iter().sum();
                    if total > known {
                        sizes[u] = total - known;
                        progressed = true;
                    }
                }
            }
            if !progressed {
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
        let mut any_diff = false;
        for &r in &inferred {
            let Some(cand) = self.tile_row(r as usize, target) else { continue };
            any_diff |= cand != self.col_x;
            if let Some(slot) = out.get_mut(r as usize) {
                *slot = Some(cand);
            }
        }
        if !any_diff {
            out.iter_mut().for_each(|o| *o = None);
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
        self.lines(axis).get(line as usize).is_some_and(|l| l.aligned_for(band))
    }

    /// 어긋선 — 내부 선인데 한 줄만 경계로 쓴다(is_misaligned_line).
    pub fn is_misaligned(&self, axis: Axis, line: u16) -> bool {
        self.interior(axis, line) && self.lines(axis)[line as usize].misaligned()
    }

    /// 부분 공유선 — 내부 선을 일부는 경계로 쓰고 일부는 관통한다(is_partially_shared_line).
    pub fn is_partially_shared(&self, axis: Axis, line: u16) -> bool {
        self.interior(axis, line) && self.lines(axis)[line as usize].partially_shared()
    }

    fn stored_explicit(&self, c: &Cell) -> bool {
        stored_explicit(self.table, c)
    }

    fn row_span1_cells_explicit(&self, row: usize) -> bool {
        span1_cells_explicit(self.table, row)
    }

    /// 어긋내기 조각 행 — 위/아래 선이 어긋선이고 행의 span1 셀이 모두 명시 높이(is_stagger_piece_row).
    /// 1열 표는 제외.
    pub fn is_piece_row(&self, row: usize) -> bool {
        self.piece_row_with(row, self.row_span1_cells_explicit(row))
    }

    /// is_piece_row 의 명시 높이 자격을 밖에서 넘기는 판(compute_row_floors 의 한 패스 집계용).
    fn piece_row_with(&self, row: usize, span1_explicit: bool) -> bool {
        if self.col_count < 2 {
            return false;
        }
        let (up, down) = (row as u16, (row + 1) as u16);
        (self.is_misaligned(Axis::Rows, up) || self.is_misaligned(Axis::Rows, down)) && span1_explicit
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
