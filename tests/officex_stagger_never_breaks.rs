//! [2026-08-16 신고 "절대 표 깨지지 않게"] 3×3 어긋내기 **전수 불변식 핀**.
//!
//! 모든 (셀, 변, 델타) 단발 108조합 + 3연타·왕복 연쇄 + 전 경계 순차를 돌리고,
//! 매 조작 후 표 불변식(폭 합·실효 높이 합 보존, 셀 크기 건전, 행 최소)을 단언한다.
//! 성공/실패 여부와 무관하게 상태가 온전해야 한다 — 깨질 조작은 엔진 트랜잭션
//! 안전망(offset_cell_boundary 의 스냅샷+검증+롤백)이 거부한다.
//!
//! 이 핀이 있는 한 "어긋내기가 표를 깨뜨리는" 회귀는 어떤 경로로도 통과하지 못한다.

use rhwp::wasm_api::HwpDocument;

fn make_doc() -> (HwpDocument, u32, u32) {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    let out = doc
        .create_table_native_sized(0, 0, 0, 3, 3, None, None)
        .expect("표 생성");
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    (
        doc,
        v["paraIdx"].as_u64().unwrap_or(0) as u32,
        v["controlIdx"].as_u64().unwrap_or(0) as u32,
    )
}

fn table_snapshot(doc: &HwpDocument) -> (u32, u32, Vec<(u16, u16, u16, u16, u32, u32)>, Vec<u32>) {
    for para in &doc.document().sections[0].paragraphs {
        for ctrl in &para.controls {
            if let rhwp::model::control::Control::Table(t) = ctrl {
                return (
                    t.common.width,
                    t.common.height,
                    t.cells
                        .iter()
                        .map(|c| (c.row, c.col, c.row_span, c.col_span, c.width, c.height))
                        .collect(),
                    t.effective_row_heights(),
                );
            }
        }
    }
    panic!("표 없음");
}

fn check(tag: &str, doc: &HwpDocument, base_w: u32, base_h_eff: u64, violations: &mut Vec<String>) {
    let (w, h, cells, eff) = table_snapshot(doc);
    for (i, &(r, c, rs, cs, cw, ch)) in cells.iter().enumerate() {
        if cw > 1_000_000 || ch > 1_000_000 {
            violations.push(format!(
                "{tag}: 셀{i}({r},{c} span {rs}x{cs}) 크기 폭주 w={cw} h={ch}"
            ));
        }
    }
    if w > 1_000_000 || h > 10_000_000 {
        violations.push(format!("{tag}: 표 크기 폭주 w={w} h={h}"));
    }
    if w != base_w {
        violations.push(format!("{tag}: 표 폭 변동 {base_w} → {w}"));
    }
    let eff_sum: u64 = eff.iter().map(|&x| x as u64).sum();
    if eff_sum.abs_diff(base_h_eff) > 4 {
        violations.push(format!("{tag}: 실효 높이 합 변동 {base_h_eff} → {eff_sum}"));
    }
    // 행별 실효 높이가 셀 최솟값(200) 미만이면 격자 붕괴
    for (r, &e) in eff.iter().enumerate() {
        if e < 200 {
            violations.push(format!("{tag}: 행{r} 실효 높이 {e} < 200"));
        }
    }
}

#[test]
fn exhaustive_single_and_sequence() {
    let mut violations: Vec<String> = Vec::new();

    let (doc0, _pi, _ci) = make_doc();
    let (w0, _h0, cells0, eff0) = table_snapshot(&doc0);
    let base_h: u64 = eff0.iter().map(|&x| x as u64).sum();
    eprintln!("기준: w={w0} eff={eff0:?} cells={}", cells0.len());

    // ── 1) 단발: 모든 (셀, 변, 델타) ──
    let deltas = [-1500i32, -600, -200, 200, 600, 1500];
    for cell in 0..9usize {
        for edge_right in [false, true] {
            for &d in &deltas {
                let (mut doc, pi, ci) = make_doc();
                let tag = format!(
                    "단발 셀{cell} {} d={d}",
                    if edge_right { "우변" } else { "하변" }
                );
                let r = doc.offset_cell_boundary_native(
                    0,
                    pi as usize,
                    ci as usize,
                    cell,
                    edge_right,
                    d,
                );
                let ok = r.is_ok();
                check(
                    &format!("{tag} ({})", if ok { "Ok" } else { "Err" }),
                    &doc,
                    w0,
                    base_h,
                    &mut violations,
                );
            }
        }
    }

    // ── 2) 연쇄: 같은 경계 반복(같은 방향 ×3, 왕복, 어긋→복원) ──
    for cell in 0..9usize {
        for edge_right in [false, true] {
            // 같은 방향 3연타
            let (mut doc, pi, ci) = make_doc();
            for step in 0..3 {
                let _ = doc.offset_cell_boundary_native(
                    0,
                    pi as usize,
                    ci as usize,
                    cell,
                    edge_right,
                    400,
                );
                check(
                    &format!(
                        "3연타 셀{cell} {} 스텝{step}",
                        if edge_right { "우" } else { "하" }
                    ),
                    &doc,
                    w0,
                    base_h,
                    &mut violations,
                );
            }
            // 왕복 (+400 → -400): 원상이어야 함
            let (mut doc, pi, ci) = make_doc();
            let r1 =
                doc.offset_cell_boundary_native(0, pi as usize, ci as usize, cell, edge_right, 400);
            let r2 = doc.offset_cell_boundary_native(
                0,
                pi as usize,
                ci as usize,
                cell,
                edge_right,
                -400,
            );
            check(
                &format!("왕복 셀{cell} {}", if edge_right { "우" } else { "하" }),
                &doc,
                w0,
                base_h,
                &mut violations,
            );
            if r1.is_ok() && r2.is_ok() {
                let (_, _, cells_now, eff_now) = table_snapshot(&doc);
                if cells_now.len() != cells0.len() {
                    violations.push(format!(
                        "왕복 셀{cell} {}: 셀 수 {} → {} (원상 복귀 실패)",
                        if edge_right { "우" } else { "하" },
                        cells0.len(),
                        cells_now.len()
                    ));
                }
                if eff_now != eff0 {
                    violations.push(format!(
                        "왕복 셀{cell} {}: 실효 행높이 {eff0:?} → {eff_now:?}",
                        if edge_right { "우" } else { "하" }
                    ));
                }
            }
        }
    }

    // ── 3) 서로 다른 경계 순차 전부(모든 셀 하변 한 번씩, 이어서 우변 전부) ──
    let (mut doc, pi, ci) = make_doc();
    for cell in 0..9usize {
        let _ = doc.offset_cell_boundary_native(0, pi as usize, ci as usize, cell, false, 300);
        check(
            &format!("순차 하변 셀{cell}"),
            &doc,
            w0,
            base_h,
            &mut violations,
        );
    }
    for cell in 0..9usize {
        let _ = doc.offset_cell_boundary_native(0, pi as usize, ci as usize, cell, true, 300);
        check(
            &format!("순차 우변 셀{cell}"),
            &doc,
            w0,
            base_h,
            &mut violations,
        );
    }

    if !violations.is_empty() {
        eprintln!("=== 위반 {}건 ===", violations.len());
        for v in &violations {
            eprintln!("{v}");
        }
        panic!("불변식 위반 {}건", violations.len());
    }
}
