//! TAC 표 셀 아래 탈출·논리 편집 회귀 핀 (2026-08-10 사용자 신고 2건)
//!
//! ① 마지막 문단의 TAC 표 마지막 행에서 ↓ 탈출: 종전 `exit_table_vertical` 문서 끝
//!    분기가 (para 0, offset 0) 하드코딩이라 커서가 **표 앞**으로 갔고, 이어 친
//!    공백/글자가 표 앞에 삽입돼 표가 밀렸다. 한컴: 아래 탈출 = 표 뒤.
//! ② IME 조합 preedit 교체는 커서 논리 좌표로 삽입·삭제한다 — deleteTextLogical 이
//!    없어 텍스트 좌표 deleteText 로 지우면 한 칸 밀려 자모가 잔류("ㄴ니" 이중 입력).

use std::path::Path;

use rhwp::wasm_api::HwpDocument;

fn load(name: &str) -> HwpDocument {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("samples")
        .join(name);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {}", path.display(), e));
    HwpDocument::from_bytes(&bytes).unwrap_or_else(|e| panic!("parse {name}: {e:?}"))
}

fn json_usize(json: &str, key: &str) -> usize {
    let pattern = format!("\"{}\":", key);
    let start = json.find(&pattern).expect("key") + pattern.len();
    let rest = &json[start..];
    let end = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    rest[..end].parse().expect("usize")
}

/// 마지막 문단 "가나[2×2 TAC 표]다라"의 마지막 셀에서 ↓ → 호스트 문단 끝(표 뒤, 논리 5).
#[test]
fn down_exit_from_last_cell_goes_after_table_at_document_end() {
    let doc = load("officex_tac_last_para.hwpx");
    // 셀 (마지막, cei=3) 문단 0 오프셋 0에서 아래로
    let json = doc
        .move_vertical(0, 0, 0, 1, 130.0, 0, 2, 3, 0)
        .expect("moveVertical");
    let para = json_usize(&json, "paragraphIndex");
    let off = json_usize(&json, "charOffset");
    assert_eq!(para, 0, "호스트 문단이어야 함: {json}");
    assert_eq!(
        off, 5,
        "표 뒤(문단 끝, 논리 5)여야 함 — 0이면 표 앞 회귀: {json}"
    );
}

/// 논리 좌표 preedit 교체 왕복: 표 뒤(논리 3)에 'ㄴ' 삽입 → 논리 3에서 1자 삭제 → 원상복구.
#[test]
fn logical_insert_delete_roundtrip_after_table() {
    let mut doc = load("officex_tac_mid_anchor.hwpx");
    let before = doc.get_text_range(0, 0, 0, 20).expect("text");
    doc.insert_text_logical(0, 0, 3, "ㄴ").expect("insert");
    let mid = doc.get_text_range(0, 0, 0, 20).expect("text");
    assert!(mid.contains('ㄴ'), "preedit 삽입 확인: {mid}");
    doc.delete_text_logical(0, 0, 3, 1).expect("delete");
    let after = doc.get_text_range(0, 0, 0, 20).expect("text");
    assert_eq!(
        after, before,
        "논리 삽입·삭제 왕복은 원문 보존이어야 함 — 자모 잔류 회귀"
    );
}

/// 단폭 이내지만 앞 텍스트와 한 줄에 안 들어가는 광폭(140mm) 끝-앵커 TAC 표:
/// 표는 앞 텍스트 **다음 줄**로 내려간다(순서 보존). 종전 90% 폭 휴리스틱이
/// 블록 취급해 표가 앞 텍스트 위 줄로 올라갔다(2026-08-10 신고).
#[test]
fn wide_end_anchor_table_wraps_below_preceding_text() {
    let doc = load("officex_tac_wide_end_anchor.hwpx");
    let text_json = doc.get_cursor_rect(0, 0, 1).expect("가 뒤 캐럿(텍스트 줄)");
    let after_json = doc.get_cursor_rect(0, 0, 3).expect("표 뒤 캐럿");
    let y = |j: &str| json_usize(j, "y");
    assert!(
        y(&after_json) > y(&text_json) + 10,
        "표 뒤 캐럿은 앞 텍스트 **다음 줄**이어야(표가 위 줄로 올라가면 역전): \
         text={text_json} after={after_json}"
    );
}
