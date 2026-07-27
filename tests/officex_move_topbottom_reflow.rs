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
