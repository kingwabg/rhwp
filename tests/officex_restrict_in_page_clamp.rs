//! [officex] restrictInPage(bit13) 클램프 계약 핀.
//!
//! 계약: 제한 ON = 본문 영역 안, 제한 OFF = 용지 안(밖으로는 못 나감).
//! 실측 결함(2026-07-26): 용지 기하(current_paper_height)가 **바탕쪽에서만** 설정돼
//! 일반 문서는 0 → compute_table_y_position 의 폴백(col_area.y*2 + height)이 위·아래
//! 여백 대칭을 가정하고 용지 높이를 과대평가(A4 1141.4 vs 실제 1122.5) → 제한 OFF 표가
//! 용지 밖으로 18.9px 새어나갔다. 수리 = build_render_tree 진입점에서 매 페이지 설정.

use rhwp::wasm_api::HwpDocument;

struct Placed {
    top: f64,
    bottom: f64,
    paper_height: f64,
    body_bottom: f64,
}

fn place(restrict: bool) -> Placed {
    let mm = 283.46_f64;
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().expect("blank");
    doc.insert_text(0, 0, 0, "본문").expect("text");
    let created = doc.create_table(0, 0, 2, 3, 3).expect("table");
    let v: serde_json::Value = serde_json::from_str(&created).unwrap();
    let (pi, ci) = (
        v["paraIdx"].as_u64().unwrap() as u32,
        v["controlIdx"].as_u64().unwrap() as u32,
    );
    doc.set_table_properties(
        0,
        pi,
        ci,
        &format!(
            r#"{{"treatAsChar":false,"textWrap":"TopAndBottom","vertRelTo":"Para","restrictInPage":{restrict},"vertOffset":0}}"#
        ),
    )
    .expect("props");
    // 쪽을 한참 넘기는 이동 — 클램프가 걸리지 않으면 종이 밖으로 나간다.
    doc.move_table_offset(0, pi, ci, 0, (400.0 * mm).round() as i32)
        .expect("move");

    let b: serde_json::Value =
        serde_json::from_str(&doc.get_table_bbox(0, pi, ci).expect("bbox")).unwrap();
    let pg: serde_json::Value =
        serde_json::from_str(&doc.get_page_info(0).expect("pageinfo")).unwrap();
    let top = b["y"].as_f64().unwrap();
    let paper_height = pg["height"].as_f64().unwrap();
    // 본문 아래끝 = 용지높이 − (아래여백 + 꼬리말여백)
    let body_bottom =
        paper_height - pg["marginBottom"].as_f64().unwrap() - pg["marginFooter"].as_f64().unwrap();
    Placed {
        top,
        bottom: top + b["height"].as_f64().unwrap(),
        paper_height,
        body_bottom,
    }
}

#[test]
fn restrict_on_keeps_table_inside_body_area() {
    let p = place(true);
    assert!(
        p.bottom <= p.body_bottom + 0.5,
        "제한 ON 인데 본문 밖: bottom={:.1} > body_bottom={:.1}",
        p.bottom,
        p.body_bottom
    );
}

#[test]
fn restrict_off_still_keeps_table_inside_paper() {
    let p = place(false);
    // 제한 OFF 는 여백 영역 진입을 허용한다 — 본문 밖으로 나갈 수 있어야 한다.
    assert!(
        p.bottom > p.body_bottom + 0.5,
        "제한 OFF 인데 본문에 갇힘: bottom={:.1} body_bottom={:.1}",
        p.bottom,
        p.body_bottom
    );
    // 그러나 용지 밖은 금지 — 종전 폴백이 여백 대칭을 가정해 18.9px 새던 자리.
    assert!(
        p.bottom <= p.paper_height + 0.5,
        "제한 OFF 표가 용지 밖으로: bottom={:.1} > paper={:.1} (top={:.1})",
        p.bottom,
        p.paper_height,
        p.top
    );
}
