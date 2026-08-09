//! [트랙3] 도형·글상자 treatAsChar 토글 마이그레이션 핀 — 그림(picture.rs) 계약의 도형판.
//!
//! false→true: h/v_rel_to=Para · offset=0 · host line_segs[0].line_height =
//! max(common.height, shape_attr.current_height) · baseline = round(0.85h) (4필드).
//! true→false(빈 문단): line_segs 재구성(migrate_empty_picture_para_inline_to_floating
//! 의 Shape 대응 경로) — 낡은 개체 높이 줄이 남지 않는다.
use rhwp::model::control::Control;
use rhwp::model::shape::{HorzRelTo, VertRelTo};
use rhwp::wasm_api::HwpDocument;

const W_HU: u32 = 9000;
const H_HU: u32 = 6000;

fn create_floating_shape(doc: &mut HwpDocument, shape_type: &str) -> (usize, usize) {
    let json = format!(
        r#"{{"sectionIdx":0,"paraIdx":0,"charOffset":0,"shapeType":"{shape_type}","width":{W_HU},"height":{H_HU},"treatAsChar":false}}"#
    );
    let c: serde_json::Value =
        serde_json::from_str(&doc.create_shape_control(&json).unwrap()).unwrap();
    (
        c["paraIdx"].as_u64().unwrap() as usize,
        c["controlIdx"].as_u64().unwrap() as usize,
    )
}

fn shape_fields(doc: &HwpDocument, pi: usize, ci: usize) -> (HorzRelTo, VertRelTo, u32, u32, i32) {
    let para = &doc.document().sections[0].paragraphs[pi];
    let Control::Shape(s) = &para.controls[ci] else {
        panic!("controls[{ci}] 가 Shape 이 아니다");
    };
    let c = s.common();
    let expected_h = (c.height as i32).max(s.shape_attr().current_height as i32);
    (
        c.horz_rel_to,
        c.vert_rel_to,
        c.horizontal_offset,
        c.vertical_offset,
        expected_h,
    )
}

fn toggle_migration_contract(shape_type: &str) {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    let (pi, ci) = create_floating_shape(&mut doc, shape_type);

    // ── false→true: 4필드 마이그레이션 ──
    doc.set_shape_properties(0, pi as u32, ci as u32, r#"{"treatAsChar":true}"#)
        .unwrap();
    let (h_rel, v_rel, h_off, v_off, expected_h) = shape_fields(&doc, pi, ci);
    assert_eq!(h_rel, HorzRelTo::Para, "{shape_type}: horz_rel_to 미리셋");
    assert_eq!(v_rel, VertRelTo::Para, "{shape_type}: vert_rel_to 미리셋");
    assert_eq!(h_off, 0, "{shape_type}: horizontal_offset 미리셋");
    assert_eq!(v_off, 0, "{shape_type}: vertical_offset 미리셋");

    let seg = doc.document().sections[0].paragraphs[pi].line_segs[0].clone();
    assert_eq!(
        seg.line_height, expected_h,
        "{shape_type}: line_segs[0].line_height != max(common.height, current_height)"
    );
    assert_eq!(
        seg.baseline_distance,
        (expected_h as f64 * 0.85).round() as i32,
        "{shape_type}: baseline != round(0.85 * line_height)"
    );

    // ── true→false (빈 문단): line_segs 재구성 — 개체 높이 줄이 남으면 stale ──
    doc.set_shape_properties(0, pi as u32, ci as u32, r#"{"treatAsChar":false}"#)
        .unwrap();
    let segs = &doc.document().sections[0].paragraphs[pi].line_segs;
    assert!(
        segs.iter().all(|s| s.line_height < expected_h),
        "{shape_type}: TAC off 후에도 line_segs 에 개체 높이 줄 잔재 — 재구성 실패. \
         lh={:?}",
        segs.iter().map(|s| s.line_height).collect::<Vec<_>>()
    );
}

#[test]
fn rectangle_tac_toggle_migrates_four_fields_and_rebuilds_segs() {
    toggle_migration_contract("rectangle");
}

#[test]
fn textbox_tac_toggle_migrates_four_fields_and_rebuilds_segs() {
    toggle_migration_contract("textbox");
}
