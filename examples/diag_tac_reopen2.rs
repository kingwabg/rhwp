//! [app-workflow/TAC 재열기 스크래치] 실물 tall.hwp(일지 양식 + 행 40개, 재열기 1쪽 고착)의
//! 인라인 판정 입력 실측. 사용: cargo run --example diag_tac_reopen2 -- <tall.hwp>

use rhwp::model::control::Control;
use rhwp::renderer::height_measurer::is_tac_table_inline_in_para;
use rhwp::wasm_api::HwpDocument;

fn main() {
    let path = std::env::args().nth(1).expect("usage: diag_tac_reopen2 <tall.hwp>");
    let bytes = std::fs::read(&path).expect("read");
    let doc = HwpDocument::from_bytes(&bytes).expect("open");
    println!("pages={}", doc.page_count());

    let model = rhwp::parse_document(&bytes).expect("parse");
    for (pi, para) in model.sections[0].paragraphs.iter().enumerate() {
        for (ci, ctrl) in para.controls.iter().enumerate() {
            let Control::Table(t) = ctrl else { continue };
            let tbl_line_h =
                t.common.height as i64 + t.outer_margin_top as i64 + t.outer_margin_bottom as i64;
            let seg_w = para.line_segs.first().map(|s| s.segment_width).unwrap_or(0);
            let segs: Vec<(i32, i32, i32)> = para
                .line_segs
                .iter()
                .map(|s| (s.line_height, s.segment_width, s.vertical_pos))
                .collect();
            println!(
                "pi{pi} ci{ci} tac={} rows={} tbl_h_hu={} tbl_line_h_hu={} inline={} segs(lh,sw,vpos)={:?}",
                t.common.treat_as_char,
                t.row_count,
                t.common.height,
                tbl_line_h,
                is_tac_table_inline_in_para(t, seg_w, para),
                segs
            );
        }
    }
}
