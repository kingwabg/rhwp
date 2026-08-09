//! [paste-import/셀글자서식 스크래치] QA 프로브의 네이티브 재현 — td 인라인 스타일과
//! <b> 태그 서식이 셀 문단 char_shapes 에 실리는지 실측.
//! 사용: cargo run --example diag_paste_fmt

use rhwp::wasm_api::HwpDocument;

fn main() {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().expect("blank");
    let paste = doc
        .paste_html(
            0,
            0,
            0,
            r#"<table><tr><td>가<b>나</b>다</td><td><b>태그굵게</b></td></tr></table>"#,
        )
        .expect("paste");
    println!("paste={paste}");
    for cell in 0..2usize {
        if let Ok(ch) = doc.get_cell_char_properties_at(0, 0, 0, cell, 0, 0) {
            let v: serde_json::Value = serde_json::from_str(&ch).unwrap();
            println!(
                "cell{cell}.char bold={} size={} color={} shapeId={}",
                v["bold"], v["fontSize"], v["textColor"], v["charShapeId"]
            );
        }
    }
    if let Ok(ch) = doc.get_cell_char_properties_at(0, 0, 0, 0, 0, 2) {
        let v: serde_json::Value = serde_json::from_str(&ch).unwrap();
        println!(
            "live cell0@2 bold={} shapeId={}",
            v["bold"], v["charShapeId"]
        );
    }
    // 모델 관찰: 저장 → 재파스로 셀 문단 char_shapes 실물 확인
    let bytes = doc.export_hwp().expect("export");
    let model = rhwp::parse_document(&bytes).expect("reparse");
    for (pi, para) in model.sections[0].paragraphs.iter().enumerate().take(3) {
        for ctrl in &para.controls {
            if let rhwp::model::control::Control::Table(t) = ctrl {
                for (ci, c) in t.cells.iter().enumerate() {
                    for (cpi, cp) in c.paragraphs.iter().enumerate() {
                        println!(
                            "model pi{pi} cell{ci} para{cpi} text={:?} char_shapes={:?}",
                            cp.text,
                            cp.char_shapes
                                .iter()
                                .map(|r| (r.start_pos, r.char_shape_id))
                                .collect::<Vec<_>>()
                        );
                    }
                }
            }
        }
    }
    // 스타일 실물: 문서 char_shapes 테이블의 굵기/크기/색
    for (i, cs) in model.doc_info.char_shapes.iter().enumerate().take(6) {
        println!(
            "docCharShape[{i}] size={} bold={} attr={:#x}",
            cs.base_size, cs.bold, cs.attr
        );
    }
}
