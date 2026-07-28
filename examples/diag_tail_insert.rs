//! 문서 끝 삽입된 좁은 Square 표 host 가 새 페이지로 배정되는 문제 재현.
use rhwp::wasm_api::HwpDocument;
fn main() {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    let filler = |n: usize| format!("채움 {n:02} 가나다라마바사 아자차카타파하 강물이 흐르고");
    doc.insert_text(0, 0, 0, &filler(6)).unwrap();
    for n in (1..6).rev() {
        doc.insert_paragraph(0, 0).unwrap();
        doc.insert_text(0, 0, 0, &filler(n)).unwrap();
    }
    let len = doc.get_paragraph_length(0, 5).unwrap();
    let c: serde_json::Value = serde_json::from_str(&doc.create_table_ex(&format!(
        r#"{{"sectionIdx":0,"paraIdx":5,"charOffset":{len},"rowCount":3,"colCount":2,"treatAsChar":false}}"#
    )).unwrap()).unwrap();
    let (pi, ci) = (c["paraIdx"].as_u64().unwrap() as u32, c["controlIdx"].as_u64().unwrap() as u32);
    doc.set_table_column_widths(0, pi, ci, "[10000,10000]").unwrap();
    doc.set_table_properties(0, pi, ci,
        r#"{"treatAsChar":false,"textWrap":"Square","vertRelTo":"Para","horzRelTo":"Column","horzAlign":"Right","vertOffset":0,"horzOffset":0}"#
    ).unwrap();
    // ⚠ 문단별 column_type 검사(쪽나누기 플래그 오염 추적)는 `doc.core` 가 private 이라
    //   예제에서 볼 수 없다. 이 블록이 컴파일되지 않아 `cargo test`(예제까지 빌드)가
    //   c84529431 이후 계속 깨져 있었다 → 제거(2026-07-28).
    //   같은 관찰이 다시 필요하면 src/ 안의 단위 테스트로 옮겨서 볼 것.
    let b: serde_json::Value = serde_json::from_str(&doc.get_table_bbox(0, pi, ci).unwrap()).unwrap();
    println!("pages={} tblPage={} y={:.0}", doc.page_count(), b["pageIndex"], b["y"].as_f64().unwrap());
}
