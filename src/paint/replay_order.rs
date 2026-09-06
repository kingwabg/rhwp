use serde::Serialize;

use crate::model::shape::TextWrap;
use crate::paint::layer_tree::{LayerNode, LayerNodeKind};
use crate::paint::paint_op::PaintOp;
use crate::renderer::render_tree::RenderLayerInfo;

/// Logical replay planes for PageLayerTree direct paint backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PaintReplayPlane {
    Background,
    BehindText,
    Flow,
    InFrontOfText,
}

impl PaintReplayPlane {
    pub const ORDERED: [Self; 4] = [
        Self::Background,
        Self::BehindText,
        Self::Flow,
        Self::InFrontOfText,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Background => "background",
            Self::BehindText => "behindText",
            Self::Flow => "flow",
            Self::InFrontOfText => "inFrontOfText",
        }
    }
}

pub fn paint_op_replay_plane(op: &PaintOp) -> PaintReplayPlane {
    paint_op_replay_plane_with_layer(op, None)
}

pub fn paint_op_replay_plane_with_layer(
    op: &PaintOp,
    layer: Option<RenderLayerInfo>,
) -> PaintReplayPlane {
    if matches!(op, PaintOp::PageBackground { .. }) {
        return PaintReplayPlane::Background;
    }
    if layer.and_then(|layer| layer.text_wrap).is_some() {
        return render_layer_replay_plane(layer);
    }

    let plane = match op {
        PaintOp::Image { image, .. } => match image.text_wrap {
            Some(TextWrap::BehindText) => PaintReplayPlane::BehindText,
            Some(TextWrap::InFrontOfText) => PaintReplayPlane::InFrontOfText,
            _ => PaintReplayPlane::Flow,
        },
        _ => PaintReplayPlane::Flow,
    };
    cap_master_page_plane(plane, layer)
}

pub fn render_layer_replay_plane(layer: Option<RenderLayerInfo>) -> PaintReplayPlane {
    let plane = match layer.and_then(|layer| layer.text_wrap) {
        Some(TextWrap::BehindText) => PaintReplayPlane::BehindText,
        Some(TextWrap::InFrontOfText) => PaintReplayPlane::InFrontOfText,
        _ => PaintReplayPlane::Flow,
    };
    cap_master_page_plane(plane, layer)
}

/// 바탕쪽 유래 op 의 replay plane 상한 (#2318).
///
/// 한컴 의미론: 바탕쪽 개체의 text_wrap 은 바탕쪽 **내부** 개체 간 순서에만
/// 적용되고, 바탕쪽 전체는 항상 본문 뒤에 깔린다. SVG 의 `node_z_plane` 계약
/// (페이지 배경 → 바탕쪽 → BehindText → Flow → InFrontOfText, #1167)과 동일
/// 의미를 plane 재생 backend(web_canvas/skia/canvaskit)에 적용한다.
/// BehindText plane 내에서 바탕쪽 그룹은 트리 순서상 본문 개체보다 먼저
/// 재생되므로 더 깊게 깔린다.
fn cap_master_page_plane(
    plane: PaintReplayPlane,
    layer: Option<RenderLayerInfo>,
) -> PaintReplayPlane {
    if plane != PaintReplayPlane::Background && layer.is_some_and(|layer| layer.master_page) {
        PaintReplayPlane::BehindText
    } else {
        plane
    }
}

pub(crate) fn layer_node_has_replay_plane(node: &LayerNode, target: PaintReplayPlane) -> bool {
    layer_node_has_replay_plane_with_layer(node, target, None)
}

fn layer_node_has_replay_plane_with_layer(
    node: &LayerNode,
    target: PaintReplayPlane,
    inherited_layer: Option<RenderLayerInfo>,
) -> bool {
    let active_layer = node.layer.or(inherited_layer);
    match &node.kind {
        LayerNodeKind::Group { children, .. } => children
            .iter()
            .any(|child| layer_node_has_replay_plane_with_layer(child, target, active_layer)),
        LayerNodeKind::ClipRect { child, .. } => {
            layer_node_has_replay_plane_with_layer(child, target, active_layer)
        }
        LayerNodeKind::Leaf { ops } => ops
            .iter()
            .any(|op| paint_op_replay_plane_with_layer(op, active_layer) == target),
    }
}
