//! [어울림 본편] 본문이 표 **옆으로** 흐르는가 — 좁은 Square 표 + 긴 본문.
//! 한컴: 표가 오른쪽 절반이면 왼쪽에 텍스트가 흐르고 줄 폭이 좁아진다.
use rhwp::wasm_api::HwpDocument;

fn probe(wrap: &str, width_hu: u32, halign: &str) -> String {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().expect("blank");
    doc.insert_text(0, 0, 0, &"가나다라마바사아자차카타파하 ".repeat(30)).expect("t");
    let opts = format!(r#"{{"sectionIdx":0,"paraIdx":0,"charOffset":200,"rowCount":3,"colCount":2,"colWidths":[{w},{w}]}}"#, w = width_hu / 2);
    let c: serde_json::Value = serde_json::from_str(&doc.create_table_ex(&opts).unwrap()).unwrap();
    let (pi, ci) = (c["paraIdx"].as_u64().unwrap() as u32, c["controlIdx"].as_u64().unwrap() as u32);
    doc.set_table_properties(0, pi, ci, &format!(
        r#"{{"treatAsChar":false,"textWrap":"{wrap}","vertRelTo":"Para","horzRelTo":"Column","horzAlign":"{halign}","vertOffset":0}}"#
    )).unwrap();
    // 표 폭 축소: setTableColumnWidths 만 정상 경로(실측)
    let half = width_hu / 2;
    let _ = doc.set_table_column_widths(0, pi, ci, &format!("[{half},{half}]"));
    let b: serde_json::Value = serde_json::from_str(&doc.get_table_bbox(0, pi, ci).unwrap()).unwrap();
    // 본문 줄들의 x/width 를 selection rect 로 훑는다 (각 줄 시작 근처)
    let mut lines = Vec::new();
    for i in 0..8 {
        let s = i * 20;
        if let Ok(r) = doc.get_selection_rects(0, 0, s, 0, s + 12) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&r) {
                if let Some(a) = v.as_array().and_then(|a| a.first()) {
                    lines.push(format!("({:.0},{:.0}w{:.0})", a["y"].as_f64().unwrap_or(0.0), a["x"].as_f64().unwrap_or(0.0), a["width"].as_f64().unwrap_or(0.0)));
                }
            }
        }
    }
    format!("{wrap:12} 표[x{:.0} y{:.0} w{:.0} h{:.0}] 줄들: {}", b["x"].as_f64().unwrap(), b["y"].as_f64().unwrap(), b["width"].as_f64().unwrap(), b["height"].as_f64().unwrap(), lines.join(" "))
}

fn main() {
    // 표 폭 = 본문 폭(42520HU)의 절반 정도, 오른쪽 정렬
    for wrap in ["Square", "TopAndBottom"] {
        println!("{}", probe(wrap, 21000, "Right"));
    }
}
