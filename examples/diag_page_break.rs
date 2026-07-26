//! [capability-map E3 스크래치] 표 나눔(pageBreak) 3종 대조 — "안 나눔"이 지켜지는가.
//! 쪽을 넘기는 큰 표를 만들고 pageBreak 만 바꿔 쪽 수·표 조각 분포를 본다.
//! 사용: cargo run --release --example diag_page_break

use rhwp::wasm_api::HwpDocument;

fn probe(page_break: u8, rows: u32) -> (u32, usize, f64) {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().expect("blank");
    doc.insert_text(0, 0, 0, "앞 본문").expect("text");
    let created = doc.create_table(0, 0, 4, 3, 3).expect("table");
    let v: serde_json::Value = serde_json::from_str(&created).unwrap();
    let (pi, ci) = (
        v["paraIdx"].as_u64().unwrap() as u32,
        v["controlIdx"].as_u64().unwrap() as u32,
    );
    doc.set_table_properties(0, pi, ci, &format!(
        r#"{{"treatAsChar":false,"textWrap":"TopAndBottom","vertRelTo":"Para","pageBreak":{page_break}}}"#
    )).expect("props");
    let readback = doc.get_table_properties(0, pi, ci).expect("getprops");
    let rv: serde_json::Value = serde_json::from_str(&readback).unwrap();
    if rows == 40 {
        println!("  setter 왕복: 요청={page_break} 회신={} (전체키: {})", rv["pageBreak"],
            rv.as_object().map(|o| o.keys().cloned().collect::<Vec<_>>().join(",")).unwrap_or_default());
    }
    for _ in 0..rows {
        doc.insert_table_row(0, pi, ci, 1, true).expect("row");
    }
    // 표 조각이 몇 쪽에 걸쳐 있는지 = bbox 의 pageIndex 분포
    let boxes: serde_json::Value =
        serde_json::from_str(&doc.get_table_cell_bboxes(0, pi, ci, None).expect("bboxes")).unwrap();
    let arr = boxes.as_array().cloned().unwrap_or_default();
    let mut pages: Vec<i64> = arr
        .iter()
        .filter_map(|b| b["pageIndex"].as_i64())
        .collect();
    pages.sort_unstable();
    pages.dedup();
    let max_bottom = arr
        .iter()
        .filter_map(|b| Some(b["y"].as_f64()? + b["h"].as_f64()?))
        .fold(0.0f64, f64::max);
    (doc.page_count(), pages.len(), max_bottom)
}

fn main() {
    for rows in [40u32, 90] {
        for (pb, name) in [(0u8, "0=안나눔"), (1, "1=셀단위"), (2, "2=나눔")] {
            let (pages, spread, bottom) = probe(pb, rows);
            println!("rows={rows:3} pageBreak={name:9} 쪽수={pages} 표가걸친쪽수={spread} 표바닥={bottom:7.1}");
        }
    }
}
