//! [6단계 2026-09-02] set_cell_properties / resize_table_cells 관문 편입 확인 — 성공 경로 불변식.
//! [9-c] localResize 키는 파싱만 되고 무시된다(힌트 필드 폐기) — common 은 격자 유도값 그대로.
use rhwp::model::control::Control;
use rhwp::wasm_api::HwpDocument;

fn tbl(doc: &HwpDocument, pi: usize, ci: usize) -> &rhwp::model::table::Table {
    let Control::Table(t) = &doc.document().sections[0].paragraphs[pi].controls[ci] else {
        panic!()
    };
    t
}
fn main() {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    let c: serde_json::Value = serde_json::from_str(&doc.create_table_ex(
        r#"{"sectionIdx":0,"paraIdx":0,"charOffset":0,"rowCount":3,"colCount":3,"treatAsChar":false}"#).unwrap()).unwrap();
    let (pi, ci) = (
        c["paraIdx"].as_u64().unwrap() as usize,
        c["controlIdx"].as_u64().unwrap() as usize,
    );
    let show = |doc: &HwpDocument, tag: &str| {
        let t = tbl(doc, pi, ci);
        println!(
            "{tag}: common={}x{} cell0={}x{} inv={}",
            t.common.width,
            t.common.height,
            t.cells[0].width,
            t.cells[0].height,
            t.check_invariants().err().unwrap_or_else(|| "ok".into())
        );
    };
    show(&doc, "load");
    let r = doc.set_cell_properties(
        0,
        pi as u32,
        ci as u32,
        0,
        r#"{"width":15000,"height":2000,"paddingTop":300}"#,
    );
    println!(
        "set_cell_properties(w15000,h2000) → {}",
        r.map(|_| "OK".to_string())
            .unwrap_or_else(|e| format!("{e:?}"))
    );
    show(&doc, "after props");
    // (wasm 래퍼의 Err 경로는 네이티브에서 JsValue 변환이 abort 하므로 여기서 프로브하지 않는다 —
    //  거부 메시지는 diag_table_ops_matrix 의 _native 호출로 확인)
    let r = doc.set_cell_properties(0, pi as u32, ci as u32, 1, r#"{"borderFillId":1}"#);
    println!(
        "set_cell_properties(borderFillId) → {}",
        r.map(|_| "OK".to_string())
            .unwrap_or_else(|e| format!("{e:?}"))
    );
    show(&doc, "after border");
    let r = doc.resize_table_cells(
        0,
        pi as u32,
        ci as u32,
        r#"[{"cellIdx":0,"widthDelta":1000,"localResize":true}]"#,
    );
    println!(
        "resize(local +1000) → {}",
        r.map(|_| "OK".to_string())
            .unwrap_or_else(|e| format!("{e:?}"))
    );
    show(&doc, "after local resize");
    let r = doc.resize_table_cells(
        0,
        pi as u32,
        ci as u32,
        r#"[{"cellIdx":7,"widthDelta":1000}]"#,
    );
    println!(
        "resize(cell 7 of 9) → {}",
        r.map(|_| "OK".to_string())
            .unwrap_or_else(|e| format!("{e:?}"))
    );
    // 구조 변경 뒤 불변식 유지: 행 삽입
    let r = doc.insert_table_row_native(0, pi, ci, 0, true);
    println!(
        "insert_row → {}",
        r.map(|_| "OK".to_string())
            .unwrap_or_else(|e| format!("{e:?}"))
    );
    show(&doc, "after insert_row");
}
