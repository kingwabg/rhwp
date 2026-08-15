//! [2026-08-15 머지 전 검토] 어긋내기 재이동 경로의 두 결함 핀.
//!
//! 둘 다 **세로 병합 이웃이 있는 한컴 저장 표**에서만 터져 기존 핀
//! (officex_stagger_self_axis_invariance)이 못 잡았다 — 그 핀들은 생성 직후 표
//! (모든 행이 이미 글줄 바닥값)만 다뤄 재이동 경로를 통과하지 않는다.
//!
//! ① 재이동 클램프가 **원시 stored 공간**에서 여유를 재 음수가 되고, delta<0 분기의
//!    부호 반전이 그 음수를 양수로 뒤집어 — 드래그가 반대 방향으로 가고 상대 셀
//!    높이가 u32 언더플로(4.29e9 HWPUNIT ≈ 59만 인치)했다.
//! ② `is_boundary_aligned` 가 "그 격자선을 쓰는 셀이 있는가"만 봐서, 이웃 열이 세로
//!    병합이면 **한 번도 어긋낸 적 없는 정상 경계**를 어긋난 것으로 오판했다. 그 뒤
//!    복원 승격 → 즉시 Err 로 드래그가 통째로 거부됐다("조금 끌면 되고 많이 끌면 실패").

use rhwp::model::table::{Cell, Table};

/// 한컴 저장 규약을 흉내낸 2×2 표 — 빈 셀 stored height 는 **패딩만**(284).
/// 오른쪽 열은 세로 병합(row_span=2).
fn merged_neighbor_table() -> Table {
    let mut t = Table::default();
    t.row_count = 2;
    t.col_count = 2;
    t.cells = vec![
        Cell {
            row: 0,
            col: 0,
            row_span: 1,
            col_span: 1,
            width: 4000,
            height: 284,
            ..Default::default()
        },
        Cell {
            row: 1,
            col: 0,
            row_span: 1,
            col_span: 1,
            width: 4000,
            height: 284,
            ..Default::default()
        },
        Cell {
            row: 0,
            col: 1,
            row_span: 2,
            col_span: 1,
            width: 4000,
            height: 568,
            ..Default::default()
        },
    ];
    t.rebuild_grid();
    t
}

fn heights(t: &Table) -> Vec<i64> {
    t.cells.iter().map(|c| c.height as i64).collect()
}

/// 첫 행의 **실효** 높이 — 사용자가 화면에서 보는 공간.
fn eff_row0(t: &Table) -> i64 {
    t.effective_row_heights().first().copied().unwrap_or(0) as i64
}

/// 핀 ① — 어떤 방향/크기로 끌어도 셀 높이가 u32 언더플로로 폭주하지 않는다.
///
/// 종전엔 -500 을 요청하면 대상이 **+1000** 이동하고 이웃이 4294966580 이 됐다.
#[test]
fn shift_never_underflows_or_reverses() {
    for delta in [-2000i32, -500, -100, 100, 500, 2000] {
        let mut t = merged_neighbor_table();
        let before = heights(&t);
        let eff_before = eff_row0(&t);
        let r = t.offset_cell_boundary(0, false, delta);
        let after = heights(&t);
        for (i, &h) in after.iter().enumerate() {
            assert!(
                h < 1_000_000,
                "delta {delta}: 셀 {i} 높이가 폭주했다 {before:?} → {after:?} (결과 {r:?})"
            );
        }
        // 성공했다면 대상은 요청 방향으로 갔거나 그대로여야 한다(반대 방향 금지).
        //
        // 비교는 **실효 공간**에서 한다. 저장값(stored)은 한컴 규약상 실제보다 작게
        // 기록돼 있고(빈 셀=패딩만), 이 연산이 통과하면 연루 행을 실효 높이로
        // 물질화한 뒤 자르므로 저장값만 보면 "줄이랬는데 커졌다"로 보인다 —
        // 화면에서는 줄어든 게 맞다.
        if r.is_ok() {
            let moved = eff_row0(&t) - eff_before;
            assert!(
                moved == 0 || moved.signum() == delta.signum() as i64,
                "delta {delta}: 대상 행이 반대 방향으로 이동 {eff_before} → {} (저장값 {before:?} → {after:?})",
                eff_row0(&t)
            );
        }
    }
}

/// 핀 ② — 세로 병합 이웃이 있어도 정상 경계는 정상으로 판정돼 드래그가 먹는다.
///
/// 종전엔 delta 가 조금만 커지면(≈284HU=3.8px) 복원 승격 → Err 로 통째 거부됐다.
#[test]
fn merged_neighbor_boundary_is_not_treated_as_misaligned() {
    let mut t = merged_neighbor_table();
    let r = t.offset_cell_boundary(0, false, 500);
    assert!(
        r.is_ok(),
        "세로 병합 이웃이 있는 정상 경계가 거부됐다: {r:?}"
    );
    let after = heights(&t);
    assert!(
        after[0] > 284,
        "드래그가 대상 셀을 키우지 못했다: {after:?}"
    );
    assert!(after.iter().all(|&h| h < 1_000_000), "높이 폭주: {after:?}");
}

/// 핀 ③ — 재이동은 **격자를 바꾸지 않는다**(셀 수·span 불변, 크기만 주고받음).
#[test]
fn shift_keeps_grid_intact() {
    let mut t = merged_neighbor_table();
    let cells_before = t.cells.len();
    let spans_before: Vec<(u16, u16)> = t.cells.iter().map(|c| (c.row_span, c.col_span)).collect();
    if t.offset_cell_boundary(0, false, 300).is_ok() {
        assert_eq!(t.cells.len(), cells_before, "재이동이 셀을 쪼갰다");
        let spans_after: Vec<(u16, u16)> =
            t.cells.iter().map(|c| (c.row_span, c.col_span)).collect();
        assert_eq!(spans_after, spans_before, "재이동이 span 을 바꿨다");
    }
}
