//! Issue #1198: 중첩 표 셀 내부 클립보드 붙여넣기 대상 경로 보존.
//!
//! 재현 문서: `samples/exam_social.hwp`
//! 대상: 1쪽 상단 답안지 영역의 `성명` 오른쪽 빈 입력칸.

use std::path::Path;

use rhwp::renderer::render_tree::{RenderNode, RenderNodeType};
use rhwp::wasm_api::HwpDocument;
use serde_json::Value;

fn load_sample(name: &str) -> HwpDocument {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("samples")
        .join(name);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {}", path.display(), e));
    HwpDocument::from_bytes(&bytes).unwrap_or_else(|e| panic!("parse {name}: {e}"))
}

fn hit_json(doc: &HwpDocument, page: u32, x: f64, y: f64) -> Value {
    let json = doc
        .hit_test_native(page, x, y)
        .unwrap_or_else(|e| panic!("hit_test_native({page}, {x}, {y}): {e}"));
    serde_json::from_str(&json).unwrap_or_else(|e| panic!("parse hit json `{json}`: {e}"))
}

fn path_tuples(hit: &Value) -> Vec<(usize, usize, usize)> {
    hit["cellPath"]
        .as_array()
        .expect("cellPath array")
        .iter()
        .map(|entry| {
            (
                entry["controlIndex"].as_u64().expect("controlIndex") as usize,
                entry["cellIndex"].as_u64().expect("cellIndex") as usize,
                entry["cellParaIndex"].as_u64().expect("cellParaIndex") as usize,
            )
        })
        .collect()
}

/// `성명` 오른쪽 빈 입력칸(중첩 표) 내부의 한 점을 **렌더 트리에서** 구한다.
///
/// 종전엔 (250, 210) 을 하드코딩했는데, 그 y 는 입력칸 상단 경계에서 1.6px 밖이라
/// 정당한 배치 변경에도 깨졌다 — 2026-08-06 TAC 세로 배치식 정정(오라클 §2-C: 글리프
/// 상자가 기준선을 r:(1−r) 로 가른다)으로 입력칸이 바깥여백만큼(≈3.8px) 아래로
/// 내려가자 점이 칸 밖으로 나가 중첩 경로가 한 단계로 줄었다. 칸 **중심**을 쓰면
/// 배치가 정상 범위에서 움직여도 이 핀은 "중첩 셀 경로 보존"만 검사한다.
fn nested_input_cell_point(doc: &mut HwpDocument) -> (f64, f64) {
    const PROBE_X: f64 = 250.0; // 성명 입력칸이 걸치는 x
    const BAND: (f64, f64) = (180.0, 260.0); // 1쪽 상단 답안지 영역
    let tree = doc.build_page_render_tree(0).expect("page 0 render tree");
    // 가장 깊은(=중첩) Table 노드를 찾는다.
    let mut best: Option<(usize, f64, f64)> = None; // (depth, cy, cx)
    fn walk(n: &RenderNode, depth: usize, best: &mut Option<(usize, f64, f64)>) {
        if matches!(n.node_type, RenderNodeType::Table(_)) {
            let b = &n.bbox;
            let hits_x = b.x <= PROBE_X && PROBE_X <= b.x + b.width;
            let in_band = b.y >= BAND.0 && b.y + b.height <= BAND.1;
            if hits_x && in_band && best.map_or(true, |(d, ..)| depth > d) {
                *best = Some((depth, b.y + b.height / 2.0, PROBE_X));
            }
        }
        for c in &n.children {
            walk(c, depth + 1, best);
        }
    }
    walk(&tree.root, 0, &mut best);
    let (_, cy, cx) = best.expect("성명 입력칸(중첩 표)을 렌더 트리에서 찾지 못했다");
    (cx, cy)
}

fn first_copyable_char(doc: &HwpDocument) -> (u32, u32, String) {
    let para_count = doc
        .get_paragraph_count(0)
        .unwrap_or_else(|e| panic!("get_paragraph_count: {e:?}"));
    for para_idx in 0..para_count {
        let len = doc
            .get_paragraph_length(0, para_idx)
            .unwrap_or_else(|e| panic!("get_paragraph_length({para_idx}): {e:?}"));
        for offset in 0..len {
            let text = doc
                .get_text_range(0, para_idx, offset, 1)
                .unwrap_or_default();
            if text
                .chars()
                .any(|ch| !ch.is_whitespace() && !ch.is_control())
            {
                return (para_idx, offset, text);
            }
        }
    }
    panic!("copyable source character not found");
}

#[test]
fn issue_1198_exam_social_internal_paste_uses_nested_cell_path() {
    let mut doc = load_sample("exam_social.hwp");

    // 1쪽 상단 답안지 `성명` 오른쪽 빈 입력칸 내부 좌표(렌더 트리에서 중심을 잡는다).
    let (px, py) = nested_input_cell_point(&mut doc);
    let hit = hit_json(&doc, 0, px, py);
    let path = path_tuples(&hit);
    assert_eq!(
        path,
        vec![(4, 0, 3), (0, 1, 0)],
        "exam_social.hwp name field must remain a nested cell path: {hit}"
    );

    let path_json = serde_json::to_string(&hit["cellPath"]).expect("cellPath json");
    let char_offset = hit["charOffset"].as_u64().unwrap_or(0) as usize;
    let (source_para, source_offset, expected) = first_copyable_char(&doc);
    let expected_chars = expected.chars().count();

    doc.copy_selection(
        0,
        source_para,
        source_offset,
        source_para,
        source_offset + expected_chars as u32,
    )
    .unwrap_or_else(|e| panic!("copy_selection failed: {e:?}"));
    assert_eq!(doc.get_clipboard_text(), expected);

    let result_json = doc
        .paste_internal_in_cell_by_path(0, 0, &path_json, char_offset as u32)
        .unwrap_or_else(|e| panic!("paste_internal_in_cell_by_path failed: {e:?}"));
    let result: Value = serde_json::from_str(&result_json)
        .unwrap_or_else(|e| panic!("parse paste result `{result_json}`: {e}"));
    assert_eq!(result["ok"].as_bool(), Some(true), "{result_json}");
    assert_eq!(
        result["cellParaIdx"].as_u64(),
        Some(path[path.len() - 1].2 as u64),
        "{result_json}"
    );
    assert_eq!(
        result["charOffset"].as_u64(),
        Some((char_offset + expected_chars) as u64),
        "{result_json}"
    );

    let inserted = doc
        .get_text_in_cell_by_path(0, 0, &path, char_offset, expected_chars)
        .unwrap_or_else(|e| panic!("get_text_in_cell_by_path failed: {e}"));
    assert_eq!(inserted, expected);
}

#[test]
fn issue_1198_exam_social_html_paste_uses_nested_cell_path() {
    let mut doc = load_sample("exam_social.hwp");

    let (px, py) = nested_input_cell_point(&mut doc);
    let hit = hit_json(&doc, 0, px, py);
    let path = path_tuples(&hit);
    assert_eq!(
        path,
        vec![(4, 0, 3), (0, 1, 0)],
        "exam_social.hwp name field must remain a nested cell path: {hit}"
    );

    let path_json = serde_json::to_string(&hit["cellPath"]).expect("cellPath json");
    let char_offset = hit["charOffset"].as_u64().unwrap_or(0) as usize;
    let expected = "붙여넣기";
    let html = format!(
        "<html><body><!--StartFragment--><p>{}</p><!--EndFragment--></body></html>",
        expected
    );

    let result_json = doc
        .paste_html_in_cell_by_path(0, 0, &path_json, char_offset as u32, &html)
        .unwrap_or_else(|e| panic!("paste_html_in_cell_by_path failed: {e:?}"));
    let result: Value = serde_json::from_str(&result_json)
        .unwrap_or_else(|e| panic!("parse paste result `{result_json}`: {e}"));
    assert_eq!(result["ok"].as_bool(), Some(true), "{result_json}");
    assert_eq!(
        result["cellParaIdx"].as_u64(),
        Some(path[path.len() - 1].2 as u64),
        "{result_json}"
    );
    assert_eq!(
        result["charOffset"].as_u64(),
        Some((char_offset + expected.chars().count()) as u64),
        "{result_json}"
    );

    let inserted = doc
        .get_text_in_cell_by_path(0, 0, &path, char_offset, expected.chars().count())
        .unwrap_or_else(|e| panic!("get_text_in_cell_by_path failed: {e}"));
    assert_eq!(inserted, expected);
}
