//! 통합 테스트 공용 헬퍼.

use rhwp::renderer::render_tree::{RenderNode, RenderNodeType};
use rhwp::wasm_api::HwpDocument;

/// 답안지 `성명` 오른쪽 빈 입력칸(중첩 표) 내부의 한 점을 **렌더 트리에서** 구한다.
///
/// 하드코딩 좌표를 쓰면 안 되는 이유: 종전 핀들은 (250, 210) 을 박아 뒀는데 그 y 는
/// 입력칸 상단 경계에서 1.6px 밖이었다. 2026-08-06 TAC 세로 배치식 정정(오라클 §2-C:
/// 글리프 상자가 기준선을 r:(1−r) 로 가른다)으로 입력칸이 바깥여백만큼(≈3.8px) 아래로
/// 내려가자 그 점이 칸 밖으로 나가 중첩 셀 경로가 한 단계로 줄었다 — 배치는 맞고
/// 좌표가 낡은 것이었다. 칸 **중심**을 쓰면 배치가 정상 범위에서 움직여도 핀이
/// 검사하려는 것("중첩 셀 경로 보존")만 검사한다.
///
/// `exam_social.hwp` / `exam_science.hwp` 1쪽 상단 답안지 영역 기준.
pub fn nested_input_cell_point(doc: &mut HwpDocument) -> (f64, f64) {
    const PROBE_X: f64 = 250.0; // 성명 입력칸이 걸치는 x
    const BAND: (f64, f64) = (180.0, 260.0); // 1쪽 상단 답안지 영역

    let tree = doc.build_page_render_tree(0).expect("page 0 render tree");
    // x 를 지나고 밴드 안에 온전히 들어가는 Table 노드 중 **가장 깊은**(=중첩) 것.
    let mut best: Option<(usize, f64)> = None; // (depth, center_y)
    fn walk(n: &RenderNode, depth: usize, best: &mut Option<(usize, f64)>) {
        if matches!(n.node_type, RenderNodeType::Table(_)) {
            let b = &n.bbox;
            let hits_x = b.x <= PROBE_X && PROBE_X <= b.x + b.width;
            let in_band = b.y >= BAND.0 && b.y + b.height <= BAND.1;
            if hits_x && in_band && best.map_or(true, |(d, _)| depth > d) {
                *best = Some((depth, b.y + b.height / 2.0));
            }
        }
        for c in &n.children {
            walk(c, depth + 1, best);
        }
    }
    walk(&tree.root, 0, &mut best);
    let (_, center_y) = best.expect("성명 입력칸(중첩 표)을 렌더 트리에서 찾지 못했다");
    (PROBE_X, center_y)
}
