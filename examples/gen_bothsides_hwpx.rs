//! [왕복 실측용] 어울림 양쪽 흐름(2세그)이 들어간 문서를 HWPX 로 내보낸다.
//! 한컴에서 열어 줄 구조(좌·우 조각)가 그대로 보이는지 확인하기 위한 표본.
use rhwp::wasm_api::HwpDocument;

fn main() {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    let long = "가나다라마바사 아자차카타파하 강물이 흐르고 산이 높다 바람이 분다 구름이 간다 하늘이 푸르다 나무가 자란다 새가 웃는다 경치가 아름답다 보리가 여무다 들판이 넓다 여기에 표를 놓으면 글이 어떻게 흐르는지 본다 뒤에 말을 더 붙여 여러 줄이 되도록 한다 그래야 어울림이 보인다";
    doc.insert_text(0, 0, 0, long).unwrap();
    doc.split_paragraph_native(0, 0, long.chars().count()).unwrap();
    let host = doc.get_paragraph_count(0).unwrap() - 1;

    // 폭 12000HU(160px) 어울림(양쪽) 표를 본문 3째 줄 높이·가운데로
    let r = doc
        .create_table_ex_native(0, host as usize, 0, 2, 1, false, Some(&[12000]), None)
        .unwrap();
    let c: serde_json::Value = serde_json::from_str(&r).unwrap();
    let (pi, ci) = (
        c["paraIdx"].as_u64().unwrap() as u32,
        c["controlIdx"].as_u64().unwrap() as u32,
    );
    doc.set_table_properties(
        0, pi, ci,
        r#"{"treatAsChar":false,"textWrap":"Square","textFlow":"BothSides","vertRelTo":"Para","horzRelTo":"Column","vertOffset":0,"horzOffset":0}"#,
    ).unwrap();
    doc.move_table_offset(0, pi, ci, 15000, -4500).unwrap();

    let bb: serde_json::Value =
        serde_json::from_str(&doc.get_table_bbox(0, pi, ci).unwrap()).unwrap();
    let segs: Vec<(i32, i32, u32)> =
        serde_json::from_str(&doc.debug_line_seg_tags(0, 0).unwrap()).unwrap();
    let pairs = segs.windows(2).filter(|w| w[0].0 == w[1].0).count();
    eprintln!(
        "표 x={:.0} y={:.0} w={:.0} / 세그 {}개 · 같은 vpos 쌍 {}쌍",
        bb["x"].as_f64().unwrap(),
        bb["y"].as_f64().unwrap(),
        bb["width"].as_f64().unwrap(),
        segs.len(),
        pairs
    );
    for (v, cs, tag) in &segs {
        eprintln!("  vpos={v:>6} cs={cs:>6} tag=0x{tag:x}");
    }

    let bytes = doc.export_hwpx_native().unwrap();
    std::fs::write("/tmp/bothsides.hwpx", &bytes).unwrap();
    eprintln!("wrote /tmp/bothsides.hwpx ({} bytes)", bytes.len());
}
