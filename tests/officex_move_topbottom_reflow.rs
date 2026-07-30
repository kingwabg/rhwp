//! [편집/자리차지] 표를 아래로 이동하면 본문이 표 위 공간을 채우고 상자를 건너뛴다.
//!
//! 사용자 실증(2026-07-27, studio 실기동): 자리차지 표를 끌어도 본문이 전혀 재배치되지
//! 않아 표가 "채움 08~10" 글자 위에 그대로 겹쳐 그려졌다(옛 자리엔 공백). 원인은 조판
//! 의미론: 빈 host + TopAndBottom(Para) + voff>0 케이스가 advance=표높이·상자만 아래로
//! 라는 절충으로 남아 있었다. 한컴 정본(도움말): 자리차지 = "개체 높이만큼 줄을 차지,
//! 그 영역엔 본문이 못 온다" — 상자의 **실제 위치**가 배타 영역이다.
use rhwp::renderer::render_tree::{RenderNode, RenderNodeType};
use rhwp::wasm_api::HwpDocument;

fn lines_and_table(move_v: i32) -> (Vec<(usize, i32)>, (i32, i32)) {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    let filler = |n: usize| format!("채움 {n:02} 가나다라마바사 아자차카타파하 강물이 흐르고 산이 높다");
    doc.insert_text(0, 0, 0, &filler(14)).unwrap();
    for n in (1..14).rev() {
        doc.insert_paragraph(0, 0).unwrap();
        doc.insert_text(0, 0, 0, &filler(n)).unwrap();
    }
    let c: serde_json::Value = serde_json::from_str(&doc.create_table_ex(
        r#"{"sectionIdx":0,"paraIdx":7,"charOffset":0,"rowCount":3,"colCount":2,"treatAsChar":false}"#
    ).unwrap()).unwrap();
    let (pi, ci) = (c["paraIdx"].as_u64().unwrap() as u32, c["controlIdx"].as_u64().unwrap() as u32);
    doc.set_table_properties(0, pi, ci,
        r#"{"treatAsChar":false,"textWrap":"TopAndBottom","vertRelTo":"Para","horzRelTo":"Column","vertOffset":0}"#
    ).unwrap();
    if move_v != 0 {
        doc.move_table_offset(0, pi, ci, 0, move_v).unwrap();
    }
    let tree = doc.build_page_render_tree(0).unwrap();
    let mut lines = Vec::new();
    let mut tbl = (0, 0);
    fn walk(n: &RenderNode, lines: &mut Vec<(usize, i32)>, tbl: &mut (i32, i32)) {
        if let RenderNodeType::TextLine(tl) = &n.node_type {
            if tl.line_index == Some(0) {
                if let Some(p) = tl.para_index {
                    lines.push((p, n.bbox.y as i32));
                }
            }
        }
        if matches!(n.node_type, RenderNodeType::Table(_)) {
            *tbl = (n.bbox.y as i32, (n.bbox.y + n.bbox.height) as i32);
            return; // 셀 내부 줄은 본문이 아니다 — 상자 안이 정상이므로 순회 제외
        }
        for c in &n.children {
            walk(c, lines, tbl);
        }
    }
    walk(&tree.root, &mut lines, &mut tbl);
    (lines, tbl)
}

#[test]
fn moving_topbottom_table_down_reflows_following_text() {
    let (lines, (tbl_top, tbl_bottom)) = lines_and_table(4800); // 64px 아래로
    let y = |p: usize| lines.iter().find(|l| l.0 == p).map(|l| l.1).unwrap();
    // ① 겹침 금지: 어떤 본문 첫 줄도 표 상자 세로 구간 안에서 시작하지 않는다.
    for (p, ly) in &lines {
        assert!(
            *ly + 2 < tbl_top || *ly >= tbl_bottom,
            "p{p} 줄(y={ly})이 표 상자({tbl_top}~{tbl_bottom}) 안에서 시작 — 자리차지 위반"
        );
    }
    // ② 위 공간 채움: 표 바로 뒤 문단(p8)이 표 상자 위에서 시작한다(종전엔 상자 안 332).
    assert!(
        y(8) < tbl_top,
        "p8(y={}) 이 표 위 공간을 채우지 않음(표 top={tbl_top})",
        y(8)
    );
    // ③ 건너뛰기: 상자와 겹칠 차례였던 문단은 상자 아래로 밀린다.
    assert!(
        lines.iter().any(|(p, ly)| *p >= 9 && *ly >= tbl_bottom),
        "표 아래로 이어지는 문단이 없음 — 상자 건너뛰기 실패"
    );
    // ④ 이동량 자체는 소비된다: 표 top 이 이동 전(≈281)보다 아래.
    assert!(tbl_top > 320, "표 top={tbl_top} — vertOffset 이 소비되지 않음");
}

/// [자리차지 이동 2026-07-30] 사용자 신고 ①② — 빈 문서에서 표를 만들어(빈 host 앵커)
/// 아래로 이동하면 앵커 문단 캐럿이 표 좌상단으로 끌려갔다. 한컴 오라클(웹한글 실측):
/// 앵커 조판부호·커서는 본문 흐름 위치에 남고 표 상자만 오프셋 위치에 그려진다.
#[test]
fn moving_empty_host_table_down_keeps_anchor_cursor_in_flow() {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    let c: serde_json::Value = serde_json::from_str(&doc.create_table_ex(
        r#"{"sectionIdx":0,"paraIdx":0,"charOffset":0,"rowCount":2,"colCount":2,"treatAsChar":false}"#
    ).unwrap()).unwrap();
    let (pi, ci) = (c["paraIdx"].as_u64().unwrap() as u32, c["controlIdx"].as_u64().unwrap() as u32);
    doc.set_table_properties(0, pi, ci,
        r#"{"treatAsChar":false,"textWrap":"TopAndBottom","vertRelTo":"Para","horzRelTo":"Column","vertOffset":0}"#
    ).unwrap();

    let rect_y = |doc: &HwpDocument| -> f64 {
        let r: serde_json::Value =
            serde_json::from_str(&doc.get_cursor_rect(0, pi, 0).unwrap()).unwrap();
        r["y"].as_f64().unwrap()
    };
    let y_before = rect_y(&doc);

    doc.move_table_offset(0, pi, ci, 0, 6000).unwrap();

    // ① 이동 전후 앵커 캐럿 y 불변(±2px) — 흐름 위치 보존.
    let y_after = rect_y(&doc);
    assert!(
        (y_after - y_before).abs() <= 2.0,
        "앵커 캐럿이 흐름을 떠남: 이동 전 y={y_before}, 후 y={y_after}"
    );

    // ② 캐럿은 표 상자 위에 있다(표 좌상단에 붙지 않는다).
    let tree = doc.build_page_render_tree(0).unwrap();
    fn table_top(n: &RenderNode) -> Option<f64> {
        if matches!(n.node_type, RenderNodeType::Table(_)) {
            return Some(n.bbox.y);
        }
        n.children.iter().find_map(table_top)
    }
    let tbl_top = table_top(&tree.root).expect("표 노드 없음");
    assert!(
        y_after < tbl_top - 2.0,
        "앵커 캐럿(y={y_after})이 표 상자(top={tbl_top}) 위에 있지 않음"
    );
}

/// [한컴 O6 판정식 2026-07-30] 이동된 자리차지 앵커 문단은 **자기 줄을 차지**하고
/// 뒤 문단은 그 아래 줄에서 시작한다(웹한글 라벨 실험: `AAA↵` / `[표]BBB↵` / 표 상자).
/// 종전 결함은 앵커 캐럿이 표 상자로 끌려가 뒤 문단과 순서가 뒤집힌 것 — marker_y 수리로
/// 충족됐고, 이 테스트가 회귀를 막는다.
#[test]
fn moved_anchor_paragraph_keeps_its_own_line_above_following_text() {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    let c: serde_json::Value = serde_json::from_str(&doc.create_table_ex(
        r#"{"sectionIdx":0,"paraIdx":0,"charOffset":0,"rowCount":2,"colCount":2,"treatAsChar":false}"#
    ).unwrap()).unwrap();
    let (pi, ci) = (c["paraIdx"].as_u64().unwrap() as u32, c["controlIdx"].as_u64().unwrap() as u32);
    doc.set_table_properties(0, pi, ci,
        r#"{"treatAsChar":false,"textWrap":"TopAndBottom","vertRelTo":"Para","horzRelTo":"Column","vertOffset":0}"#
    ).unwrap();
    let pc = doc.get_paragraph_count(0).unwrap();
    let last = pc - 1;
    doc.insert_text(0, last, 0, "BBB").unwrap();
    doc.move_table_offset(0, pi, ci, 0, 6000).unwrap();

    let rect = |p: u32| -> (f64, f64) {
        let r: serde_json::Value =
            serde_json::from_str(&doc.get_cursor_rect(0, p, 0).unwrap()).unwrap();
        (r["y"].as_f64().unwrap(), r["height"].as_f64().unwrap())
    };
    let (anchor_y, anchor_h) = rect(pi);
    let (next_y, _) = rect(pi + 1);

    // ① 문서 순서 = 화면 순서: 앵커 줄이 뒤 문단보다 위.
    assert!(
        anchor_y < next_y,
        "앵커 줄(y={anchor_y})이 뒤 문단(y={next_y})보다 아래 — 순서 역전"
    );
    // ② 앵커가 자기 줄을 차지한다: 뒤 문단이 앵커 줄 높이만큼 아래에서 시작(겹침 금지).
    assert!(
        next_y >= anchor_y + anchor_h - 1.0,
        "뒤 문단(y={next_y})이 앵커 줄(y={anchor_y} h={anchor_h}) 안에서 시작 — 겹침"
    );
    // ③ 두 줄 모두 표 상자 위에 있다(표만 오프셋 위치).
    let tree = doc.build_page_render_tree(0).unwrap();
    fn table_top(n: &RenderNode) -> Option<f64> {
        if matches!(n.node_type, RenderNodeType::Table(_)) {
            return Some(n.bbox.y);
        }
        n.children.iter().find_map(table_top)
    }
    let tbl = table_top(&tree.root).expect("표 노드 없음");
    assert!(
        next_y < tbl,
        "뒤 문단(y={next_y})이 표 상자(top={tbl}) 아래 — 흐름이 표를 못 건너뜀"
    );
}
