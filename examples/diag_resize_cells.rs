//! [capability-map §3 resizeTableCells 스크래치] 파라미터 규약 확인 —
//! {cellIdx,width}(문서에 없는 키) vs {cellIdx,widthDelta}(실제 규약).
use rhwp::wasm_api::HwpDocument;

fn cell_width(doc: &HwpDocument, pi: u32, ci: u32, cell: u32) -> i64 {
    let p = doc.get_cell_properties(0, pi, ci, cell).expect("props");
    serde_json::from_str::<serde_json::Value>(&p).unwrap()["width"]
        .as_i64()
        .unwrap_or(-1)
}

fn main() {
    for (label, payload) in [
        ("{cellIdx,width:5000}   ", r#"[{"cellIdx":0,"width":5000}]"#),
        ("{cellIdx,widthDelta:-2000}", r#"[{"cellIdx":0,"widthDelta":-2000}]"#),
        ("{cellIdx,renderWidth:5000}", r#"[{"cellIdx":0,"renderWidth":5000}]"#),
    ] {
        let mut doc = HwpDocument::create_empty();
        doc.create_blank_document().expect("blank");
        let created = doc.create_table(0, 0, 0, 2, 3).expect("table");
        let v: serde_json::Value = serde_json::from_str(&created).unwrap();
        let (pi, ci) = (
            v["paraIdx"].as_u64().unwrap() as u32,
            v["controlIdx"].as_u64().unwrap() as u32,
        );
        let before = cell_width(&doc, pi, ci, 0);
        let resp = doc
            .resize_table_cells(0, pi, ci, payload)
            .unwrap_or_else(|e| format!("ERR {e:?}"));
        let after = cell_width(&doc, pi, ci, 0);
        println!(
            "{label} → 응답={:<40} 폭 {before} → {after} {}",
            resp.chars().take(40).collect::<String>(),
            if before == after { "(불변)" } else { "(변경됨)" }
        );
    }
}
