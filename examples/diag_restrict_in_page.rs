//! [capability-map E2 스크래치] restrictInPage(bit13) 재현 — true/false 로 표를 쪽 밖으로
//! 밀었을 때 클램프가 걸리는지 대조. 사용: cargo run --example diag_restrict_in_page

use rhwp::wasm_api::HwpDocument;

fn probe(restrict: bool, v_off_mm: f64) -> (f64, f64) {
    let mm = 283.46_f64;
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().expect("blank");
    doc.insert_text(0, 0, 0, "본문").expect("text");
    let created = doc.create_table(0, 0, 2, 3, 3).expect("table");
    let v: serde_json::Value = serde_json::from_str(&created).unwrap();
    let (pi, ci) = (
        v["paraIdx"].as_u64().unwrap() as u32,
        v["controlIdx"].as_u64().unwrap() as u32,
    );
    doc.set_table_properties(0, pi, ci, &format!(
        r#"{{"treatAsChar":false,"textWrap":"TopAndBottom","vertRelTo":"Para","restrictInPage":{restrict},"vertOffset":0}}"#
    )).expect("props");
    doc.move_table_offset(0, pi, ci, 0, (v_off_mm * mm).round() as i32)
        .expect("move");
    let b = doc.get_table_bbox(0, pi, ci).expect("bbox");
    let j: serde_json::Value = serde_json::from_str(&b).unwrap();
    if v_off_mm < 0.0 {
        let pg = doc.get_page_info(0).expect("pageinfo");
        println!("PAGE {pg}");
    }
    (j["y"].as_f64().unwrap(), j["height"].as_f64().unwrap())
}

fn main() {
    // A4 본문 아래끝 ≈1009px, 용지 높이 ≈1123px (96dpi)
    probe(true, -1.0);
    for off in [50.0, 150.0, 250.0, 400.0] {
        let (yt, h) = probe(true, off);
        let (yf, _) = probe(false, off);
        println!(
            "v_off={off:6.0}mm  restrict=true y={yt:7.1}(bottom {:7.1})  restrict=false y={yf:7.1}  diff={:7.1}",
            yt + h,
            yf - yt
        );
    }
}
