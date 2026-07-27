//! [officex/parity] 어울림 옆흐름 — 파일이 적어둔 줄 폭을 버리지 않는다.
//!
//! 오라클: `samples/pic2-2018.hwp` 안의 PrvImage(한글이 직접 렌더한 1쪽). 그림 오른쪽에
//! 놓인 문단들은 한컴에서 그림 왼쪽 경계에서 끊기는데, 종전 엔진은 앵커가 안 잡히면
//! 저장된 LINE_SEG(0+26140HU=348.5px)를 통째로 버리고 단 전폭(566.9px)으로 조판했다
//! → 양쪽정렬이 전폭으로 벌어지고 줄 꼬리가 그림 밑으로 파고들었다(2026-07-27 실측).
//!
//! ⚠ 이 수리는 **아무도 안 좁혔을 때만** 개입한다 — 다른 어울림 경로가 이미 폭을 정한
//! 문단(온새미로 35쪽)은 건드리지 않는다. 그 경계는 issue_1440 핀이 지킨다.
use rhwp::renderer::render_tree::{RenderNode, RenderNodeType};
use rhwp::wasm_api::HwpDocument;

fn line_widths(page: u32) -> Vec<(f64, f64)> {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/samples/pic2-2018.hwp"
    ))
    .expect("fixture");
    let doc = HwpDocument::from_bytes(&bytes).expect("parse");
    let tree = doc.build_page_render_tree(page).expect("tree");
    let mut out = Vec::new();
    collect(&tree.root, &mut out);
    out
}

fn collect(n: &RenderNode, out: &mut Vec<(f64, f64)>) {
    if matches!(n.node_type, RenderNodeType::TextLine(_)) {
        out.push((n.bbox.y, n.bbox.width));
    }
    for c in &n.children {
        collect(c, out);
    }
}

#[test]
fn text_beside_square_picture_uses_stored_segment_width() {
    // 그림 옆 구간의 줄들은 저장값 26140HU = 348.5px 여야 한다.
    // ⚠ y 하한 610 — 그 위(537~602)는 앞 문단(para7)의 줄로, 전폭이 정상이다.
    let narrow: Vec<f64> = line_widths(0)
        .into_iter()
        .filter(|(y, _)| *y > 610.0 && *y < 720.0)
        .map(|(_, w)| w)
        .collect();
    assert!(
        !narrow.is_empty(),
        "그림 옆 구간에서 줄을 못 찾음 — 픽스처/페이지 가정이 깨졌다"
    );
    let full = narrow.iter().filter(|w| **w > 500.0).count();
    assert_eq!(
        full, 0,
        "그림 옆 줄이 단 전폭으로 조판됨(폭 목록 {narrow:?}) — 저장된 LINE_SEG 를 버렸다"
    );
    for w in &narrow {
        assert!(
            (w - 348.5).abs() < 2.0,
            "그림 옆 줄 폭 {w:.1}px — 한컴 저장값 348.5px 와 어긋난다"
        );
    }
}
