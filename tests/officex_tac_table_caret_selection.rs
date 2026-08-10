//! TAC(글자처럼 취급) 표가 텍스트 사이에 앵커된 문단("가나[표]다라")의
//! 캐럿/선택 좌표 회귀 핀 — 한컴독스 웹한글 실측(2026-08-10) 정합.
//!
//! layout_inline_table_paragraph 가 TextRun.char_start 를 텍스트 좌표로 기록해
//! (논리 좌표 아님), 표 뒤 첫 글자('다')의 선택 rect 가 두 글자 폭이 되고
//! 마지막 글자('라')의 선택 rect 는 비어 캐럿 밑줄이 무너지던 결함의 핀.

use std::path::Path;

use rhwp::wasm_api::HwpDocument;

fn json_number(json: &str, key: &str) -> f64 {
    let pattern = format!("\"{}\":", key);
    let start = json.find(&pattern).expect("json key not found") + pattern.len();
    let rest = &json[start..];
    let end = rest
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
        .unwrap_or(rest.len());
    rest[..end].parse::<f64>().expect("json number parse")
}

fn load_doc() -> HwpDocument {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("samples/officex_tac_mid_anchor.hwpx");
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {}", path.display(), e));
    HwpDocument::from_bytes(&bytes).expect("parse officex_tac_mid_anchor.hwpx")
}

/// 문단 0: 가나[2×2 TAC 표 30mm]다라 — 논리 오프셋 0..=5 캐럿 x 가 단조 증가하고,
/// 표(113.4px)를 건너뛰며, 캐럿 높이는 줄이 아니라 **텍스트** 높이(≈16px)다
/// (한컴 실측: 캐럿은 개체 높이를 따라가지 않는다).
#[test]
fn tac_table_paragraph_cursor_rects_follow_logical_offsets() {
    let doc = load_doc();
    let expected_x = [113.4, 129.4, 145.4, 258.7, 274.7, 290.7];
    for (off, expected) in expected_x.iter().enumerate() {
        let json = doc
            .get_cursor_rect(0, 0, off as u32)
            .unwrap_or_else(|_| panic!("cursor rect off={off}"));
        let x = json_number(&json, "x");
        let h = json_number(&json, "height");
        assert!(
            (x - expected).abs() < 1.0,
            "off={off}: x={x} (기대 {expected}) json={json}"
        );
        assert!(
            (14.0..=20.0).contains(&h),
            "off={off}: 캐럿 높이는 텍스트 높이(≈16px)여야 함 — h={h} json={json}"
        );
    }
}

/// Home/End(줄 시작·끝) 캐럿도 줄 상자(표 높이)가 아니라 텍스트 높이여야 한다.
#[test]
fn tac_table_paragraph_line_edge_cursor_height_is_text_height() {
    let doc = load_doc();
    for at_end in [false, true] {
        let json = doc
            .get_cursor_rect_on_line(0, 0, 0, at_end, u32::MAX, 0, 0, 0)
            .expect("cursor rect on line");
        let h = json_number(&json, "height");
        assert!(
            (14.0..=20.0).contains(&h),
            "at_end={at_end}: 줄 경계 캐럿 높이는 텍스트 높이여야 함 — {json}"
        );
    }
}

/// 표 오른쪽 본문 텍스트 위 클릭은 그 텍스트 오프셋으로 — 표 우측 히트 밴드가
/// 무한대라 텍스트 클릭·드래그 앵커까지 '표 뒤(1)'로 삼키던 회귀 핀.
#[test]
fn click_on_text_right_of_table_hits_text_not_table_band() {
    let doc = load_doc();
    // '라' 글리프 중앙쯤 (x≈282, 줄 y≈160) — 표 뒤 텍스트 두 번째 글자
    let json = doc.hit_test(0, 282.0, 160.0).expect("hitTest");
    let off = {
        let pattern = "\"charOffset\":";
        let start = json.find(pattern).expect("charOffset") + pattern.len();
        let rest = &json[start..];
        let end = rest
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(rest.len());
        rest[..end].parse::<usize>().expect("usize")
    };
    assert!(
        off >= 3,
        "표 오른쪽 텍스트 클릭이 표 밴드(1)에 삼켜짐: off={off} json={json}"
    );
}

/// 표 뒤 글자들의 선택 rect: '다'(논리 3..4) = 한 글자 폭, '라'(논리 4..5) = 비지 않음.
#[test]
fn tac_table_paragraph_selection_rects_after_table_are_single_char() {
    let doc = load_doc();

    let da = doc.get_selection_rects(0, 0, 3, 0, 4).expect("sel 3..4");
    let da_w = json_number(&da, "width");
    let da_x = json_number(&da, "x");
    assert!(
        (da_x - 258.7).abs() < 1.0 && (da_w - 16.0).abs() < 2.0,
        "'다' rect 는 표 오른쪽에서 한 글자 폭이어야 함: {da}"
    );

    let ra = doc.get_selection_rects(0, 0, 4, 0, 5).expect("sel 4..5");
    assert!(
        ra.trim() != "[]",
        "'라'(마지막 글자) rect 가 비면 캐럿 밑줄이 무너진다: {ra}"
    );
    let ra_x = json_number(&ra, "x");
    let ra_w = json_number(&ra, "width");
    assert!(
        (ra_x - 274.7).abs() < 1.0 && (ra_w - 16.0).abs() < 2.0,
        "'라' rect 위치/폭: {ra}"
    );
}
