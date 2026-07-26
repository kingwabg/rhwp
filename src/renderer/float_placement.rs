//! Flow reservation helpers for non-inline floating objects.

use crate::model::shape::{CommonObjAttr, HorzAlign, HorzRelTo, TextWrap, VertAlign, VertRelTo};
use crate::model::HwpUnit;

use super::hwpunit_to_px;
use super::page_layout::LayoutRect;

/// Interpret an HWPUNIT value that may have been stored through a signed field.
pub(crate) fn signed_hwpunit(value: HwpUnit) -> i32 {
    value as i32
}

/// A non-TAC `TopAndBottom` object positioned from its host paragraph.
///
/// ⚠ **[officex] `VertRelTo::Para` 조건을 절대 떼지 말 것** (2026-07-26).
/// 어울림(배타 밴드) 대상을 "TopAndBottom 인 모든 표"로 넓히려던 계획이 있었으나,
/// 그러면 `VertRelTo::Page` + `VertAlign::Bottom` 인 **결재 서명틀**까지 밴드가 되어
/// 서로 반대 방향을 잠근 짝 테스트가 동시에 깨진다:
///   · `tests/issue_1611_footer_page_bottom_pagination.rs:19` — 발신명의 footer 가
///     flow 를 **소비해야** `page_count == 2`
///   · `tests/issue_1658_page_bottom_fixed_exclusion.rs:32` — 같은 틀이 flow 를
///     **소비하면 안 되어** `page_count == 1`
/// `issue_1658` 머리주석이 이 둘을 "배타 예약(과소)과 flow 소비(과대) 양쪽을 잠근다"고
/// 명시한다. 그 개체는 아래 `is_page_bottom_fixed_float` 가 따로 맡는 영역이다.
/// 즉 이 술어는 **세 조건이 다 필요하다** — 하나라도 빼면 두 계약이 충돌한다.
pub(crate) fn is_para_topbottom_float(common: &CommonObjAttr) -> bool {
    !common.treat_as_char
        && matches!(common.text_wrap, TextWrap::TopAndBottom)
        && matches!(common.vert_rel_to, VertRelTo::Para)
}

/// [officex 2026-07-27] Para 기준 **어울림 가족**(Square|Tight|Through) 부동 표.
/// 빈 host 어울림 표가 어느 배치 경로에도 못 들어 vertOffset 이 렌더에 반영되지 않던
/// 결함(드래그해도 화면 부동 — 모델 오프셋만 축적)의 수리 지점: 빈 host lane 경로의
/// 분류를 topbottom 전용에서 이 가족까지 넓힌다. Page/Paper 기준 빈 host 는
/// paper_page_square_empty_top(#2019/가족 확장)이 이미 처리하므로 Para 만 대상이다.
pub(crate) fn is_para_square_family_float(common: &CommonObjAttr) -> bool {
    !common.treat_as_char
        && matches!(
            common.text_wrap,
            TextWrap::Square | TextWrap::Tight | TextWrap::Through
        )
        && matches!(common.vert_rel_to, VertRelTo::Para)
}

/// [Task #1658 v3] 페이지 하단 고정(vert=쪽·valign=Bottom) 자리차지 개체 (결재/서명 틀).
/// 한글은 이를 본문 하단에 절대배치(겹침 허용)하고 본문 텍스트를 그 위까지만 흐르게
/// 한다(하단 배타 영역) — 문서순 flow 소비 대상이 아니다. #1653 RCA 패턴 B.
pub(crate) fn is_page_bottom_fixed_float(common: &CommonObjAttr) -> bool {
    !common.treat_as_char
        && matches!(common.text_wrap, TextWrap::TopAndBottom)
        && matches!(common.vert_rel_to, VertRelTo::Page)
        && matches!(common.vert_align, VertAlign::Bottom)
}

/// Horizontal reference data used by float placement and table layout.
#[derive(Debug, Clone, Copy)]
pub(crate) struct FloatPlacementContext {
    pub col_area: LayoutRect,
    pub body_area: Option<LayoutRect>,
    pub paper_width: Option<f64>,
    pub host_margin_left: f64,
    pub host_margin_right: f64,
}

impl FloatPlacementContext {
    pub(crate) fn new(col_area: LayoutRect) -> Self {
        Self {
            col_area,
            body_area: None,
            paper_width: None,
            host_margin_left: 0.0,
            host_margin_right: 0.0,
        }
    }

    pub(crate) fn with_body_area(mut self, body_area: LayoutRect) -> Self {
        self.body_area = Some(body_area);
        self
    }

    pub(crate) fn with_paper_width(mut self, paper_width: f64) -> Self {
        self.paper_width = Some(paper_width);
        self
    }

    pub(crate) fn with_host_margins(mut self, left: f64, right: f64) -> Self {
        self.host_margin_left = left;
        self.host_margin_right = right;
        self
    }
}

/// Compute the same depth-0 horizontal range used by table layout.
pub(crate) fn horizontal_range(
    common: &CommonObjAttr,
    width_px: f64,
    ctx: FloatPlacementContext,
    dpi: f64,
) -> (f64, f64) {
    let h_offset = hwpunit_to_px(signed_hwpunit(common.horizontal_offset), dpi);
    let col_area = ctx.col_area;
    let (ref_x, ref_w) = match common.horz_rel_to {
        HorzRelTo::Paper => {
            let fallback_paper_w = if width_px > col_area.width {
                col_area.x * 2.0 + width_px
            } else {
                col_area.x * 2.0 + col_area.width
            };
            let paper_w = ctx.paper_width.unwrap_or(fallback_paper_w);
            (0.0, paper_w)
        }
        HorzRelTo::Page => ctx
            .body_area
            .filter(|body| body.width > 0.0)
            .map(|body| (body.x, body.width))
            .unwrap_or((col_area.x, col_area.width)),
        HorzRelTo::Para => (
            col_area.x + ctx.host_margin_left,
            col_area.width - ctx.host_margin_left,
        ),
        HorzRelTo::Column => (col_area.x, col_area.width),
    };

    let x = match common.horz_align {
        HorzAlign::Left | HorzAlign::Inside => ref_x + h_offset,
        HorzAlign::Center => ref_x + (ref_w - width_px).max(0.0) / 2.0 + h_offset,
        HorzAlign::Right | HorzAlign::Outside => ref_x + (ref_w - width_px).max(0.0) - h_offset,
    };
    (x, x + width_px.max(0.0))
}

/// A placed float lane in page/column-relative coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct FloatLane {
    pub x_start: f64,
    pub x_end: f64,
    pub bottom: f64,
}

impl FloatLane {
    fn overlaps_x(&self, x_start: f64, x_end: f64) -> bool {
        ranges_overlap(self.x_start, self.x_end, x_start, x_end)
    }
}

/// Tracks bottom reservations for horizontally independent float lanes.
#[derive(Debug, Default, Clone)]
pub(crate) struct FloatLaneSet {
    lanes: Vec<FloatLane>,
}

impl FloatLaneSet {
    pub(crate) fn new() -> Self {
        Self { lanes: Vec::new() }
    }

    pub(crate) fn clear(&mut self) {
        self.lanes.clear();
    }

    pub(crate) fn lanes(&self) -> &[FloatLane] {
        &self.lanes
    }

    pub(crate) fn pushed_top(&self, x_start: f64, x_end: f64, raw_top: f64) -> f64 {
        self.lanes
            .iter()
            .filter(|lane| lane.overlaps_x(x_start, x_end))
            .fold(raw_top, |top, lane| top.max(lane.bottom))
    }

    pub(crate) fn place(
        &mut self,
        x_start: f64,
        x_end: f64,
        raw_top: f64,
        height: f64,
    ) -> FloatLane {
        let top = self.pushed_top(x_start, x_end, raw_top);
        let lane = FloatLane {
            x_start,
            x_end,
            bottom: top + height.max(0.0),
        };
        self.lanes.push(lane);
        lane
    }

    pub(crate) fn max_bottom(&self) -> f64 {
        self.lanes
            .iter()
            .map(|lane| lane.bottom)
            .fold(0.0, f64::max)
    }
}

pub(crate) fn ranges_overlap(a_start: f64, a_end: f64, b_start: f64, b_end: f64) -> bool {
    let a0 = a_start.min(a_end);
    let a1 = a_start.max(a_end);
    let b0 = b_start.min(b_end);
    let b1 = b_start.max(b_end);
    a0 < b1 && b0 < a1
}

// ── [officex] 자리차지(TopAndBottom) 배타 밴드 — layout·typeset 공용 ──────────────
//
// 왜 여기 있나: 같은 개념이 layout.rs 와 typeset.rs 에 **각각 따로** 구현돼 있었다
// (VisibleFloatExclusion 이 두 벌, 게이트도 미묘하게 다름). typeset 이 페이지 분할을
// 먼저 확정하고 layout 이 그리므로, 두 쪽 규칙이 어긋나면 컬럼 높이 예산이 갈려
// 페이지 바닥이 터진다. 그래서 규칙을 **한 함수**로 모은다 — 이후 어떤 변경이든
// 두 엔진에 동시에 적용되도록 하는 것이 목적이다.
//
// 밴드는 x 가 없는 **순수 y 구간**이다. 이는 명세의 TopAndBottom 정의
// ("좌, 우에는 텍스트를 배치하지 않음", 「한글 문서 파일 형식 5.0」 표 69)와 정확히 일치한다.

/// 자리차지 개체가 본문에서 밀어내는 구간.
///
/// [officex] x 범위를 갖는다 — 지금 모든 호출자는 컬럼 전폭(`FloatBand::full_width`)을 넘겨
/// 세로 밴드처럼 쓰지만, 빈-host 표는 x 를 아는 `FloatLaneSet` 경로를 타고 있어
/// 좌·중·우 표가 나란히 선다(`tests/issue_986.rs:114`). 그 경로를 밴드로 옮기려면
/// 폭이 반드시 있어야 하므로, 자료구조를 먼저 넓혀 두고 등록 범위는 나중에 손댄다.
#[derive(Debug, Clone, Copy)]
pub(crate) struct FloatBand {
    /// 밴드가 가리는 가로 범위(컬럼 로컬 px). 전폭이면 `f64::NEG_INFINITY..INFINITY`.
    pub x_start: f64,
    pub x_end: f64,
    pub top: f64,
    pub bottom: f64,
    /// 이 밴드를 만든 개체가 앵커된 문단. 같은 문단의 텍스트(섹션 제목)는 자기 표가 만든
    /// 밴드에 밀리면 안 된다 — 한컴은 제목을 앵커(표 위)에 두고 표를 그 아래에 둔다.
    /// typeset 처럼 소유자 개념이 없는 호출자는 None 을 쓴다(그러면 스킵이 일어나지 않는다).
    pub owner_para: Option<usize>,
}

impl FloatBand {
    /// 가로 전폭을 가리는 밴드 — 종전 동작(x 무시)과 정확히 같다.
    pub(crate) fn full_width(top: f64, bottom: f64, owner_para: Option<usize>) -> Self {
        Self { x_start: f64::NEG_INFINITY, x_end: f64::INFINITY, top, bottom, owner_para }
    }

    fn overlaps_x(&self, x_start: f64, x_end: f64) -> bool {
        // 전폭 밴드는 항상 겹친다(무한대 비교를 타지 않고 빠르게 끝낸다).
        if self.x_start.is_infinite() && self.x_end.is_infinite() {
            return true;
        }
        ranges_overlap(self.x_start, self.x_end, x_start, x_end)
    }
}

/// 시작 y 에서 밴드들을 피해 내려간 y 를 돌려준다. 피할 게 없으면 `start` 그대로.
///
/// - `probe_height`: 항목/줄의 잉크 높이. **0 이면 겹침 프로브를 끈다**(시작점이 밴드
///   안에 있는 경우만 본다). 호출자마다 프로브 조건이 달라 값으로 흡수한다.
/// - `owner`: 지금 배치 중인 문단. 같은 문단이 소유한 밴드는 건너뛴다.
/// - `x_range`: 배치 중인 항목의 가로 범위. `None` 이면 가로를 보지 않는다(= 종전 동작).
pub(crate) fn skip_float_bands(
    start: f64,
    bands: &[FloatBand],
    probe_height: f64,
    owner: Option<usize>,
    x_range: Option<(f64, f64)>,
) -> f64 {
    let mut jump_to = start;
    for band in bands {
        if let (Some(band_owner), Some(current)) = (band.owner_para, owner) {
            if band_owner == current {
                continue;
            }
        }
        // 가로가 안 겹치면 이 밴드는 이 항목을 밀지 않는다(좌·중·우 표가 나란히 서는 근거).
        if let Some((x0, x1)) = x_range {
            if !band.overlaps_x(x0, x1) {
                continue;
            }
        }
        let starts_in_band = jump_to + 0.5 >= band.top && jump_to < band.bottom;
        let overlaps_band =
            probe_height > 0.0 && jump_to < band.top && jump_to + probe_height > band.top + 0.5;
        if starts_in_band || overlaps_band {
            jump_to = jump_to.max(band.bottom);
        }
    }
    jump_to
}

/// [officex/어울림 본편] 문단의 줄들을 밴드를 피해 세로로 쌓는다 — layout·typeset 공용 계산부.
///
/// 지금의 소비는 **문단 단위**(첫 줄만 프로브)라, 밴드 위에서 시작한 문단의 중간 줄이
/// 표를 관통한다. 이 함수가 그 격차를 메우는 계산이다: 줄마다 skip_float_bands 를 적용해
/// 최종 y 목록과 끝 y 를 돌려준다. **두 엔진이 이 한 함수를 써야** 한다 —
/// layout 은 줄 y 배치에, typeset 은 같은 값으로 문단 높이 예산에. 한쪽만 쓰면
/// 분할 예산과 그림이 갈려 페이지 바닥이 터진다(오늘 S3·S4 에서 확인한 병).
///
/// - `line_advances`: 줄별 (잉크 높이, 줄 간격). 프로브는 **잉크 높이만** 쓴다(#1789 계약 —
///   spacing 포함 판정은 표 위에 남아야 할 줄을 아래로 밀어 한컴과 최대 345px 어긋났다).
/// - 반환: (각 줄의 top y, 마지막 줄 아래 y).
pub(crate) fn stack_lines_through_bands(
    start_y: f64,
    line_advances: &[(f64, f64)],
    bands: &[FloatBand],
    owner: Option<usize>,
    x_range: Option<(f64, f64)>,
) -> (Vec<f64>, f64) {
    let mut y = start_y;
    let mut tops = Vec::with_capacity(line_advances.len());
    for &(ink_height, spacing) in line_advances {
        y = skip_float_bands(y, bands, ink_height, owner, x_range);
        tops.push(y);
        y += ink_height + spacing;
    }
    (tops, y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::shape::{HorzAlign, HorzRelTo};

    fn base_common() -> CommonObjAttr {
        CommonObjAttr {
            text_wrap: TextWrap::TopAndBottom,
            vert_rel_to: VertRelTo::Para,
            horz_rel_to: HorzRelTo::Column,
            horz_align: HorzAlign::Left,
            ..Default::default()
        }
    }

    #[test]
    fn signed_hwpunit_preserves_negative_offsets() {
        assert_eq!(signed_hwpunit((-43892i32) as u32), -43892);
        assert_eq!(signed_hwpunit(51100), 51100);
    }

    #[test]
    fn para_topbottom_float_predicate_requires_non_tac_para_topbottom() {
        let mut common = base_common();
        assert!(is_para_topbottom_float(&common));

        common.treat_as_char = true;
        assert!(!is_para_topbottom_float(&common));

        common.treat_as_char = false;
        common.text_wrap = TextWrap::Square;
        assert!(!is_para_topbottom_float(&common));

        common.text_wrap = TextWrap::TopAndBottom;
        common.vert_rel_to = VertRelTo::Page;
        assert!(!is_para_topbottom_float(&common));
    }

    #[test]
    fn lane_set_does_not_push_non_overlapping_ranges() {
        let mut lanes = FloatLaneSet::new();
        let first = lanes.place(0.0, 100.0, 10.0, 40.0);
        let second = lanes.place(120.0, 200.0, 10.0, 20.0);

        assert_eq!(first.bottom, 50.0);
        assert_eq!(second.bottom, 30.0);
        assert_eq!(lanes.max_bottom(), 50.0);
    }

    #[test]
    fn lane_set_pushes_overlapping_ranges() {
        let mut lanes = FloatLaneSet::new();
        lanes.place(0.0, 100.0, 10.0, 40.0);
        let second = lanes.place(90.0, 160.0, 10.0, 20.0);

        assert_eq!(second.bottom, 70.0);
        assert_eq!(lanes.max_bottom(), 70.0);
    }

    #[test]
    fn horizontal_range_matches_column_right_offset_rule() {
        let mut common = base_common();
        common.horz_align = HorzAlign::Right;
        common.horizontal_offset = 10;

        let ctx = FloatPlacementContext::new(LayoutRect {
            x: 20.0,
            y: 0.0,
            width: 200.0,
            height: 100.0,
        });
        let (x0, x1) = horizontal_range(&common, 50.0, ctx, 7200.0);

        assert_eq!(x0, 160.0);
        assert_eq!(x1, 210.0);
    }

    #[test]
    fn horizontal_range_uses_body_area_for_page_relative_objects() {
        let mut common = base_common();
        common.horz_rel_to = HorzRelTo::Page;
        common.horz_align = HorzAlign::Center;

        let ctx = FloatPlacementContext::new(LayoutRect {
            x: 20.0,
            y: 0.0,
            width: 200.0,
            height: 100.0,
        })
        .with_body_area(LayoutRect {
            x: 40.0,
            y: 0.0,
            width: 300.0,
            height: 100.0,
        });
        let (x0, x1) = horizontal_range(&common, 100.0, ctx, 7200.0);

        assert_eq!(x0, 140.0);
        assert_eq!(x1, 240.0);
    }

    // ── skip_float_bands — 리팩터 전 두 엔진의 동작을 그대로 고정한다 ──────────────
    fn band(top: f64, bottom: f64, owner: Option<usize>) -> FloatBand {
        FloatBand::full_width(top, bottom, owner)
    }
    /// 가로 범위를 가진 밴드 — 좌·중·우 표가 나란히 서는 근거를 고정한다.
    fn xband(x0: f64, x1: f64, top: f64, bottom: f64) -> FloatBand {
        FloatBand { x_start: x0, x_end: x1, top, bottom, owner_para: None }
    }

    #[test]
    fn skip_bands_leaves_start_when_clear() {
        let bands = [band(100.0, 200.0, None)];
        assert_eq!(skip_float_bands(10.0, &bands, 0.0, None, None), 10.0);
        assert_eq!(skip_float_bands(250.0, &bands, 0.0, None, None), 250.0);
    }

    #[test]
    fn skip_bands_jumps_when_starting_inside() {
        let bands = [band(100.0, 200.0, None)];
        assert_eq!(skip_float_bands(150.0, &bands, 0.0, None, None), 200.0);
        // 경계: top 바로 위(0.5 여유) 는 안쪽으로 본다
        assert_eq!(skip_float_bands(99.6, &bands, 0.0, None, None), 200.0);
    }

    #[test]
    fn skip_bands_overlap_probe_only_when_height_given() {
        let bands = [band(100.0, 200.0, None)];
        // 시작은 밴드 위지만 잉크가 밴드를 관통한다
        assert_eq!(skip_float_bands(90.0, &bands, 30.0, None, None), 200.0);
        // probe_height 0 이면 겹침을 보지 않는다(typeset 의 비-HWPX 경로와 동일)
        assert_eq!(skip_float_bands(90.0, &bands, 0.0, None, None), 90.0);
    }

    #[test]
    fn skip_bands_ignores_self_owned_band() {
        let bands = [band(100.0, 200.0, Some(7))];
        // 자기 표가 만든 밴드에는 밀리지 않는다(Issue #1549 제목 유지)
        assert_eq!(skip_float_bands(150.0, &bands, 0.0, Some(7), None), 150.0);
        // 다른 문단은 그대로 밀린다
        assert_eq!(skip_float_bands(150.0, &bands, 0.0, Some(8), None), 200.0);
        // 소유자 개념이 없는 호출자(typeset)는 항상 밀린다
        assert_eq!(skip_float_bands(150.0, &bands, 0.0, None, None), 200.0);
    }

    #[test]
    fn skip_bands_chains_through_multiple() {
        let bands = [band(100.0, 200.0, None), band(200.0, 300.0, None)];
        assert_eq!(skip_float_bands(150.0, &bands, 0.0, None, None), 300.0);
    }

    #[test]
    fn skip_bands_respects_x_when_range_given() {
        // 왼쪽 절반만 가리는 밴드. 오른쪽에 놓인 항목은 밀리지 않아야 한다
        // (빈-host 좌·중·우 표가 나란히 서는 근거 — tests/issue_986.rs:114).
        let bands = [xband(0.0, 100.0, 100.0, 200.0)];
        assert_eq!(skip_float_bands(150.0, &bands, 0.0, None, Some((0.0, 50.0))), 200.0);
        assert_eq!(skip_float_bands(150.0, &bands, 0.0, None, Some((120.0, 200.0))), 150.0);
        // x_range 를 안 주면 가로를 보지 않는다 = 종전 동작
        assert_eq!(skip_float_bands(150.0, &bands, 0.0, None, None), 200.0);
    }

    #[test]
    fn full_width_band_always_overlaps() {
        // 전폭 밴드는 어떤 x 를 줘도 민다(무한대 비교를 타지 않는 빠른 경로).
        let bands = [band(100.0, 200.0, None)];
        assert_eq!(skip_float_bands(150.0, &bands, 0.0, None, Some((9_000.0, 9_100.0))), 200.0);
    }

    #[test]
    fn stack_lines_no_bands_is_cumulative() {
        let (tops, end) = stack_lines_through_bands(100.0, &[(17.0, 3.0), (17.0, 3.0)], &[], None, None);
        assert_eq!(tops, vec![100.0, 120.0]);
        assert_eq!(end, 140.0);
    }

    #[test]
    fn stack_lines_mid_paragraph_jumps_band() {
        // 문단이 밴드 위(y=100)에서 시작 — 3번째 줄이 밴드[140..300]에 닿으면 아래로 점프.
        // 지금 문단 단위 소비(첫 줄만 프로브)로는 불가능한, 관통을 막는 바로 그 동작이다.
        let bands = [band(140.0, 300.0, None)];
        let lines = [(17.0, 3.0), (17.0, 3.0), (17.0, 3.0), (17.0, 3.0)];
        let (tops, end) = stack_lines_through_bands(100.0, &lines, &bands, None, None);
        assert_eq!(tops[0], 100.0);
        assert_eq!(tops[1], 120.0); // 잉크 120..137 — 밴드 위에 안전(#1789: spacing 미포함)
        assert_eq!(tops[2], 300.0); // 잉크가 밴드에 닿는 첫 줄 — 밴드 아래로
        assert_eq!(tops[3], 320.0);
        assert_eq!(end, 340.0);
    }

    #[test]
    fn stack_lines_owner_paragraph_not_pushed() {
        // 자기 표가 만든 밴드는 자기 문단(제목)을 밀지 않는다 — Issue #1549 계약 그대로.
        let bands = [band(140.0, 300.0, Some(7))];
        let lines = [(17.0, 3.0), (17.0, 3.0), (17.0, 3.0)];
        let (tops, _) = stack_lines_through_bands(100.0, &lines, &bands, Some(7), None);
        assert_eq!(tops, vec![100.0, 120.0, 140.0]);
    }

    #[test]
    fn stack_lines_respects_x_lane() {
        // 왼쪽 절반 밴드 — 오른쪽 레인의 줄은 관통이 아니라 '옆'이므로 안 밀린다.
        let bands = [xband(0.0, 100.0, 140.0, 300.0)];
        let lines = [(17.0, 3.0), (17.0, 3.0), (17.0, 3.0)];
        let (right, _) = stack_lines_through_bands(100.0, &lines, &bands, None, Some((120.0, 200.0)));
        assert_eq!(right, vec![100.0, 120.0, 140.0]);
        let (left, _) = stack_lines_through_bands(100.0, &lines, &bands, None, Some((0.0, 50.0)));
        assert_eq!(left[2], 300.0);
    }
}
