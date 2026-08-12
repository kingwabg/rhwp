//! [2026-08-12 신고 "또 경계선 이상해졌어"] 열 폭 드래그의 자기 축 불변식 핀.
//!
//! 결함: 표 생성부가 셀 lineseg 글줄을 1000HU(10pt 가정)로 하드코딩했는데 셀 문단은
//! 캐럿 글자모양(예: 12pt)을 상속한다. 열 경계 드래그(resizeTableCells widthDelta)가
//! 폭 바뀐 셀을 reflow_line_segs 로 재조판하는 순간 글줄이 규약값(글자×100 = 1200)으로
//! 재합성돼 — 열만 옮겼는데 행·표가 세로로 부풀었다(3852→4461HU, 실측 17.1→19.8px/행).
//! 수리: 생성부터 글줄 = 상속 글자 크기×100 (reflow 와 같은 식 → reflow 멱등).

use rhwp::wasm_api::HwpDocument;

fn table_state(doc: &HwpDocument) -> (u32, u32, Vec<u32>, Vec<(i32, i32, i32)>) {
    for para in &doc.document().sections[0].paragraphs {
        for ctrl in &para.controls {
            if let rhwp::model::control::Control::Table(t) = ctrl {
                let segs = t
                    .cells
                    .iter()
                    .map(|c| {
                        let s = c.paragraphs[0]
                            .line_segs
                            .first()
                            .cloned()
                            .unwrap_or_default();
                        (s.line_height, s.text_height, s.line_spacing)
                    })
                    .collect();
                return (
                    t.common.width,
                    t.common.height,
                    t.effective_row_heights(),
                    segs,
                );
            }
        }
    }
    panic!("표 없음");
}

fn make_default_table() -> (HwpDocument, u32, u32) {
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

/// 열 경계 일반 드래그(+d/−d 짝)는 폭 배분만 바꾼다 — 표 폭·표 높이·행높이·셀 lineseg 불변.
#[test]
fn column_drag_never_changes_heights() {
    let (mut doc, pi, ci) = make_default_table();
    let (w0, h0, rows0, segs0) = table_state(&doc);
    doc.resize_table_cells(
        0,
        pi,
        ci,
        r#"[{"cellIdx":0,"widthDelta":1500},{"cellIdx":1,"widthDelta":-1500},{"cellIdx":3,"widthDelta":1500},{"cellIdx":4,"widthDelta":-1500},{"cellIdx":6,"widthDelta":1500},{"cellIdx":7,"widthDelta":-1500}]"#,
    )
    .unwrap();
    let (w1, h1, rows1, segs1) = table_state(&doc);
    assert_eq!(w0, w1, "열 드래그가 표 폭을 바꿨다");
    assert_eq!(
        h0, h1,
        "열 드래그가 표 높이를 바꿨다(신고 증상): {h0} → {h1}"
    );
    assert_eq!(
        rows0, rows1,
        "열 드래그가 행높이를 바꿨다: {rows0:?} → {rows1:?}"
    );
    assert_eq!(
        segs0, segs1,
        "열 드래그 reflow 가 셀 lineseg 치수를 바꿨다(생성/재조판 불일치)"
    );
}

/// 생성 셀 lineseg 는 상속 글자 크기 규약(크기×100)이다 — reflow(make_line_seg)와 같은 식.
/// 빈 문서 기본 글자가 몇 pt 든, 생성 직후 값과 reflow 직후 값이 같아야 한다(멱등).
#[test]
fn creation_linesegs_match_reflow_exactly() {
    let (mut doc, pi, ci) = make_default_table();
    let (_w, _h, _rows, segs_created) = table_state(&doc);
    // 폭 변화 없이도 전 셀 reflow 를 강제: +0 은 무시되므로 +1/−1 왕복
    for delta in [1i32, -1] {
        let upd: Vec<String> = (0..9)
            .map(|i| format!(r#"{{"cellIdx":{i},"widthDelta":{delta}}}"#))
            .collect();
        doc.resize_table_cells(0, pi, ci, &format!("[{}]", upd.join(",")))
            .unwrap();
    }
    let (_w, _h, _rows, segs_reflowed) = table_state(&doc);
    assert_eq!(
        segs_created, segs_reflowed,
        "생성 lineseg 와 reflow lineseg 가 다르다 — 드래그 순간 표가 변하는 뿌리"
    );
}

/// 셀 콘텐츠 바닥(cell_content_floors_hu) = 글줄 + 상하 패딩 — 행 축소 한계의 단일 근거.
/// 빈 12pt 셀 = 1200 + 284 = 1484. 스튜디오 리사이즈 클램프(getCellContentFloors)가 이 값을
/// 최소로 써야 격자(기록)와 표 상자(측정 바닥)가 어긋나는 유령 공간이 안 생긴다.
#[test]
fn content_floors_match_line_plus_padding() {
    let (doc, _pi, _ci) = make_default_table();
    for para in &doc.document().sections[0].paragraphs {
        for ctrl in &para.controls {
            if let rhwp::model::control::Control::Table(t) = ctrl {
                let floors = t.cell_content_floors_hu();
                assert_eq!(floors.len(), 9);
                let seg = t.cells[0].paragraphs[0].line_segs.first().cloned().unwrap();
                let pad = (t.padding.top + t.padding.bottom) as u32;
                for f in &floors {
                    assert_eq!(
                        *f,
                        seg.line_height as u32 + pad,
                        "바닥 = 글줄({}) + 패딩({})",
                        seg.line_height,
                        pad
                    );
                }
                // 바닥은 절대 최소(1276)보다 커야 유령 공간 수리가 실효 — 12pt 기준 1484
                assert!(*floors.iter().max().unwrap() > 1276, "floors={floors:?}");
                return;
            }
        }
    }
    panic!("표 없음");
}
