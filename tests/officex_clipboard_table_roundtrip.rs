//! [2026-08-11 신고 "한컴↔우리 복사·붙여넣기에서 표 테두리 굵기·색이 다르다"]
//! 클립보드 HTML 왕복 핀 — 수출한 HTML 을 다시 수입했을 때 표의 물성이 보존돼야 한다.
//!
//! 종전 결함(수출측): ① `BorderLine.width` 가 mm 표의 **인덱스**인데 그대로 `px` 로 적어
//! (idx4 → "4.0px") 왕복마다 테두리가 굵어졌다(실측 잉크 1.95배, 0.25mm→0.5mm)
//! ② 열 폭·행 높이를 아예 안 내보내 붙여넣는 쪽이 균등 분할했다 ③ 셀 안 여백이
//! 하드코딩(1px 5px)이라 왕복마다 바뀌었다.

use rhwp::wasm_api::HwpDocument;

/// 본문 첫 표를 찾아 (테두리 인덱스, 열 폭, 셀 안여백 좌/상) 를 뽑는다.
fn table_facts(doc: &HwpDocument) -> (u8, Vec<u32>, (i16, i16)) {
    for para in &doc.document().sections[0].paragraphs {
        for ctrl in &para.controls {
            if let rhwp::model::control::Control::Table(t) = ctrl {
                let bf_id = t.cells.first().map(|c| c.border_fill_id).unwrap_or(0);
                let width_idx = doc
                    .document()
                    .doc_info
                    .border_fills
                    .get((bf_id as usize).saturating_sub(1))
                    .map(|bf| bf.borders[0].width)
                    .unwrap_or(255);
                let pad = t
                    .cells
                    .first()
                    .map(|c| c.effective_padding(&t.padding))
                    .unwrap_or_default();
                return (width_idx, t.get_column_widths(), (pad.left, pad.top));
            }
        }
    }
    panic!("표 컨트롤 없음");
}

fn make_doc_with_table(col_widths: &str) -> (HwpDocument, u32, u32) {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    let c: serde_json::Value = serde_json::from_str(
        &doc.create_table_ex(
            r#"{"sectionIdx":0,"paraIdx":0,"charOffset":0,"rowCount":2,"colCount":2,"treatAsChar":false}"#,
        )
        .unwrap(),
    )
    .unwrap();
    let (pi, ci) = (
        c["paraIdx"].as_u64().unwrap() as u32,
        c["controlIdx"].as_u64().unwrap() as u32,
    );
    doc.set_table_column_widths(0, pi, ci, col_widths).unwrap();
    (doc, pi, ci)
}

/// 수출 → 수입 왕복에서 테두리 굵기 인덱스가 그대로여야 한다(굵어짐 금지).
#[test]
fn clipboard_html_roundtrip_preserves_border_width_index() {
    let (mut doc, pi, ci) = make_doc_with_table("[8000,8000]");
    let (idx_before, _, _) = table_facts(&doc);
    assert_eq!(idx_before, 1, "새 표 기본 테두리는 0.12mm(index 1)");

    let html = doc.export_control_html(0, pi, "", ci).expect("HTML 수출");
    assert!(
        !html.contains("border-left:1.0px") && !html.contains("border-left:4.0px"),
        "인덱스를 px 로 적고 있다(왕복마다 굵어짐): {}",
        &html[..html.len().min(400)]
    );

    let mut dst = HwpDocument::create_empty();
    dst.create_blank_document().unwrap();
    dst.paste_html(0, 0, 0, &html).expect("HTML 수입");
    let (idx_after, _, _) = table_facts(&dst);
    assert_eq!(
        idx_after, idx_before,
        "왕복 후 테두리 굵기 인덱스가 {idx_before} → {idx_after} 로 변했다"
    );
}

/// 열 폭이 왕복에서 보존돼야 한다(균등 분할로 뭉개짐 금지).
#[test]
fn clipboard_html_roundtrip_preserves_column_widths() {
    let (mut doc, pi, ci) = make_doc_with_table("[6000,12000]");
    let (_, widths_before, _) = table_facts(&doc);
    assert_eq!(widths_before, vec![6000, 12000]);

    let html = doc.export_control_html(0, pi, "", ci).expect("HTML 수출");
    let mut dst = HwpDocument::create_empty();
    dst.create_blank_document().unwrap();
    dst.paste_html(0, 0, 0, &html).expect("HTML 수입");
    let (_, widths_after, _) = table_facts(&dst);

    assert_eq!(widths_after.len(), 2, "열 수가 달라짐: {widths_after:?}");
    for (b, a) in widths_before.iter().zip(widths_after.iter()) {
        let diff = (*b as i64 - *a as i64).abs();
        assert!(
            diff <= 60,
            "열 폭 왕복 오차 과다: {widths_before:?} → {widths_after:?}"
        );
    }
}

/// 셀 안 여백이 왕복에서 보존돼야 한다(하드코딩 1px 5px 금지).
#[test]
fn clipboard_html_roundtrip_preserves_cell_padding() {
    let (mut doc, pi, ci) = make_doc_with_table("[8000,8000]");
    let (_, _, pad_before) = table_facts(&doc);
    assert_eq!(pad_before, (510, 142), "새 표 기본 안여백(1.80mm/0.50mm)");

    let html = doc.export_control_html(0, pi, "", ci).expect("HTML 수출");
    let mut dst = HwpDocument::create_empty();
    dst.create_blank_document().unwrap();
    dst.paste_html(0, 0, 0, &html).expect("HTML 수입");
    let (_, _, pad_after) = table_facts(&dst);

    assert!(
        (pad_before.0 - pad_after.0).abs() <= 10 && (pad_before.1 - pad_after.1).abs() <= 10,
        "셀 안여백 왕복 오차: {pad_before:?} → {pad_after:?}"
    );
}

/// 본문 폭을 넘는 표를 붙여넣으면 비율을 지켜 본문 폭에 맞춘다(용지 밖 돌출 금지).
#[test]
fn pasted_oversize_table_is_clamped_to_body_width() {
    // 본문 폭 42520HU 을 훌쩍 넘는 표 (30000 + 30000)
    let html = r#"<html><body><table style="border-collapse:collapse;width:600.0pt;">
<tr><td style="width:300.0pt;height:10.0pt;border-left:0.34pt solid #000000;">가</td>
<td style="width:300.0pt;height:10.0pt;border-left:0.34pt solid #000000;">나</td></tr>
</table></body></html>"#;
    let mut dst = HwpDocument::create_empty();
    dst.create_blank_document().unwrap();
    dst.paste_html(0, 0, 0, html).expect("HTML 수입");
    let (_, widths, _) = table_facts(&dst);
    let total: u32 = widths.iter().sum();
    assert!(
        total <= 42520,
        "본문 폭(42520)을 넘는 표가 그대로 들어왔다: {widths:?} 합={total}"
    );
    // 비율 보존 — 두 열이 같은 폭이었으므로 왕복 후에도 서로 비슷해야 한다
    let diff = (widths[0] as i64 - widths[1] as i64).abs();
    assert!(diff <= 2, "클램프가 비율을 깨뜨렸다: {widths:?}");
}
