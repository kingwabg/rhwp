//! render-position 회귀 가드 — 저장된 위치 제한/좌표가 렌더에 반영되는지.
//!
//! 1) restrictInPage(flowWithText, HWP5 attr bit13)=on 인 **Paper 앵커** floating
//!    그림 + 페이지 높이를 넘는 vertOffset 이 쪽 밖(y≈쪽 높이의 2배)으로 나가
//!    완전 소실되던 결함(image-shape QA "restrictInPage 가 렌더 위치를 가두는가").
//!    issue_2032 는 vert=Para 만 덮었고 삽입 기본 앵커(Paper)는 클램프 밖이었다.
//! 2) 머리말 문단을 머리말 띠보다 많이 늘리면 4번째부터 좌표가 첫 줄 위로 되감겨
//!    겹치던 결함(page-section QA "머리말 문단을 늘리면 좌표가 아래로 쌓이는가").
//!    렌더 loop 이 띠 하단에서 break 해 넘친 문단이 tree 에 안 실려 cursor 폴백이
//!    영역 top 을 돌려주던 것이 원인.
use rhwp::document_core::DocumentCore;
use rhwp::renderer::render_tree::{RenderNode, RenderNodeType};

const TINY_PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x62, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
    0x42, 0x60, 0x82,
];

fn load() -> DocumentCore {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("samples/basic/english.hwp");
    DocumentCore::from_bytes(&std::fs::read(&p).unwrap()).unwrap()
}

fn collect(node: &RenderNode, out: &mut Vec<(f64, f64)>) {
    if let RenderNodeType::Image(_) = &node.node_type {
        out.push((node.bbox.y, node.bbox.height));
    }
    for c in &node.children {
        collect(c, out);
    }
}

#[test]
fn restrict_in_page_paper_anchor_clamped() {
    let mut core = load();
    let pi = core.document().sections[0]
        .paragraphs
        .iter()
        .position(|p| !p.text.is_empty())
        .unwrap();
    // insert_picture_native 본문 기본 = Paper 앵커, floating.
    let r = core
        .insert_picture_native(0, pi, 0, &[], TINY_PNG, 4000, 3000, 4, 3, "png", "", None, None)
        .unwrap();
    let ci = serde_json::from_str::<serde_json::Value>(&r).unwrap()["controlIdx"]
        .as_u64()
        .unwrap() as usize;
    // QA 그대로: vertOffset=200000 + restrictInPage=true (vertRelTo 미지정 → Paper 유지)
    core.set_picture_properties_native(0, pi, ci, r#"{"vertOffset":200000,"restrictInPage":true}"#)
        .unwrap();
    let pages = core.page_count();
    let mut found = None;
    for pg in 0..pages {
        let tree = core.build_page_render_tree(pg).unwrap();
        let mut imgs = Vec::new();
        collect(&tree.root, &mut imgs);
        if let Some(&(y, h)) = imgs.first() {
            found = Some((y, h, tree.root.bbox.height));
            break;
        }
    }
    let (y, h, page_h) = found.expect("이미지가 어느 페이지에도 없음(완전 소실)");
    println!("PAPER-CLAMP y={y:.1} h={h:.1} bottom={:.1} page_h={page_h:.1}", y + h);
    assert!(
        y + h <= page_h + 0.5,
        "restrictInPage=on Paper 앵커 그림이 쪽 밖: bottom={:.1} page_h={page_h:.1}",
        y + h
    );
}

#[test]
fn header_paragraphs_monotonic_y() {
    let mut core = load();
    core.create_header_footer_native(0, true, 0).unwrap();
    core.insert_text_in_header_footer_native(0, true, 0, 0, 0, "머리말1")
        .unwrap();
    for i in 0..4 {
        let info = core.get_header_footer_para_info_native(0, true, 0, i).unwrap();
        let cc = serde_json::from_str::<serde_json::Value>(&info).unwrap()["charCount"]
            .as_u64()
            .unwrap() as usize;
        core.split_paragraph_in_header_footer_native(0, true, 0, i, cc)
            .unwrap();
        core.insert_text_in_header_footer_native(0, true, 0, i + 1, 0, &format!("줄{}", i + 2))
            .unwrap();
    }
    let ys: Vec<f64> = (0..5)
        .map(|p| {
            let r = core
                .get_cursor_rect_in_header_footer_native(0, true, 0, p, 0, -1)
                .unwrap();
            serde_json::from_str::<serde_json::Value>(&r).unwrap()["y"]
                .as_f64()
                .unwrap()
        })
        .collect();
    println!("HEADER ys = {ys:?}");
    for i in 1..ys.len() {
        assert!(
            ys[i] > ys[i - 1],
            "머리말 문단 y 단조 증가 실패: {ys:?} (index {i})"
        );
    }
}
