//! [법칙 1 이중 진실] HWPX 로드 표의 속성을 한 번 건드리면 HWP5 저장에서 배치가 유실됐다.
//!
//! `Table.attr` 은 `CommonObjAttr` FLAGS 의 미러이고 물리는 `Table.common.*` 다.
//! HWPX 파서는 미러를 bit0 만 채우는데(`materialize_hwpx_table_attrs`),
//! `set_table_properties` 가 **역방향**으로 `table.common.attr = table.attr` 을
//! 덮어쓰고 `raw_ctrl_data` 를 12바이트만 만들어 FLAGS 를 되썼다. 그 결과:
//!
//! - HWP5 직렬화기(`serialize_common_obj_attr`)는 `common.attr != 0` 이면 "HWP 출처"로
//!   보고 그 값을 FLAGS 로 그대로 기록한다 → stripped 미러가 FLAGS 가 된다.
//! - HWPX→HWP 어댑터(`adapt_table_with_context`)는 `raw_ctrl_data.is_empty()` 일 때만
//!   전체 CommonObjAttr 를 합성한다 → 12바이트가 심어져 있으면 합성을 건너뛰고
//!   직렬화기가 그 12바이트를 ctrl_data 로 기록한다(width/height/z_order/margin/
//!   instance_id/prevent_page_break/description 통째 유실).
use rhwp::model::control::Control;
use rhwp::wasm_api::HwpDocument;

/// 기본값과 최대한 다른 배치 물리 — 유실을 눈에 보이게 한다.
const PLACEMENT: &str = r#"{"treatAsChar":false,"textWrap":"TopAndBottom","vertRelTo":"Para",
    "vertAlign":"Bottom","horzRelTo":"Column","horzAlign":"Right","textFlow":"LeftOnly",
    "restrictInPage":true,"allowOverlap":true}"#;

/// 배치 물리 스냅샷 (미러가 아니라 물리만 읽는다).
type Physics = (
    bool,   // treat_as_char
    String, // text_wrap
    String, // vert_rel_to
    String, // vert_align
    String, // horz_rel_to
    String, // horz_align
    String, // text_flow
    bool,   // flow_with_text (restrictInPage)
    bool,   // allow_overlap
    u32,    // width
    u32,    // height
);

fn first_table_physics(doc: &HwpDocument) -> Physics {
    let t = doc.document().sections[0]
        .paragraphs
        .iter()
        .flat_map(|p| p.controls.iter())
        .find_map(|c| match c {
            Control::Table(t) => Some(t),
            _ => None,
        })
        .expect("본문에 표 1개");
    let c = &t.common;
    (
        c.treat_as_char,
        format!("{:?}", c.text_wrap),
        format!("{:?}", c.vert_rel_to),
        format!("{:?}", c.vert_align),
        format!("{:?}", c.horz_rel_to),
        format!("{:?}", c.horz_align),
        format!("{:?}", c.text_flow),
        c.flow_with_text,
        c.allow_overlap,
        c.width,
        c.height,
    )
}

fn first_table_index(doc: &HwpDocument) -> (u32, u32) {
    for (pi, p) in doc.document().sections[0].paragraphs.iter().enumerate() {
        for (ci, c) in p.controls.iter().enumerate() {
            if matches!(c, Control::Table(_)) {
                return (pi as u32, ci as u32);
            }
        }
    }
    panic!("본문에 표가 없다");
}

/// 배치 물리를 지정한 표 1개를 담은 HWPX 바이트.
fn hwpx_with_placed_table() -> Vec<u8> {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    let created: serde_json::Value = serde_json::from_str(
        &doc.create_table_ex(
            r#"{"sectionIdx":0,"paraIdx":0,"charOffset":0,"rowCount":2,"colCount":2}"#,
        )
        .unwrap(),
    )
    .unwrap();
    let (pi, ci) = (
        created["paraIdx"].as_u64().unwrap() as u32,
        created["controlIdx"].as_u64().unwrap() as u32,
    );
    doc.set_table_properties(0, pi, ci, PLACEMENT).unwrap();
    doc.export_hwpx().unwrap()
}

/// 핀: HWPX 로드 표의 **배치와 무관한** 속성 하나를 건드린 뒤 HWP5 로 저장·재파스해도
/// 배치 물리가 전부 보존돼야 한다.
#[test]
fn hwpx_table_placement_survives_property_edit_then_hwp5_save() {
    let mut doc = HwpDocument::from_bytes(&hwpx_with_placed_table()).unwrap();
    let before = first_table_physics(&doc);
    assert_eq!(
        (
            before.1.as_str(),
            before.2.as_str(),
            before.4.as_str(),
            before.7,
            before.8
        ),
        // before.8(allowOverlap) 은 HWPX 표 직렬화기가 "0" 하드코딩이던 시절 false 로 떨어졌다.
        ("TopAndBottom", "Para", "Column", true, true),
        "전제: HWPX 로드 시 배치 물리가 지정대로 들어와 있어야 한다: {before:?}"
    );

    // 배치와 무관한 속성 하나 (셀 간격) — 그래도 setter 는 attr/raw_ctrl_data 를 만진다.
    let (pi, ci) = first_table_index(&doc);
    doc.set_table_properties(0, pi, ci, r#"{"cellSpacing":0}"#)
        .unwrap();

    let hwp = doc.export_hwp().unwrap();
    let after = first_table_physics(&HwpDocument::from_bytes(&hwp).unwrap());
    assert_eq!(
        before, after,
        "표 속성을 한 번 건드리고 .hwp 로 저장하니 배치 물리가 유실됐다"
    );
}

/// 핀: 저장·재파스 없이도 setter 는 물리를 훼손하지 않아야 한다 (미러 역방향 대입 금지).
#[test]
fn hwpx_table_property_edit_does_not_corrupt_common_attr() {
    let mut doc = HwpDocument::from_bytes(&hwpx_with_placed_table()).unwrap();
    let (pi, ci) = first_table_index(&doc);
    doc.set_table_properties(0, pi, ci, r#"{"cellSpacing":0}"#)
        .unwrap();
    let t = doc.document().sections[0]
        .paragraphs
        .iter()
        .flat_map(|p| p.controls.iter())
        .find_map(|c| match c {
            Control::Table(t) => Some(t),
            _ => None,
        })
        .unwrap();
    // attr 은 물리의 비트 표현이다 — PLACEMENT 가 지정한 값이 비트로 남아야 한다.
    let a = t.common.attr;
    assert_eq!(
        (
            (a >> 3) & 0x03,  // vert_rel_to: Para=2
            (a >> 5) & 0x07,  // vert_align: Bottom=2
            (a >> 8) & 0x03,  // horz_rel_to: Column=2
            (a >> 10) & 0x07, // horz_align: Right=2
            (a >> 13) & 0x01, // flow_with_text
            (a >> 14) & 0x01, // allow_overlap
            (a >> 21) & 0x07, // text_wrap: TopAndBottom=1
            (a >> 24) & 0x03, // text_flow: LeftOnly=1
        ),
        (2, 2, 2, 2, 1, 1, 1, 1),
        "setter 가 common.attr 을 stripped 미러로 덮어썼다 (attr={a:#x})"
    );
}
