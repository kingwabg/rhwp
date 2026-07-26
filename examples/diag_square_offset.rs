//! [어울림 이동 결함 A] 빈 host Square+Para 표의 vertOffset 렌더 반영 실측.
use rhwp::wasm_api::HwpDocument;
fn main() {
    let mm = 283.46_f64;
    for wrap in ["Square", "Tight", "TopAndBottom"] {
        let mut doc = HwpDocument::create_empty();
        doc.create_blank_document().expect("blank");
        doc.insert_text(0, 0, 0, &"가나다라마바사아자차카타파하 ".repeat(20)).expect("t");
        let c: serde_json::Value = serde_json::from_str(&doc.create_table(0, 0, 100, 3, 3).unwrap()).unwrap();
        let (pi, ci) = (c["paraIdx"].as_u64().unwrap() as u32, c["controlIdx"].as_u64().unwrap() as u32);
        doc.set_table_properties(0, pi, ci, &format!(
            r#"{{"treatAsChar":false,"textWrap":"{wrap}","vertRelTo":"Para","restrictInPage":true,"vertOffset":0}}"#
        )).unwrap();
        let y0: serde_json::Value = serde_json::from_str(&doc.get_table_bbox(0, pi, ci).unwrap()).unwrap();
        doc.move_table_offset(0, pi, ci, 0, (100.0 * mm).round() as i32).unwrap();
        let y1: serde_json::Value = serde_json::from_str(&doc.get_table_bbox(0, pi, ci).unwrap()).unwrap();
        println!("{wrap:12} y0={:7.1} +100mm→ y1={:7.1} (이동 {:6.1}px)", y0["y"].as_f64().unwrap(), y1["y"].as_f64().unwrap(), y1["y"].as_f64().unwrap() - y0["y"].as_f64().unwrap());
    }
}
