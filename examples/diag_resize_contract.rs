//! [9-c 계약 2026-09-02] 리사이즈 델타는 **표시(실효) 높이 기준** — 힌트(renderHeight) 없이 스튜디오가
//! "원하는 높이 − 화면 높이"를 보내면 엔진은 기준 = 행 실효 높이 위에 델타를 더해야 한다.
//! 엔진 규칙(유지, issue_493): 기준 = 저장 높이가 자기 패딩 이하(빈 셀 규약)이면 행 글줄 바닥, 아니면 저장 높이.
//! 3×3 빈 표: 저장 284 / 실효 1284. (a) 셀0 +216 → 저장 1500(표시 1284 기준). (b) 셀3 저장 300(패딩과 바닥
//! 사이, 드문 상태)으로 만든 뒤 +216 → 516: 실높이 셀은 승격하지 않으므로 표시(1284) 기준 델타와 어긋난다 —
//! 알려진 한계로 기록. (c) 폭 +1000 → 저장 +1000.
use rhwp::model::control::Control;
use rhwp::wasm_api::HwpDocument;

fn main() {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    let c: serde_json::Value = serde_json::from_str(&doc.create_table_ex(
        r#"{"sectionIdx":0,"paraIdx":0,"charOffset":0,"rowCount":3,"colCount":3,"treatAsChar":false}"#).unwrap()).unwrap();
    let (pi, ci) = (c["paraIdx"].as_u64().unwrap() as usize, c["controlIdx"].as_u64().unwrap() as usize);
    let show = |doc: &HwpDocument, tag: &str| {
        let Control::Table(t) = &doc.document().sections[0].paragraphs[pi].controls[ci] else { panic!() };
        println!("{tag}: heights={:?} eff={:?} widths={:?} common={}x{} inv={}",
            t.cells.iter().map(|c| c.height).collect::<Vec<_>>(), t.effective_row_heights(),
            t.get_column_widths(), t.common.width, t.common.height,
            t.check_invariants().err().unwrap_or_else(|| "ok".into()));
    };
    show(&doc, "load");
    doc.resize_table_cells(0, pi as u32, ci as u32, r#"[{"cellIdx":0,"heightDelta":216}]"#).unwrap();
    show(&doc, "(a) cell0 +216 (display 1284 → want 1500)");
    doc.set_cell_properties(0, pi as u32, ci as u32, 3, r#"{"height":300}"#).unwrap();
    show(&doc, "(b0) cell3 stored 300 (displayed 1284)");
    doc.resize_table_cells(0, pi as u32, ci as u32, r#"[{"cellIdx":3,"heightDelta":216}]"#).unwrap();
    show(&doc, "(b) cell3 +216 (stored 300 → 516: 실높이 셀 비승격, 알려진 한계)");
    doc.resize_table_cells(0, pi as u32, ci as u32, r#"[{"cellIdx":6,"widthDelta":1000}]"#).unwrap();
    show(&doc, "(c) cell6 width +1000");
}
