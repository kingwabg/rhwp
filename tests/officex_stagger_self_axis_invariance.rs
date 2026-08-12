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
fn fresh_table_stagger_is_rejected_without_side_effects() {
    let (mut doc, pi, ci) = make_table();
    let (h0, eff0, cells0) = table_state(&doc);
    let r = doc.offset_cell_boundary_native(0, pi as usize, ci as usize, 0, false, 283);
    assert!(r.is_err(), "여유 없는 신선 표 어긋내기는 거부돼야 한다");
    let (h1, eff1, cells1) = table_state(&doc);
    assert_eq!(h0, h1, "거부됐는데 표 높이가 변했다");
    assert_eq!(eff0, eff1, "거부됐는데 실효 행높이가 변했다");
    assert_eq!(
        cells0, cells1,
        "거부됐는데 셀 모델 높이가 변했다(몰래 어긋남 재발)"
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
