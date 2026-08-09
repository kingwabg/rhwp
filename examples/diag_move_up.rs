//! [편집] 자리차지 표 위로 이동 — 사용자 신고 "위로 옮기면 팅기듯 아래로 이동" 재현.
//! 구성은 신고 스크린샷과 동일: 본문 2문단 + 표(자리차지) + 아래 본문 1문단.
use rhwp::wasm_api::HwpDocument;
fn main() {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    doc.insert_text(0, 0, 0, "ㅇㅇㅇㄴㅇㅇㄴㅁㅇㅁㄴㅇ ㄴㅁㅇㅁ ㅁㄴㅇ")
        .unwrap();
    doc.insert_paragraph(0, 0).unwrap();
    doc.insert_text(0, 0, 0, "첫 줄 본문입니다").unwrap();
    // para2(마지막)에 표 + 아래 텍스트
    let last = doc.get_paragraph_count(0).unwrap() - 1;
    let len = doc.get_paragraph_length(0, last).unwrap_or(0);
    let c: serde_json::Value = serde_json::from_str(&doc.create_table_ex(&format!(
        r#"{{"sectionIdx":0,"paraIdx":{last},"charOffset":{len},"rowCount":3,"colCount":6,"treatAsChar":false}}"#
    )).unwrap()).unwrap();
    let (pi, ci) = (
        c["paraIdx"].as_u64().unwrap() as u32,
        c["controlIdx"].as_u64().unwrap() as u32,
    );
    doc.set_table_properties(0, pi, ci,
        r#"{"treatAsChar":false,"textWrap":"TopAndBottom","vertRelTo":"Para","horzRelTo":"Column","vertOffset":0}"#
    ).unwrap();
    let after_tbl = doc.get_paragraph_count(0).unwrap() - 1;
    doc.insert_text(0, after_tbl, 0, "ㅇㄹㅎㄹㅇㅎㅇㄹㅎㅇㄹㅎ")
        .unwrap();
    // 위로 10프레임 (-375HU = -5px 씩)
    println!("프레임  voff    bbox.y  Δy");
    let mut prev = None;
    for i in 0..10 {
        let b: serde_json::Value =
            serde_json::from_str(&doc.get_table_bbox(0, pi, ci).unwrap()).unwrap();
        let y = b["y"].as_f64().unwrap();
        let p: serde_json::Value =
            serde_json::from_str(&doc.get_table_properties(0, pi, ci).unwrap()).unwrap();
        let d = prev.map(|q: f64| y - q).unwrap_or(0.0);
        println!("{i:5}  {:6}  {y:6.1}  {d:+.1}", p["vertOffset"]);
        prev = Some(y);
        doc.move_table_offset(0, pi, ci, 0, -375).unwrap();
    }
    let b: serde_json::Value =
        serde_json::from_str(&doc.get_table_bbox(0, pi, ci).unwrap()).unwrap();
    println!("최종           {:6.1}", b["y"].as_f64().unwrap());
}
