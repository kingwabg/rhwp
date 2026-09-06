use serde::Serialize;
use std::collections::BTreeMap;

use crate::model::image::ImageEffect;
use crate::model::shape::TextWrap;
use crate::model::style::{ImageFillMode, UnderlineType};
use crate::paint::{
    paint_op_replay_plane_with_layer, CacheHint, ClipKind, LayerNode, LayerNodeKind, PageLayerTree,
    PaintOp, PaintReplayPlane, RenderProfile, ResolvedImageKind, ResolvedImagePayload,
    TextDecorationKind, TextVariantKind,
};
use crate::renderer::composer::expand_pua_display_text;
use crate::renderer::equation::{
    layout::{LayoutBox, LayoutKind},
    symbols::{DecoKind, FontStyleKind},
};
use crate::renderer::layer_renderer::{
    analyze_text_variant_selection, TextVariantSelectionOptions, VariantSelectedReason,
    VariantSelectionBackend,
};
use crate::renderer::render_tree::{
    FieldMarkerType, ImageNode, PageBackgroundNode, RenderLayerInfo, TextRunNode,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CanvasKitReplayMode {
    Default,
    Compat,
}

impl CanvasKitReplayMode {
    pub fn from_str(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "" | "default" => Some(Self::Default),
            "compat" | "compatibility" => Some(Self::Compat),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Compat => "compat",
        }
    }

    fn policy(self) -> CanvasKitReplayPolicy {
        match self {
            // P17 intentionally keeps both public modes on the same direct replay
            // contract. `compat` is still accepted for API/URL compatibility and
            // future conservative direct-replay tuning, but it must not mean a
            // hidden Canvas2D paint overlay.
            Self::Default | Self::Compat => CanvasKitReplayPolicy::DIRECT_ONLY,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CanvasKitReplayPolicy {
    hidden_canvas2d_overlay_allowed: bool,
    direct_replay_required: bool,
}

impl CanvasKitReplayPolicy {
    const DIRECT_ONLY: Self = Self {
        hidden_canvas2d_overlay_allowed: false,
        direct_replay_required: true,
    };
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasKitReplayPlan {
    pub mode: CanvasKitReplayMode,
    pub hidden_canvas2d_overlay_allowed: bool,
    pub direct_replay_required: bool,
    pub summary: CanvasKitReplaySummary,
    pub items: Vec<CanvasKitReplayItem>,
    pub text_variants: Vec<CanvasKitTextVariantReport>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasKitReplaySummary {
    pub total_items: u32,
    pub direct_items: u32,
    pub direct_required_items: u32,
    pub compat_overlay_items: u32,
    pub text_fallback_items: u32,
    pub unsupported_items: u32,
    pub hidden_overlay_violations: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasKitReplayItem {
    pub path: String,
    pub op_type: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replay_plane: Option<PaintReplayPlane>,
    pub feature: CanvasKitReplayFeature,
    pub status: CanvasKitReplayStatus,
    pub reason: CanvasKitReplayReason,
    pub compat_overlay_allowed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CanvasKitReplayFeature {
    PageBackground,
    VectorShape,
    RasterImage,
    Equation,
    FormObject,
    RawSvgFragment,
    Placeholder,
    TextRun,
    TextSpecialVisual,
    TextVariant,
    Clip,
    CacheHint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CanvasKitReplayStatus {
    Direct,
    DirectRequired,
    CompatOverlay,
    TextFallback,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CanvasKitReplayReason {
    DirectReplaySupported,
    DirectReplayRequired,
    CompatOverlayAllowed,
    HiddenOverlayForbidden,
    ExplicitTextRunFallback,
    UnsupportedFeature,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasKitTextVariantReport {
    pub equivalence_group: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_variant_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_variant_kind: Option<&'static str>,
    pub selected_reason: &'static str,
    pub fallback_required: bool,
    pub rejected_variants: Vec<CanvasKitRejectedTextVariant>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasKitRejectedTextVariant {
    pub variant_id: String,
    pub variant_kind: &'static str,
    pub reasons: Vec<&'static str>,
}

pub fn analyze_canvaskit_replay_plan(
    tree: &PageLayerTree,
    mode: CanvasKitReplayMode,
) -> CanvasKitReplayPlan {
    let variant_reports = analyze_text_variant_selection(
        tree,
        TextVariantSelectionOptions {
            backend: VariantSelectionBackend::CanvasKit,
            allow_colrv1_stage1_color_graph: true,
            allow_bitmap_glyph: true,
            allow_svg_glyph: true,
            ..TextVariantSelectionOptions::canvaskit()
        },
    );
    let selected_variants = variant_reports
        .iter()
        .filter_map(|report| {
            let variant_id = report.selected_variant_id.as_ref()?;
            let variant_kind = report.selected_variant_kind?;
            Some((
                report.equivalence_group.clone(),
                SelectedTextVariant {
                    variant_id: variant_id.clone(),
                    variant_kind,
                    fallback_required: report.fallback_required,
                },
            ))
        })
        .collect::<BTreeMap<_, _>>();
    let mut builder = CanvasKitReplayPlanBuilder::new(mode, tree.profile, selected_variants);
    builder.visit_node(&tree.root, "root", None);
    let text_variants = variant_reports
        .into_iter()
        .map(|report| CanvasKitTextVariantReport {
            equivalence_group: report.equivalence_group,
            selected_variant_id: report.selected_variant_id,
            selected_variant_kind: report.selected_variant_kind.map(TextVariantKind::as_str),
            selected_reason: selected_reason_as_str(report.selected_reason),
            fallback_required: report.fallback_required,
            rejected_variants: report
                .rejected_variants
                .into_iter()
                .map(|rejected| CanvasKitRejectedTextVariant {
                    variant_id: rejected.variant_id,
                    variant_kind: rejected.variant_kind.as_str(),
                    reasons: rejected
                        .reasons
                        .into_iter()
                        .map(|reason| reason.as_str())
                        .collect(),
                })
                .collect(),
        })
        .collect();
    builder.finish(text_variants)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SelectedTextVariant {
    variant_id: String,
    variant_kind: TextVariantKind,
    fallback_required: bool,
}

struct CanvasKitReplayPlanBuilder {
    mode: CanvasKitReplayMode,
    policy: CanvasKitReplayPolicy,
    profile: RenderProfile,
    selected_variants: BTreeMap<String, SelectedTextVariant>,
    summary: CanvasKitReplaySummary,
    items: Vec<CanvasKitReplayItem>,
}

impl CanvasKitReplayPlanBuilder {
    fn new(
        mode: CanvasKitReplayMode,
        profile: RenderProfile,
        selected_variants: BTreeMap<String, SelectedTextVariant>,
    ) -> Self {
        Self {
            mode,
            policy: mode.policy(),
            profile,
            selected_variants,
            summary: CanvasKitReplaySummary::default(),
            items: Vec::new(),
        }
    }

    fn finish(self, text_variants: Vec<CanvasKitTextVariantReport>) -> CanvasKitReplayPlan {
        CanvasKitReplayPlan {
            mode: self.mode,
            hidden_canvas2d_overlay_allowed: self.policy.hidden_canvas2d_overlay_allowed,
            direct_replay_required: self.policy.direct_replay_required,
            summary: self.summary,
            items: self.items,
            text_variants,
        }
    }

    fn visit_node(
        &mut self,
        node: &LayerNode,
        path: &str,
        inherited_layer: Option<RenderLayerInfo>,
    ) {
        let active_layer = node.layer.or(inherited_layer);
        match &node.kind {
            LayerNodeKind::Group {
                children,
                cache_hint,
                ..
            } => {
                if !matches!(cache_hint, CacheHint::None) {
                    self.push_cache_hint_item(path, *cache_hint);
                }
                for (index, child) in children.iter().enumerate() {
                    self.visit_node(child, &format!("{path}/group/{index}"), active_layer);
                }
            }
            LayerNodeKind::ClipRect {
                child, clip_kind, ..
            } => {
                self.push(CanvasKitReplayItem {
                    path: format!("{path}/clip"),
                    op_type: "clipRect",
                    replay_plane: None,
                    feature: CanvasKitReplayFeature::Clip,
                    status: CanvasKitReplayStatus::Direct,
                    reason: CanvasKitReplayReason::DirectReplaySupported,
                    compat_overlay_allowed: false,
                    detail: Some(clip_kind_detail(*clip_kind).to_string()),
                });
                self.visit_node(child, &format!("{path}/clip/child"), active_layer);
            }
            LayerNodeKind::Leaf { ops } => {
                for (index, op) in ops.iter().enumerate() {
                    self.push(self.item_for_op(op, format!("{path}/leaf/{index}"), active_layer));
                }
            }
        }
    }

    fn push_cache_hint_item(&mut self, path: &str, cache_hint: CacheHint) {
        let (status, reason, compat_overlay_allowed) =
            if self.policy.hidden_canvas2d_overlay_allowed {
                (
                    CanvasKitReplayStatus::CompatOverlay,
                    CanvasKitReplayReason::CompatOverlayAllowed,
                    true,
                )
            } else {
                (
                    CanvasKitReplayStatus::DirectRequired,
                    CanvasKitReplayReason::HiddenOverlayForbidden,
                    false,
                )
            };
        self.push(CanvasKitReplayItem {
            path: format!("{path}/cacheHint"),
            op_type: "cacheHint",
            replay_plane: None,
            feature: CanvasKitReplayFeature::CacheHint,
            status,
            reason,
            compat_overlay_allowed,
            detail: Some(format!("{cache_hint:?}")),
        });
    }

    fn item_for_op(
        &self,
        op: &PaintOp,
        path: String,
        layer: Option<RenderLayerInfo>,
    ) -> CanvasKitReplayItem {
        let mut item = match op {
            PaintOp::PageBackground { background, .. } => {
                self.page_background_item(path, background)
            }
            PaintOp::Line { .. }
            | PaintOp::Rectangle { .. }
            | PaintOp::Ellipse { .. }
            | PaintOp::Path { .. } => {
                direct_item(path, paint_op_type(op), CanvasKitReplayFeature::VectorShape)
            }
            PaintOp::FootnoteMarker { .. } => {
                let mut item = direct_item(
                    path,
                    paint_op_type(op),
                    CanvasKitReplayFeature::TextSpecialVisual,
                );
                item.detail = Some("footnoteMarker".to_string());
                item
            }
            PaintOp::Image {
                image, resolved, ..
            } => self.image_item(path, image, resolved.as_deref()),
            PaintOp::Equation { equation, .. }
                if canvaskit_equation_layout_is_supported(&equation.layout_box) =>
            {
                let mut item = direct_item(path, "equation", CanvasKitReplayFeature::Equation);
                item.detail = Some("boundedSemanticLayout".to_string());
                item
            }
            PaintOp::Equation { .. } => {
                let mut item = self.transition_overlay_item(
                    path,
                    "equation",
                    CanvasKitReplayFeature::Equation,
                );
                item.detail = Some("unsupportedSemanticLayout".to_string());
                item
            }
            PaintOp::FormObject { .. } => {
                let mut item = direct_item(path, "formObject", CanvasKitReplayFeature::FormObject);
                item.detail = Some("basicStaticReplay".to_string());
                item
            }
            PaintOp::RawSvg { .. } => {
                let mut item = self.transition_overlay_item(
                    path,
                    "rawSvg",
                    CanvasKitReplayFeature::RawSvgFragment,
                );
                item.detail = Some("unsupportedDirectReplay".to_string());
                item
            }
            PaintOp::Placeholder { placeholder, .. } => {
                let mut item =
                    direct_item(path, "placeholder", CanvasKitReplayFeature::Placeholder);
                item.detail = Some(match placeholder.kind {
                    crate::renderer::render_tree::PlaceholderKind::Ole => {
                        "basicStaticReplay".to_string()
                    }
                    crate::renderer::render_tree::PlaceholderKind::MissingPicture
                        if matches!(
                            self.profile,
                            RenderProfile::Print | RenderProfile::HighQuality
                        ) =>
                    {
                        "missingPictureSuppressedPrintEquivalent".to_string()
                    }
                    crate::renderer::render_tree::PlaceholderKind::MissingPicture => {
                        "missingPictureEditorVisual".to_string()
                    }
                });
                item
            }
            PaintOp::TextRun { run, .. } => self.text_run_item(path, run),
            PaintOp::CharOverlap { .. }
            | PaintOp::TextControlMark { .. }
            | PaintOp::TabLeader { .. }
            | PaintOp::TextDecoration { .. } => self.transition_overlay_item(
                path,
                paint_op_type(op),
                CanvasKitReplayFeature::TextSpecialVisual,
            ),
            PaintOp::GlyphRun { run, .. } => self.text_variant_item(
                path,
                "glyphRun",
                &run.variant.equivalence_group,
                &run.variant.variant_id,
                TextVariantKind::GlyphRun,
            ),
            PaintOp::GlyphOutline { outline, .. } => self.text_variant_item(
                path,
                "glyphOutline",
                &outline.variant.equivalence_group,
                &outline.variant.variant_id,
                TextVariantKind::GlyphOutline,
            ),
        };
        item.replay_plane = Some(paint_op_replay_plane_with_layer(op, layer));
        item
    }

    fn page_background_item(
        &self,
        path: String,
        background: &PageBackgroundNode,
    ) -> CanvasKitReplayItem {
        if background.image.is_some() {
            let mut item = self.transition_overlay_item(
                path,
                "pageBackground",
                CanvasKitReplayFeature::RasterImage,
            );
            item.detail = Some("imageFill".to_string());
            item
        } else if background.gradient.is_some() {
            let mut item = self.transition_overlay_item(
                path,
                "pageBackground",
                CanvasKitReplayFeature::PageBackground,
            );
            item.detail = Some("gradientFill".to_string());
            item
        } else {
            direct_item(
                path,
                "pageBackground",
                CanvasKitReplayFeature::PageBackground,
            )
        }
    }

    fn text_run_item(&self, path: String, run: &TextRunNode) -> CanvasKitReplayItem {
        if let Some(detail) = text_run_transition_detail(run) {
            let mut item =
                self.transition_overlay_item(path, "textRun", CanvasKitReplayFeature::TextRun);
            item.detail = Some(detail.to_string());
            item
        } else {
            direct_item(path, "textRun", CanvasKitReplayFeature::TextRun)
        }
    }

    fn image_item(
        &self,
        path: String,
        image: &ImageNode,
        resolved: Option<&ResolvedImagePayload>,
    ) -> CanvasKitReplayItem {
        let detail = image_transition_detail(image, resolved);
        if image_can_replay_directly(image, resolved) {
            let mut item = direct_item(path, "image", CanvasKitReplayFeature::RasterImage);
            item.detail = detail;
            item
        } else {
            let mut item =
                self.transition_overlay_item(path, "image", CanvasKitReplayFeature::RasterImage);
            item.detail = detail;
            item
        }
    }

    fn text_variant_item(
        &self,
        path: String,
        op_type: &'static str,
        equivalence_group: &str,
        variant_id: &str,
        variant_kind: TextVariantKind,
    ) -> CanvasKitReplayItem {
        let selected = self.selected_variants.get(equivalence_group);
        if selected.is_some_and(|selected| {
            !selected.fallback_required
                && selected.variant_id == variant_id
                && selected.variant_kind == variant_kind
        }) {
            return CanvasKitReplayItem {
                path,
                op_type,
                replay_plane: None,
                feature: CanvasKitReplayFeature::TextVariant,
                status: CanvasKitReplayStatus::DirectRequired,
                reason: CanvasKitReplayReason::DirectReplayRequired,
                compat_overlay_allowed: false,
                detail: Some(format!("selectedVariant={variant_id}")),
            };
        }
        CanvasKitReplayItem {
            path,
            op_type,
            replay_plane: None,
            feature: CanvasKitReplayFeature::TextVariant,
            status: CanvasKitReplayStatus::TextFallback,
            reason: CanvasKitReplayReason::ExplicitTextRunFallback,
            compat_overlay_allowed: false,
            detail: Some(format!("fallbackVariantGroup={equivalence_group}")),
        }
    }

    fn transition_overlay_item(
        &self,
        path: String,
        op_type: &'static str,
        feature: CanvasKitReplayFeature,
    ) -> CanvasKitReplayItem {
        if self.policy.hidden_canvas2d_overlay_allowed {
            CanvasKitReplayItem {
                path,
                op_type,
                replay_plane: None,
                feature,
                status: CanvasKitReplayStatus::CompatOverlay,
                reason: CanvasKitReplayReason::CompatOverlayAllowed,
                compat_overlay_allowed: true,
                detail: None,
            }
        } else {
            CanvasKitReplayItem {
                path,
                op_type,
                replay_plane: None,
                feature,
                status: CanvasKitReplayStatus::DirectRequired,
                reason: CanvasKitReplayReason::HiddenOverlayForbidden,
                compat_overlay_allowed: false,
                detail: None,
            }
        }
    }

    fn push(&mut self, item: CanvasKitReplayItem) {
        self.summary.total_items += 1;
        match item.status {
            CanvasKitReplayStatus::Direct => self.summary.direct_items += 1,
            CanvasKitReplayStatus::DirectRequired => self.summary.direct_required_items += 1,
            CanvasKitReplayStatus::CompatOverlay => self.summary.compat_overlay_items += 1,
            CanvasKitReplayStatus::TextFallback => self.summary.text_fallback_items += 1,
            CanvasKitReplayStatus::Unsupported => self.summary.unsupported_items += 1,
        }
        if matches!(item.reason, CanvasKitReplayReason::HiddenOverlayForbidden) {
            self.summary.hidden_overlay_violations += 1;
        }
        self.items.push(item);
    }
}

fn direct_item(
    path: String,
    op_type: &'static str,
    feature: CanvasKitReplayFeature,
) -> CanvasKitReplayItem {
    CanvasKitReplayItem {
        path,
        op_type,
        replay_plane: None,
        feature,
        status: CanvasKitReplayStatus::Direct,
        reason: CanvasKitReplayReason::DirectReplaySupported,
        compat_overlay_allowed: false,
        detail: None,
    }
}

fn canvaskit_equation_layout_is_supported(layout: &LayoutBox) -> bool {
    const MAX_DEPTH: usize = 64;
    const MAX_NODES: usize = 4096;
    const MAX_TEXT_UTF16_UNITS: usize = 4096;

    fn visit(
        layout: &LayoutBox,
        depth: usize,
        remaining_nodes: &mut usize,
        max_text_utf16_units: usize,
    ) -> bool {
        if depth > MAX_DEPTH
            || *remaining_nodes == 0
            || ![
                layout.x,
                layout.y,
                layout.width,
                layout.height,
                layout.baseline,
            ]
            .into_iter()
            .all(f64::is_finite)
            || layout.width < 0.0
            || layout.height < 0.0
        {
            return false;
        }
        *remaining_nodes -= 1;

        let text_supported =
            |text: &str| !text.is_empty() && text.encode_utf16().count() <= max_text_utf16_units;
        let child = |child: &LayoutBox, remaining_nodes: &mut usize| {
            visit(child, depth + 1, remaining_nodes, max_text_utf16_units)
        };
        match &layout.kind {
            LayoutKind::Row(children) => children.iter().all(|item| child(item, remaining_nodes)),
            LayoutKind::Text(text)
            | LayoutKind::Number(text)
            | LayoutKind::Symbol(text)
            | LayoutKind::MathSymbol(text)
            | LayoutKind::Function(text) => text_supported(text),
            LayoutKind::Fraction { numer, denom } => {
                child(numer, remaining_nodes) && child(denom, remaining_nodes)
            }
            LayoutKind::Atop { top, bottom } => {
                child(top, remaining_nodes) && child(bottom, remaining_nodes)
            }
            LayoutKind::Sqrt { index, body } => {
                index
                    .as_deref()
                    .is_none_or(|item| child(item, remaining_nodes))
                    && child(body, remaining_nodes)
            }
            LayoutKind::Superscript { base, sup } => {
                child(base, remaining_nodes) && child(sup, remaining_nodes)
            }
            LayoutKind::Subscript { base, sub } => {
                child(base, remaining_nodes) && child(sub, remaining_nodes)
            }
            LayoutKind::SubSup { base, sub, sup } => {
                child(base, remaining_nodes)
                    && child(sub, remaining_nodes)
                    && child(sup, remaining_nodes)
            }
            LayoutKind::BigOp { symbol, sub, sup } => {
                text_supported(symbol)
                    && sub
                        .as_deref()
                        .is_none_or(|item| child(item, remaining_nodes))
                    && sup
                        .as_deref()
                        .is_none_or(|item| child(item, remaining_nodes))
            }
            LayoutKind::Limit { sub, .. } => sub
                .as_deref()
                .is_none_or(|item| child(item, remaining_nodes)),
            LayoutKind::Matrix { cells, .. } => cells
                .iter()
                .flatten()
                .all(|item| child(item, remaining_nodes)),
            LayoutKind::Rel { arrow, over, under } => {
                child(over, remaining_nodes)
                    && child(arrow, remaining_nodes)
                    && under
                        .as_deref()
                        .is_none_or(|item| child(item, remaining_nodes))
            }
            LayoutKind::EqAlign { rows } => rows
                .iter()
                .all(|(left, right)| child(left, remaining_nodes) && child(right, remaining_nodes)),
            LayoutKind::Paren { left, right, body } => {
                (left.is_empty() || text_supported(left))
                    && (right.is_empty() || text_supported(right))
                    && child(body, remaining_nodes)
            }
            LayoutKind::Decoration { kind, body } => {
                matches!(
                    kind,
                    DecoKind::Hat
                        | DecoKind::Dot
                        | DecoKind::DDot
                        | DecoKind::Bar
                        | DecoKind::Vec
                        | DecoKind::Dyad
                        | DecoKind::Under
                        | DecoKind::Underline
                        | DecoKind::Overline
                        | DecoKind::StrikeThrough
                ) && child(body, remaining_nodes)
            }
            LayoutKind::FontStyle { style, body } => {
                matches!(
                    style,
                    FontStyleKind::Roman | FontStyleKind::Italic | FontStyleKind::Bold
                ) && child(body, remaining_nodes)
            }
            LayoutKind::Space(width) => width.is_finite(),
            LayoutKind::Newline | LayoutKind::Empty => true,
        }
    }

    let mut remaining_nodes = MAX_NODES;
    visit(layout, 0, &mut remaining_nodes, MAX_TEXT_UTF16_UNITS)
}

fn paint_op_type(op: &PaintOp) -> &'static str {
    match op {
        PaintOp::PageBackground { .. } => "pageBackground",
        PaintOp::TextRun { .. } => "textRun",
        PaintOp::GlyphRun { .. } => "glyphRun",
        PaintOp::GlyphOutline { .. } => "glyphOutline",
        PaintOp::CharOverlap { .. } => "charOverlap",
        PaintOp::TextControlMark { .. } => "textControlMark",
        PaintOp::TabLeader { .. } => "tabLeader",
        PaintOp::TextDecoration {
            kind: TextDecorationKind::Underline,
            ..
        } => "underline",
        PaintOp::TextDecoration {
            kind: TextDecorationKind::Strikethrough,
            ..
        } => "strikethrough",
        PaintOp::TextDecoration {
            kind: TextDecorationKind::EmphasisDot,
            ..
        } => "emphasisDot",
        PaintOp::FootnoteMarker { .. } => "footnoteMarker",
        PaintOp::Line { .. } => "line",
        PaintOp::Rectangle { .. } => "rectangle",
        PaintOp::Ellipse { .. } => "ellipse",
        PaintOp::Path { .. } => "path",
        PaintOp::Image { .. } => "image",
        PaintOp::Equation { .. } => "equation",
        PaintOp::FormObject { .. } => "formObject",
        PaintOp::Placeholder { .. } => "placeholder",
        PaintOp::RawSvg { .. } => "rawSvg",
    }
}

fn clip_kind_detail(clip_kind: ClipKind) -> &'static str {
    match clip_kind {
        ClipKind::Body => "body",
        ClipKind::TableCell => "tableCell",
        ClipKind::TextBox => "textBox",
        ClipKind::Generic => "generic",
    }
}

fn text_run_transition_detail(run: &TextRunNode) -> Option<&'static str> {
    if run.is_vertical {
        return Some("verticalText");
    }
    if run.char_overlap.is_some() {
        return Some("charOverlap");
    }
    if run.field_marker != FieldMarkerType::None || run.is_para_end || run.is_line_break_end {
        return Some("controlMark");
    }
    if !run.style.tab_leaders.is_empty() {
        return Some("tabLeader");
    }
    if !matches!(run.style.underline, UnderlineType::None) || run.style.strikethrough {
        return Some("textDecoration");
    }
    if run.style.emphasis_dot != 0 {
        return Some("emphasisDot");
    }
    if run.style.outline_type != 0 {
        return Some("outlineTextEffect");
    }
    if run.style.shadow_type != 0 {
        return Some("shadowTextEffect");
    }
    if run.style.emboss {
        return Some("embossTextEffect");
    }
    if run.style.engrave {
        return Some("engraveTextEffect");
    }
    if run.style.shade_color != 0x00FF_FFFF {
        return Some("shadeTextEffect");
    }
    if (run.style.ratio - 1.0).abs() > f64::EPSILON {
        return Some("ratioTextEffect");
    }
    if run.style.superscript || run.style.subscript {
        let display_text = expand_pua_display_text(&run.text);
        if !display_text
            .bytes()
            .all(|byte| (0x20..=0x7e).contains(&byte))
        {
            return Some("scriptTextRequiresShaping");
        }
    }
    None
}

fn image_transition_detail(
    image: &ImageNode,
    resolved: Option<&ResolvedImagePayload>,
) -> Option<String> {
    let mut detail = Vec::new();
    let has_replayable_payload = image_has_replayable_payload(image, resolved);
    if let Some(payload) = resolved {
        detail.push(format!(
            "resolved={}",
            resolved_image_kind_detail(payload.kind)
        ));
    }
    if image.external_path.is_some() {
        detail.push("externalImage".to_string());
        if has_replayable_payload {
            detail.push("injectedImageData".to_string());
        } else {
            detail.push("missingImageData".to_string());
        }
    } else if !has_replayable_payload {
        detail.push("missingImageData".to_string());
    }
    if let Some(fill_mode) = image.fill_mode {
        detail.push(format!("fillMode={}", image_fill_mode_detail(fill_mode)));
    }
    if image.crop.is_some() {
        detail.push("crop".to_string());
    }
    let effects_are_baked = resolved.is_some_and(|payload| payload.suppress_effects);
    if !effects_are_baked && !matches!(image.effect, ImageEffect::RealPic) {
        detail.push(format!("effect={}", image_effect_detail(image.effect)));
    }
    if !effects_are_baked && (image.brightness != 0 || image.contrast != 0) {
        detail.push(format!(
            "adjustment=brightness:{},contrast:{}",
            image.brightness, image.contrast
        ));
    }
    if let Some(wrap) = image.text_wrap {
        detail.push(format!("wrap={}", text_wrap_detail(wrap)));
    }
    if image.transform.has_transform() {
        detail.push("transform".to_string());
    }
    if image.header_footer_ref.is_some() {
        detail.push("headerFooterImage".to_string());
    }
    if detail.is_empty() {
        None
    } else {
        Some(detail.join(";"))
    }
}

fn image_can_replay_directly(image: &ImageNode, resolved: Option<&ResolvedImagePayload>) -> bool {
    let has_replayable_payload = image_has_replayable_payload(image, resolved);
    let effects_are_supported = resolved.is_some_and(|payload| payload.suppress_effects)
        || (matches!(image.effect, ImageEffect::RealPic)
            && image.brightness == 0
            && image.contrast == 0);
    has_replayable_payload && effects_are_supported
}

fn image_has_replayable_payload(
    image: &ImageNode,
    resolved: Option<&ResolvedImagePayload>,
) -> bool {
    resolved.is_some() || image.data.as_ref().is_some_and(|data| !data.is_empty())
}

fn resolved_image_kind_detail(value: ResolvedImageKind) -> &'static str {
    match value {
        ResolvedImageKind::FormatConverted => "formatConverted",
        ResolvedImageKind::BakedWatermark => "bakedWatermark",
    }
}

fn image_effect_detail(value: ImageEffect) -> &'static str {
    match value {
        ImageEffect::RealPic => "realPic",
        ImageEffect::GrayScale => "grayScale",
        ImageEffect::BlackWhite => "blackWhite",
        ImageEffect::Pattern8x8 => "pattern8x8",
    }
}

fn image_fill_mode_detail(value: ImageFillMode) -> &'static str {
    match value {
        ImageFillMode::TileAll => "tileAll",
        ImageFillMode::TileHorzTop => "tileHorzTop",
        ImageFillMode::TileHorzBottom => "tileHorzBottom",
        ImageFillMode::TileVertLeft => "tileVertLeft",
        ImageFillMode::TileVertRight => "tileVertRight",
        ImageFillMode::FitToSize => "fitToSize",
        ImageFillMode::Total => "total",
        ImageFillMode::Center => "center",
        ImageFillMode::CenterTop => "centerTop",
        ImageFillMode::CenterBottom => "centerBottom",
        ImageFillMode::LeftCenter => "leftCenter",
        ImageFillMode::LeftTop => "leftTop",
        ImageFillMode::LeftBottom => "leftBottom",
        ImageFillMode::RightCenter => "rightCenter",
        ImageFillMode::RightTop => "rightTop",
        ImageFillMode::RightBottom => "rightBottom",
        ImageFillMode::None => "none",
    }
}

fn text_wrap_detail(value: TextWrap) -> &'static str {
    match value {
        TextWrap::Square => "square",
        TextWrap::Tight => "tight",
        TextWrap::Through => "through",
        TextWrap::TopAndBottom => "topAndBottom",
        TextWrap::BehindText => "behindText",
        TextWrap::InFrontOfText => "inFrontOfText",
    }
}

fn selected_reason_as_str(reason: VariantSelectedReason) -> &'static str {
    reason.as_str()
}
