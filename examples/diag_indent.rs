//! [capability-map E6 스크래치] 양수 첫줄 들여쓰기 — 단위 규약(dialog px) 확인 + 렌더 반영.
//! 사용: cargo run --release --example diag_indent
use rhwp::wasm_api::HwpDocument;

fn probe(indent_px: f64) -> (f64, f64, usize, i64) {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().expect("blank");
    doc.insert_text(0, 0, 0, &"가나다라마바사아자차카타파하 ".repeat(12))
        .expect("text");
    doc.apply_para_format(0, 0, &format!(r#"{{"indent":{indent_px}}}"#))
        .expect("format");
    let rb: serde_json::Value =
        serde_json::from_str(&doc.get_para_properties_at(0, 0).expect("get")).unwrap();
    // 첫 줄 / 둘째 줄 시작 x
    let x_of = |c0: u32, c1: u32| -> f64 {
        doc.get_selection_rects(0, 0, c0, 0, c1)
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            .and_then(|v| v[0]["x"].as_f64())
            .unwrap_or(-1.0)
    };
    let line_count = doc
        .get_selection_rects(0, 0, 0, 0, 200)
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v.as_array().map(|a| a.len()))
        .unwrap_or(0);
    let svg = doc.render_page_svg(0).unwrap_or_default();
    let first_text_x = svg
        .split("<text")
        .nth(1)
        .and_then(|t| t.split("x=\"").nth(1))
        .and_then(|t| t.split('"').next())
        .and_then(|t| t.parse::<f64>().ok())
        .unwrap_or(-1.0);
    let caret = |off: u32| -> f64 {
        doc.get_cursor_rect(0, 0, off)
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            .and_then(|v| v["x"].as_f64())
            .unwrap_or(-1.0)
    };
    println!(
        "    svg첫text x={first_text_x:7.1}  캐럿@0={:7.1}  캐럿@1={:7.1}  캐럿@5={:7.1}",
        caret(0), caret(1), caret(5)
    );
    (
        x_of(0, 3),
        x_of(60, 63),
        line_count,
        rb["indent"].as_f64().unwrap_or(-1.0) as i64,
    )
}

fn main() {
    for ind in [0.0, 20.0, 40.0, 12000.0] {
        let (x1, x2, lines, readback) = probe(ind);
        println!("indent={ind:8.0}px → 되읽기={readback:6} 첫줄x={x1:7.1} 둘째줄x={x2:7.1} 줄상자={lines}");
    }
}
