//! [2026-08-13 신고 "어긋내기하면 다른 경계선마저 커져"] 어긋내기 자기-경계 불변식 핀.
//!
//! 결함: 행 어긋내기가 빈 셀 저장 규약(height=패딩만 284)의 **원시 모델 공간**에서
//! 한계·조각을 계산했다 — ① 신선한 표에서 화면 무동작인데 모델만 몰래 84HU 어긋나고
//! ② 조각(200 등)이 글줄 바닥과 얽혀 어긋내기 한 번에 아래 행 경계 전부가 밀렸다
//! (실측 +5.3px, 표도 커짐). 수리: 한계는 실효값으로 선판정(실패 시 무부작용), 통과 시
//! 연루 행을 실효 높이로 물질화 후 실효 공간에서 자른다. 조각 한계 = 콘텐츠 글줄 바닥.

use rhwp::wasm_api::HwpDocument;

fn table_state(doc: &HwpDocument) -> (u32, Vec<u32>, Vec<u32>) {
    for para in &doc.document().sections[0].paragraphs {
        for ctrl in &para.controls {
            if let rhwp::model::control::Control::Table(t) = ctrl {
                return (
                    t.common.height,
                    t.effective_row_heights(),
                    t.cells.iter().map(|c| c.height).collect(),
                );
            }
        }
    }
    panic!("표 없음");
}

fn make_table() -> (HwpDocument, u32, u32) {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    let c: serde_json::Value = serde_json::from_str(
        &doc.create_table_ex(
            r#"{"sectionIdx":0,"paraIdx":0,"charOffset":0,"rowCount":3,"colCount":3,"treatAsChar":false}"#,
        )
        .unwrap(),
    )
    .unwrap();
    let (pi, ci) = (
        c["paraIdx"].as_u64().unwrap() as u32,
        c["controlIdx"].as_u64().unwrap() as u32,
    );
    (doc, pi, ci)
}

/// 신선한 표(전 행 = 글줄 바닥): 어긋낼 여유가 없으니 **에러 + 무부작용** — 종전엔
/// 화면 무동작인데 모델만 몰래 84HU 어긋났다(368/200 실측).
#[test]
fn fresh_table_stagger_works_and_preserves_height() {
    // [2026-08-16 계약 변경] 종전엔 "여유 없는 신선 표는 거부"였다 — 빈 조각에도 글줄
    // 바닥(1284)을 요구해 새 3×3에서 행 어긋내기가 어느 방향으로도 불가능했고, 사용자
    // 실측 "경계선 하나하나 전부 어긋내기"가 전멸했다. 이제 빈 조각 최소는 열과 같은
    // MIN_CELL(200) — 신선 표에서도 어긋내기가 **동작**하되, 표 실효 높이는 불변이어야
    // 한다(트랜잭션 안전망이 위반을 롤백한다).
    let (mut doc, pi, ci) = make_table();
    let (_h0, eff0, _cells0) = table_state(&doc);
    let sum0: u32 = eff0.iter().sum();
    let r = doc.offset_cell_boundary_native(0, pi as usize, ci as usize, 0, false, 283);
    assert!(r.is_ok(), "신선 표 어긋내기가 거부됐다(구계약 회귀): {r:?}");
    let (_h1, eff1, _cells1) = table_state(&doc);
    let sum1: u32 = eff1.iter().sum();
    assert!(
        sum1.abs_diff(sum0) <= 4,
        "어긋내기가 표 실효 높이를 바꿨다 {sum0} → {sum1}"
    );
}

/// 여유 있는 표: 어긋내기는 **자기 경계만** 움직인다 — 표 높이 불변, 어긋낸 열 밖의
/// 행 경계(= 실효 행높이 합 분포) 불변.
#[test]
fn stagger_moves_only_its_own_boundary() {
    let (mut doc, pi, ci) = make_table();
    // row1 에 실제 여유: row1 셀들 +2000 (모델 명시 높이)
    doc.resize_table_cells(
        0,
        pi,
        ci,
        r#"[{"cellIdx":3,"heightDelta":2000},{"cellIdx":4,"heightDelta":2000},{"cellIdx":5,"heightDelta":2000}]"#,
    )
    .unwrap();
    let (h0, eff0, _c) = table_state(&doc);
    // col0 의 row0/1 경계를 아래로 800
    doc.offset_cell_boundary_native(0, pi as usize, ci as usize, 0, false, 800)
        .unwrap();
    let (h1, eff1, _c) = table_state(&doc);
    assert_eq!(
        h0, h1,
        "어긋내기가 표 높이를 바꿨다(신고 증상): {h0} → {h1}"
    );
    assert_eq!(
        h0,
        eff0.iter().sum::<u32>(),
        "resize 후 common.height 가 eff 합과 다르다(스테일 여유분 재발): eff0={eff0:?}"
    );
    assert_eq!(
        eff0.iter().sum::<u32>(),
        eff1.iter().sum::<u32>(),
        "실효 행높이 합이 변했다: {eff0:?} → {eff1:?}"
    );
    // 마지막 행(어긋내기 비연루)의 실효 높이는 그대로다
    assert_eq!(
        eff0.last(),
        eff1.last(),
        "어긋내기가 마지막 행까지 밀었다: {eff0:?} → {eff1:?}"
    );
}

/// 어긋내기 → 복원 왕복: 표 높이·행 수 원복.
#[test]
fn stagger_restore_roundtrip_preserves_height() {
    let (mut doc, pi, ci) = make_table();
    doc.resize_table_cells(
        0,
        pi,
        ci,
        r#"[{"cellIdx":3,"heightDelta":2000},{"cellIdx":4,"heightDelta":2000},{"cellIdx":5,"heightDelta":2000}]"#,
    )
    .unwrap();
    let (h0, eff0, _c) = table_state(&doc);
    doc.offset_cell_boundary_native(0, pi as usize, ci as usize, 0, false, 800)
        .unwrap();
    doc.restore_cell_boundary_native(0, pi as usize, ci as usize, 0, false)
        .unwrap();
    let (h1, eff1, _c) = table_state(&doc);
    assert_eq!(
        eff1.len(),
        eff0.len(),
        "복원 후 행 수가 원복되지 않았다: {eff1:?}"
    );
    assert_eq!(h0, h1, "복원 후 표 높이가 원복되지 않았다: {h0} → {h1}");
}

// ─────────────────────────────────────────────────────────────────────
// [2026-08-13 신고 3건] 어긋내기 상태 전이 — 재이동·복귀·과잉거부
// ─────────────────────────────────────────────────────────────────────

fn table_width(doc: &HwpDocument) -> (u32, Vec<u32>) {
    for para in &doc.document().sections[0].paragraphs {
        for ctrl in &para.controls {
            if let rhwp::model::control::Control::Table(t) = ctrl {
                return (t.common.width, t.get_column_widths());
            }
        }
    }
    panic!("표 없음");
}

/// 신고 ③: 같은 방향 반복 어긋내기가 표 폭을 키우면 안 된다(실측 559→600→620px).
#[test]
fn repeated_right_stagger_preserves_table_width() {
    let (mut doc, pi, ci) = make_table();
    let (w0, cols0) = table_width(&doc);
    let sum0: u32 = cols0.iter().sum();
    for k in 1..=4 {
        doc.offset_cell_boundary_native(0, pi as usize, ci as usize, 3, true, 283)
            .unwrap_or_else(|e| panic!("{k}회차 어긋내기 실패: {e:?}"));
        let (w, cols) = table_width(&doc);
        let sum: u32 = cols.iter().sum();
        assert_eq!(
            w, w0,
            "{k}회차에 표 폭이 변했다: {w0} → {w} (cols={cols:?})"
        );
        assert_eq!(sum, sum0, "{k}회차에 열 폭 합이 변했다: {cols:?}");
    }
}

/// 신고 ①③: 어긋낸 뒤 반대 방향 이동으로 되돌아올 수 있어야 한다(거부 금지).
#[test]
fn right_stagger_is_reversible_without_undo() {
    let (mut doc, pi, ci) = make_table();
    let (w0, cols0) = table_width(&doc);
    // 3스텝 어긋냄 (치유 캐치 반경 밖)
    for _ in 0..3 {
        doc.offset_cell_boundary_native(0, pi as usize, ci as usize, 3, true, 283)
            .expect("어긋내기");
    }
    let (w1, cols1) = table_width(&doc);
    assert_eq!(w1, w0, "어긋내기가 표 폭을 바꿨다: {cols1:?}");
    assert_ne!(cols1, cols0, "어긋내기가 격자에 반영되지 않았다");
    // 같은 스텝으로 되돌리기 — 각 스텝이 거부되면 안 된다
    for k in 1..=3 {
        doc.offset_cell_boundary_native(0, pi as usize, ci as usize, 3, true, -283)
            .unwrap_or_else(|e| panic!("{k}회차 복귀 거부(신고 ①): {e:?}"));
    }
    let (w2, cols2) = table_width(&doc);
    assert_eq!(w2, w0, "복귀 후 표 폭이 원복되지 않았다: {w0} → {w2}");
    assert_eq!(
        cols2, cols0,
        "복귀 후 격자가 원래대로 돌아오지 않았다: {cols2:?}"
    );
}

/// 신고 ②: 한 행을 어긋내도 **다른 행**의 같은 경계는 계속 어긋낼 수 있어야 한다.
#[test]
fn staggering_one_row_does_not_block_other_rows() {
    let (mut doc, pi, ci) = make_table();
    let (w0, _c0) = table_width(&doc);
    // 행 1(cellIdx 3)의 우변 어긋냄
    doc.offset_cell_boundary_native(0, pi as usize, ci as usize, 3, true, 566)
        .expect("첫 어긋내기");
    // 행 0(cellIdx 0)의 같은 논리 경계 — 종전엔 "이미 어긋난 칸 쪽으로는..." 거부
    doc.offset_cell_boundary_native(0, pi as usize, ci as usize, 0, true, 283)
        .expect("다른 행 어긋내기가 거부됐다(신고 ②)");
    let (w1, cols1) = table_width(&doc);
    assert_eq!(w1, w0, "두 행 어긋내기 후 표 폭이 변했다: {cols1:?}");
}

/// 신고 ②(행 방향): 아래 경계를 어긋내도 같은 행 다른 칸이 막히면 안 된다.
#[test]
fn row_stagger_does_not_block_sibling_cells() {
    let (mut doc, pi, ci) = make_table();
    // 여유 확보(행 1 높이 키우기)
    doc.resize_table_cells(
        0,
        pi,
        ci,
        r#"[{"cellIdx":3,"heightDelta":2000},{"cellIdx":4,"heightDelta":2000},{"cellIdx":5,"heightDelta":2000}]"#,
    )
    .unwrap();
    let (h0, _e0, _c0) = table_state(&doc);
    doc.offset_cell_boundary_native(0, pi as usize, ci as usize, 0, false, 566)
        .expect("첫 행 어긋내기");
    // 같은 행의 다른 칸(cellIdx 1) — 종전엔 아래 이웃 span 때문에 전면 거부
    doc.offset_cell_boundary_native(0, pi as usize, ci as usize, 1, false, 283)
        .expect("형제 칸 어긋내기가 거부됐다(신고 ②)");
    let (h1, eff1, _c1) = table_state(&doc);
    assert_eq!(h0, h1, "행 어긋내기가 표 높이를 바꿨다: {eff1:?}");
}
