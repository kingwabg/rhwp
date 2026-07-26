//! setTableProperties(textWrap) 왕복 실측 — 세터 직후 게터가 무엇을 말하나.
use rhwp::wasm_api::HwpDocument;
fn main() {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().expect("blank");
    doc.insert_text(0, 0, 0, &"가나다 ".repeat(30)).expect("t");
    let c: serde_json::Value = serde_json::from_str(&doc.create_table(0, 0, 50, 3, 3).unwrap()).unwrap();
    let (pi, ci) = (c["paraIdx"].as_u64().unwrap() as u32, c["controlIdx"].as_u64().unwrap() as u32);
    let rb = |d: &HwpDocument| -> String {
        serde_json::from_str::<serde_json::Value>(&d.get_table_properties(0, pi, ci).unwrap()).unwrap()["textWrap"].as_str().unwrap().to_string()
    };
    println!("기본값: {}", rb(&doc));
    for wrap in ["Square", "Tight", "Through", "BehindText"] {
        doc.set_table_properties(0, pi, ci, &format!(r#"{{"textWrap":"{wrap}"}}"#)).unwrap();
        println!("set {wrap:12} → 게터 {}", rb(&doc));
    }
}
