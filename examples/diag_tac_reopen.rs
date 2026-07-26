//! [app-workflow/TAC 재열기 스크래치] 실물 base.hwp(일지 양식)에 행 40개 → 저장 → 재열기.
//! 사용: cargo run --example diag_tac_reopen -- <base.hwp>

use rhwp::wasm_api::HwpDocument;

fn main() {
    let path = std::env::args().nth(1).expect("usage: diag_tac_reopen <base.hwp>");
    let bytes = std::fs::read(&path).expect("read");
    let mut edit = HwpDocument::from_bytes(&bytes).expect("open base");
    // 정원 밴드 표 = pi3 ci0 (make-tall.ts 실측)
    for _ in 0..40 {
        edit.insert_table_row(0, 3, 0, 1, true).expect("insertRow");
    }
    println!("행추가 후 pages={}", edit.page_count());
    let tall = edit.export_hwp().expect("export");
    let reopened = HwpDocument::from_bytes(&tall).expect("reopen");
    println!("재열기    pages={}", reopened.page_count());
    let model = rhwp::parse_document(&tall).expect("parse");
    let para = &model.sections[0].paragraphs[3];
    let segs: Vec<(i32, i32)> = para.line_segs.iter().map(|s| (s.line_height, s.segment_width)).collect();
    println!("pi3 segs(lh,sw)={segs:?}");
}
