//! native 정밀 줄 좌표 — wasm(브라우저)과 소수점 비교용.
use rhwp::wasm_api::HwpDocument;
fn main() {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    let filler =
        |n: usize| format!("채움 {n:02} 가나다라마바사 아자차카타파하 강물이 흐르고 산이 높다");
    doc.insert_text(0, 0, 0, &filler(14)).unwrap();
    for n in (1..14).rev() {
        doc.insert_paragraph(0, 0).unwrap();
        doc.insert_text(0, 0, 0, &filler(n)).unwrap();
    }
    let mut prev = None;
    for p in 0..7u32 {
        let cr: serde_json::Value =
            serde_json::from_str(&doc.get_cursor_rect(0, p, 0).unwrap()).unwrap();
        let y = cr["y"].as_f64().unwrap();
        let gap = prev.map(|q: f64| (y - q)).unwrap_or(0.0);
        println!("p{p} y={y:.3} gap={gap:.3}");
        prev = Some(y);
    }
}
