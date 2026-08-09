//! [officex] HTML 붙여넣기 셀 글자 서식 핀 — qa:rhwp 붙여넣기·HTML 임포트 결함의 계약.
//!
//! 종전 3겹 소실: ①본문 디스패처가 인라인 태그(b/span)를 삼켜 flush 에 서식이 도달
//! 불가 ②td 인라인 CSS 는 워크어라운드(cell_char_shape_id=0 고정)로 고의 드롭
//! ③run 변환이 끝(_end)을 버려 리셋 ref 없음. 수리 후: td 스타일과 <b> 태그가
//! 셀 문단 char_shapes 로 실리고, run 끝에서 기본 서식으로 되돌아온다.

use rhwp::model::control::Control;
use rhwp::wasm_api::HwpDocument;

#[test]
fn td_style_and_b_tag_reach_cell_char_shapes() {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().expect("blank");
    doc.paste_html(
        0,
        0,
        0,
        r#"<table><tr><td style="font-weight:700;font-size:24pt;color:#dc2626">스타일</td><td><b>태그굵게</b></td></tr></table>"#,
    )
    .expect("paste");

    let bytes = doc.export_hwp().expect("export");
    let model = rhwp::parse_document(&bytes).expect("reparse");
    let table = model.sections[0]
        .paragraphs
        .iter()
        .flat_map(|p| &p.controls)
        .find_map(|c| match c {
            Control::Table(t) => Some(t),
            _ => None,
        })
        .expect("table");

    let shape_of = |cell_idx: usize| {
        let cp = &table.cells[cell_idx].paragraphs[0];
        let id = cp.char_shapes.first().expect("ref").char_shape_id as usize;
        &model.doc_info.char_shapes[id]
    };
    let s0 = shape_of(0);
    assert!(s0.bold, "td style 굵기 소실");
    assert_eq!(s0.base_size, 2400, "td style 크기(24pt) 소실");
    let s1 = shape_of(1);
    assert!(s1.bold, "<b> 태그 굵기 소실");
}

#[test]
fn inline_run_resets_after_close_tag() {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().expect("blank");
    doc.paste_html(0, 0, 0, r#"<table><tr><td>가<b>나</b>다</td></tr></table>"#)
        .expect("paste");
    let bytes = doc.export_hwp().expect("export");
    let model = rhwp::parse_document(&bytes).expect("reparse");
    let table = model.sections[0]
        .paragraphs
        .iter()
        .flat_map(|p| &p.controls)
        .find_map(|c| match c {
            Control::Table(t) => Some(t),
            _ => None,
        })
        .expect("table");
    let cp = &table.cells[0].paragraphs[0];
    assert_eq!(cp.text, "가나다");
    let runs: Vec<(u32, bool)> = cp
        .char_shapes
        .iter()
        .map(|r| {
            (
                r.start_pos,
                model.doc_info.char_shapes[r.char_shape_id as usize].bold,
            )
        })
        .collect();
    // '나'(pos1)만 굵고 '다'(pos2)는 리셋 — 종전엔 리셋 ref 가 없어 끝까지 굵어졌다.
    assert!(
        runs.contains(&(1, true)) && runs.iter().any(|&(p, b)| p == 2 && !b),
        "run 경계가 틀림: {runs:?} (기대: pos1 굵게 시작, pos2 리셋)"
    );
}
