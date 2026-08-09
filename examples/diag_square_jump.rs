//! [capability-map §3 스크래치] Square wrap 세로위치 튐 재현 — 동일 조건에서
//! textWrap 만 바꿔 표 y 를 대조한다. 사용: cargo run --example diag_square_jump

use rhwp::wasm_api::HwpDocument;

fn bbox_y(doc: &HwpDocument, pi: u32, ci: u32) -> (f64, f64) {
    let b = doc.get_table_bbox(0, pi, ci).expect("bbox");
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    (v["y"].as_f64().unwrap(), v["height"].as_f64().unwrap())
}

fn scenario(wrap: &str) -> (f64, f64) {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().expect("blank");
    doc.insert_text(0, 0, 0, &"가나다라마바사아자차카타파하 ".repeat(30))
        .expect("text");
    let created = doc.create_table(0, 0, 200, 3, 3).expect("table");
    let v: serde_json::Value = serde_json::from_str(&created).unwrap();
    let (pi, ci) = (
        v["paraIdx"].as_u64().unwrap() as u32,
        v["controlIdx"].as_u64().unwrap() as u32,
    );
    doc.set_table_properties(0, pi, ci, &format!(
        r#"{{"treatAsChar":false,"textWrap":"{wrap}","vertRelTo":"Page","vertAlign":"Center","vertOffset":0}}"#
    )).expect("props");
    bbox_y(&doc, pi, ci)
}

fn main() {
    for wrap in [
        "TopAndBottom",
        "Square",
        "Tight",
        "Through",
        "BehindText",
        "InFrontOfText",
    ] {
        let (y, h) = scenario(wrap);
        println!("{wrap:14} y={y:7.1} h={h:5.1}");
    }
}
// (vertRelTo 는 scenario 안에서 Page+Center — Paper 대조는 필요 시 문자열만 바꿔 재실행)
