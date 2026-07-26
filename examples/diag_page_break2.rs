//! [E3 판별 케이스] "남은 공간엔 못 들어가지만 한 쪽에는 들어가는" 표로 pageBreak 를 가른다.
//! 기대(한컴): 0=안나눔 → 표 통째로 다음 쪽 이동(분할 없음) · 2=나눔 → 쪽 경계에서 분할.
//! 사용: cargo run --release --example diag_page_break2

use rhwp::wasm_api::HwpDocument;

fn probe(page_break: u8, filler_lines: usize, rows: u32) -> (u32, usize, i64, i64) {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().expect("blank");
    // 앞 본문으로 현재 쪽 잔여 공간을 좁힌다
    let filler = "가나다라마바사아자차카타파하 ".repeat(3);
    for _ in 0..filler_lines {
        let len = doc.get_paragraph_length(0, 0).unwrap_or(0);
        doc.insert_text(0, 0, len, &format!("{filler}\n")).expect("text");
    }
    let len = doc.get_paragraph_length(0, 0).unwrap_or(0);
    let created = doc.create_table(0, 0, len, 3, 3).expect("table");
    let v: serde_json::Value = serde_json::from_str(&created).unwrap();
    let (pi, ci) = (
        v["paraIdx"].as_u64().unwrap() as u32,
        v["controlIdx"].as_u64().unwrap() as u32,
    );
    doc.set_table_properties(0, pi, ci, &format!(
        r#"{{"treatAsChar":false,"textWrap":"TopAndBottom","vertRelTo":"Para","pageBreak":{page_break}}}"#
    )).expect("props");
    for _ in 0..rows {
        doc.insert_table_row(0, pi, ci, 1, true).expect("row");
    }
    let boxes: serde_json::Value =
        serde_json::from_str(&doc.get_table_cell_bboxes(0, pi, ci, None).expect("bb")).unwrap();
    let arr = boxes.as_array().cloned().unwrap_or_default();
    let mut pages: Vec<i64> = arr.iter().filter_map(|b| b["pageIndex"].as_i64()).collect();
    pages.sort_unstable();
    pages.dedup();
    let first = pages.first().copied().unwrap_or(-1);
    let last = pages.last().copied().unwrap_or(-1);
    (doc.page_count(), pages.len(), first, last)
}

fn main() {
    for (fl, rows) in [(10usize, 40u32), (20, 40), (25, 40)] {
        for (pb, name) in [(0u8, "0=안나눔"), (2, "2=나눔")] {
            let (pages, spread, f, l) = probe(pb, fl, rows);
            println!("앞줄={fl:3} 표행={rows:2} {name:9} 쪽수={pages} 표가걸친쪽수={spread} (쪽 {f}..{l})");
        }
    }
}
