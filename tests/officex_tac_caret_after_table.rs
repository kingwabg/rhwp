//! [신고 2026-08-05] 글자처럼 취급 표 오른쪽에 커서를 두고 치면 글자가 겹치고 표가 밀린다.
//!
//! 원인: 문단 길이를 text 글자 수로만 재서 **표 뒤 오프셋이 존재하지 않았다**.
//! End·클릭이 표 앞 오프셋으로 접히고 거기 삽입되니 표가 오른쪽으로 밀렸고,
//! 캐럿은 클릭한 자리(표 오른쪽)에 있는데 글자는 표 앞에 그려져 겹쳐 보였다.
use rhwp::renderer::render_tree::{RenderNode, RenderNodeType};
use rhwp::wasm_api::HwpDocument;

fn table_and_text_x(doc: &mut HwpDocument) -> (f64, f64, String) {
    let tree = doc.build_page_render_tree(0).unwrap();
    let mut table_x = f64::NAN;
    let mut text_x = f64::NAN;
    let mut text = String::new();
    fn walk(n: &RenderNode, tx: &mut f64, sx: &mut f64, s: &mut String) {
        if matches!(n.node_type, RenderNodeType::Table(_)) {
            if tx.is_nan() {
                *tx = n.bbox.x;
            }
            return; // 셀 안 글자는 본문이 아니다
        }
        if let RenderNodeType::TextRun(tr) = &n.node_type {
            if !tr.text.trim().is_empty() {
                if sx.is_nan() {
                    *sx = n.bbox.x;
                }
                s.push_str(&tr.text);
            }
        }
        for c in &n.children {
            walk(c, tx, sx, s);
        }
    }
    walk(&tree.root, &mut table_x, &mut text_x, &mut text);
    (table_x, text_x, text)
}

fn make_tac_table(doc: &mut HwpDocument) {
    let c: serde_json::Value = serde_json::from_str(
        &doc.create_table_ex(
            r#"{"sectionIdx":0,"paraIdx":0,"charOffset":0,"rowCount":3,"colCount":3,"treatAsChar":true}"#,
        )
        .unwrap(),
    )
    .unwrap();
    let (pi, ci) = (
        c["paraIdx"].as_u64().unwrap() as u32,
        c["controlIdx"].as_u64().unwrap() as u32,
    );
    doc.set_table_column_widths(0, pi, ci, "[5000,5000,5000]")
        .unwrap();
}

/// 표만 있는 문단의 논리 길이는 1 — 표 **뒤**에 캐럿을 둘 자리가 있어야 한다.
#[test]
fn inline_table_counts_as_one_char_so_caret_can_sit_after_it() {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    make_tac_table(&mut doc);

    let len = doc.get_paragraph_length(0, 0).unwrap();
    assert_eq!(
        len, 1,
        "글자처럼 취급 표는 한 글자로 세어야 한다(표 뒤 오프셋 확보)"
    );

    let before: serde_json::Value =
        serde_json::from_str(&doc.get_cursor_rect(0, 0, 0).unwrap()).unwrap();
    let after: serde_json::Value =
        serde_json::from_str(&doc.get_cursor_rect(0, 0, 1).unwrap()).unwrap();
    let (x0, x1) = (before["x"].as_f64().unwrap(), after["x"].as_f64().unwrap());
    assert!(
        x1 - x0 > 100.0,
        "표 뒤 캐럿이 표 폭만큼 오른쪽에 서야 한다: 앞={x0} 뒤={x1}"
    );
}

/// 표 오른쪽(End)에 치면 글자가 표 뒤에 들어가고 **표는 밀리지 않는다**.
#[test]
fn typing_after_inline_table_keeps_table_in_place() {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    make_tac_table(&mut doc);
    let (table_x0, _, _) = table_and_text_x(&mut doc);

    // 사용자 조작: End(문단 끝 = 표 뒤) 위치에 한 글자씩 입력
    for ch in ["하", "나", "둘"] {
        let at = doc.get_paragraph_length(0, 0).unwrap();
        doc.insert_text(0, 0, at, ch).unwrap();
    }

    let (table_x1, text_x, text) = table_and_text_x(&mut doc);
    assert_eq!(text, "하나둘", "친 글자가 그대로 있어야 한다");
    assert!(
        (table_x1 - table_x0).abs() < 0.5,
        "표가 밀렸다: {table_x0} → {table_x1}"
    );
    assert!(
        text_x > table_x1 + 100.0,
        "글자가 표 오른쪽에 와야 한다: 표={table_x1} 글자={text_x}"
    );
}

/// [신고 2026-08-05] 표 오른쪽/왼쪽 **밖**을 클릭하면 표 뒤/앞에 캐럿이 서야 한다.
/// 종전엔 hit_test 에 인라인 표 분기가 없어 클릭이 offset 0/1 로 안 가고 표 아래로 튀었다.
#[test]
fn clicking_beside_inline_table_places_caret_before_or_after() {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    make_tac_table(&mut doc);

    // 표 bbox 를 렌더 트리에서 얻는다
    let tree = doc.build_page_render_tree(0).unwrap();
    let mut tb: Option<(f64, f64, f64, f64)> = None;
    fn find(n: &RenderNode, out: &mut Option<(f64, f64, f64, f64)>) {
        if matches!(n.node_type, RenderNodeType::Table(_)) && out.is_none() {
            *out = Some((n.bbox.x, n.bbox.y, n.bbox.width, n.bbox.height));
            return;
        }
        for c in &n.children {
            find(c, out);
        }
    }
    find(&tree.root, &mut tb);
    let (tx, ty, tw, th) = tb.expect("표 노드");
    let mid_y = ty + th / 2.0;

    // 표 오른쪽 밖 클릭 → offset 1 (표 뒤)
    let right_json = doc.hit_test_native(0, tx + tw + 80.0, mid_y).unwrap();
    let right: serde_json::Value = serde_json::from_str(&right_json).unwrap();
    assert_eq!(
        right["charOffset"].as_u64().unwrap(),
        1,
        "표 오른쪽 밖 클릭은 표 뒤(offset 1)여야 한다: {right_json}"
    );
    assert!(
        right["paragraphIndex"].as_u64().is_some() && right.get("cellPath").is_none(),
        "본문 캐럿이어야 한다(셀 진입 아님): {right_json}"
    );

    // 표 왼쪽 밖(표 시작보다 왼쪽) 클릭 → offset 0 (표 앞)
    if tx > 20.0 {
        let left_json = doc.hit_test_native(0, tx - 10.0, mid_y).unwrap();
        let left: serde_json::Value = serde_json::from_str(&left_json).unwrap();
        assert_eq!(
            left["charOffset"].as_u64().unwrap(),
            0,
            "표 왼쪽 밖 클릭은 표 앞(offset 0)이어야 한다: {left_json}"
        );
    }
}
