//! 표 왼쪽/위 테두리가 얇아 보이는 원인 실측 — body clip_rect가 표 bbox 가장자리와
//! 같은 좌표면 선 두께의 절반(중심선 기준 바깥쪽)이 잘린다는 가설을 검증한다.
//! 사용: cargo run --release --example diag_table_edge_clip
use rhwp::wasm_api::HwpDocument;

fn main() {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    let c: serde_json::Value =
        serde_json::from_str(&doc.create_table(0, 0, 0, 2, 5).unwrap()).unwrap();
    let (pi, ci) = (
        c["paraIdx"].as_u64().unwrap() as u32,
        c["controlIdx"].as_u64().unwrap() as u32,
    );
    let b: serde_json::Value =
        serde_json::from_str(&doc.get_table_bbox(0, pi, ci).unwrap()).unwrap();
    let (tx, ty, tw, th) = (
        b["x"].as_f64().unwrap(),
        b["y"].as_f64().unwrap(),
        b["width"].as_f64().unwrap(),
        b["height"].as_f64().unwrap(),
    );

    let svg = doc.render_page_svg(0).unwrap();
    // body-clip rect 추출
    let clip = svg
        .split("<clipPath id=\"body-clip")
        .nth(1)
        .and_then(|s| s.split("<rect ").nth(1))
        .and_then(|s| s.split("/>").next())
        .unwrap_or("(없음)")
        .to_string();

    let num = |key: &str| -> f64 {
        clip.split(&format!("{key}=\""))
            .nth(1)
            .and_then(|s| s.split('"').next())
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(f64::NAN)
    };
    let (cx, cy, cw, ch) = (num("x"), num("y"), num("width"), num("height"));

    println!(
        "표 bbox   x={tx:.3} y={ty:.3} r={:.3} b={:.3}",
        tx + tw,
        ty + th
    );
    println!(
        "body clip x={cx:.3} y={cy:.3} r={:.3} b={:.3}",
        cx + cw,
        cy + ch
    );
    println!(
        "여유(px)  left={:.3} top={:.3} right={:.3} bottom={:.3}",
        tx - cx,
        ty - cy,
        (cx + cw) - (tx + tw),
        (cy + ch) - (ty + th),
    );
    // 표 왼쪽 세로선의 stroke-width 실측 → 절반이 여유보다 크면 잘린다
    let mut widest: f64 = 0.0;
    for seg in svg.split("<line ").skip(1) {
        let attr = |k: &str| -> Option<f64> {
            seg.split(&format!("{k}=\""))
                .nth(1)
                .and_then(|s| s.split('"').next())
                .and_then(|s| s.parse::<f64>().ok())
        };
        let (x1, x2, w) = (attr("x1"), attr("x2"), attr("stroke-width"));
        if let (Some(x1), Some(x2), Some(w)) = (x1, x2, w) {
            if (x1 - tx).abs() < 0.5 && (x2 - tx).abs() < 0.5 && w > widest {
                widest = w;
            }
        }
    }
    println!(
        "\n표 왼쪽 세로선 stroke-width={widest:.3} → 바깥으로 {:.3}px 삐져나감",
        widest / 2.0
    );
    println!(
        "잘리는 양: 왼쪽 {:.3}px ({:.0}%)",
        (widest / 2.0 - (tx - cx)).max(0.0),
        ((widest / 2.0 - (tx - cx)).max(0.0) / widest * 100.0),
    );
    println!("\n판정: 여유가 0이면 그 변의 선은 두께의 절반이 clip 밖 → 얇아 보인다.");
}
