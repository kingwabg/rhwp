//! 음수 voff 빈 host Square 표가 다음 쪽으로 튕기는 회귀 재현.
use rhwp::wasm_api::HwpDocument;
fn main() {
    for voff in [0i32, -1875, -3750, -4875] {
        let mut doc = HwpDocument::create_empty();
        doc.create_blank_document().unwrap();
        let filler = |n: usize| format!("채움 {n:02} 가나다라마바사 아자차카타파하");
        doc.insert_text(0, 0, 0, &filler(3)).unwrap();
        for n in (1..3).rev() {
            doc.insert_paragraph(0, 0).unwrap();
            doc.insert_text(0, 0, 0, &filler(n)).unwrap();
        }
        let len = doc.get_paragraph_length(0, 2).unwrap();
        let c: serde_json::Value = serde_json::from_str(&doc.create_table_ex(&format!(
            r#"{{"sectionIdx":0,"paraIdx":2,"charOffset":{len},"rowCount":3,"colCount":3,"treatAsChar":false}}"#
        )).unwrap()).unwrap();
        let (pi, ci) = (
            c["paraIdx"].as_u64().unwrap() as u32,
            c["controlIdx"].as_u64().unwrap() as u32,
        );
        doc.set_table_properties(0, pi, ci, &format!(
            r#"{{"treatAsChar":false,"textWrap":"Square","vertRelTo":"Para","horzRelTo":"Column","horzAlign":"Right","vertOffset":{voff},"restrictInPage":false}}"#
        )).unwrap();
        let b: serde_json::Value =
            serde_json::from_str(&doc.get_table_bbox(0, pi, ci).unwrap()).unwrap();
        println!(
            "voff={voff:6}  pages={}  tblPage={} tblY={:.0}",
            doc.page_count(),
            b["pageIndex"],
            b["y"].as_f64().unwrap()
        );
    }
}
