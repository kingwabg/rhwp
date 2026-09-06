use crate::paint::{
    FontPortabilityKind, GlyphCluster, GlyphRunDiagnostics, GlyphRunOrientation,
    GlyphRunReplayEligibility, LayerAffineTransform, LayerGlyphRunPaint, LayerNode, LayerNodeKind,
    LayerPoint, LayerVector, PaintOp, PaintTextStyle, ShapeKey, TextRunPlacement, TextVariantKind,
    TextVariantQuality,
};
use crate::renderer::render_tree::{BoundingBox, TextRunNode};
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontRequest {
    pub family: String,
    pub bold: bool,
    pub italic: bool,
}

impl From<&TextRunNode> for FontRequest {
    fn from(run: &TextRunNode) -> Self {
        Self {
            family: run.style.font_family.clone(),
            bold: run.style.bold,
            italic: run.style.italic,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedFontFace {
    pub portability: FontPortabilityKind,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedGlyphRun {
    pub shape_key: ShapeKey,
    pub glyph_ids: Vec<u32>,
    pub positions: Vec<LayerPoint>,
    pub advances: Option<Vec<LayerVector>>,
    pub clusters: Vec<GlyphCluster>,
    pub diagnostics: GlyphRunDiagnostics,
}

pub trait FontResolver {
    fn resolve_font(&self, request: &FontRequest) -> ResolvedFontFace;

    fn shape_glyph_run(
        &self,
        _request: &FontRequest,
        _run: &TextRunNode,
        _resolved: &ResolvedFontFace,
    ) -> Option<ResolvedGlyphRun> {
        None
    }
}

#[derive(Debug, Default)]
pub struct NoopFontResolver;

impl FontResolver for NoopFontResolver {
    fn resolve_font(&self, _request: &FontRequest) -> ResolvedFontFace {
        ResolvedFontFace {
            portability: FontPortabilityKind::UnresolvedFallback,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlyphRunQuality {
    Exact,
    PositionAdjusted,
    Approximate,
    DiagnosticOnly,
    Omitted,
}

impl GlyphRunQuality {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::PositionAdjusted => "positionAdjusted",
            Self::Approximate => "approximate",
            Self::DiagnosticOnly => "diagnosticOnly",
            Self::Omitted => "omitted",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextShapeDiagnostic {
    pub text: String,
    pub attempted: bool,
    pub public_glyph_run_emitted: bool,
    pub quality: GlyphRunQuality,
    pub replay_eligibility: GlyphRunReplayEligibility,
    pub strict_visual_eligible: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TextShapeReport {
    pub diagnostics: Vec<TextShapeDiagnostic>,
}

impl TextShapeReport {
    pub fn public_glyph_run_count(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.public_glyph_run_emitted)
            .count()
    }
}

pub struct TextShapeLowerer<'a> {
    resolver: &'a dyn FontResolver,
}

impl<'a> TextShapeLowerer<'a> {
    pub fn new(resolver: &'a dyn FontResolver) -> Self {
        Self { resolver }
    }

    pub fn diagnostics_only(resolver: &'a dyn FontResolver) -> Self {
        Self::new(resolver)
    }

    pub fn analyze_root(&self, root: &LayerNode) -> TextShapeReport {
        let mut report = TextShapeReport::default();
        self.collect_node(root, &mut report);
        report
    }

    pub fn lower_root(&self, root: &mut LayerNode) -> TextShapeReport {
        let mut report = TextShapeReport::default();
        let mut next_text_source_id = 0_u32;
        self.lower_node(root, &mut report, &mut next_text_source_id);
        report
    }

    fn collect_node(&self, node: &LayerNode, report: &mut TextShapeReport) {
        match &node.kind {
            LayerNodeKind::Group { children, .. } => {
                for child in children {
                    self.collect_node(child, report);
                }
            }
            LayerNodeKind::ClipRect { child, .. } => self.collect_node(child, report),
            LayerNodeKind::Leaf { ops } => {
                for op in ops {
                    if let PaintOp::TextRun { run, .. } = op {
                        report.diagnostics.push(self.analyze_text_run(run));
                    }
                }
            }
        }
    }

    fn lower_node(
        &self,
        node: &mut LayerNode,
        report: &mut TextShapeReport,
        next_text_source_id: &mut u32,
    ) {
        match &mut node.kind {
            LayerNodeKind::Group { children, .. } => {
                for child in children {
                    self.lower_node(child, report, next_text_source_id);
                }
            }
            LayerNodeKind::ClipRect { child, .. } => {
                self.lower_node(child, report, next_text_source_id);
            }
            LayerNodeKind::Leaf { ops } => {
                let existing_glyph_groups = ops
                    .iter()
                    .filter_map(|op| match op {
                        PaintOp::GlyphRun { run, .. } => {
                            Some(run.variant.equivalence_group.clone())
                        }
                        _ => None,
                    })
                    .collect::<HashSet<_>>();
                let mut lowered = Vec::with_capacity(ops.len());
                for op in ops.drain(..) {
                    if let PaintOp::TextRun { bbox, run } = op {
                        let text_source_id = *next_text_source_id;
                        let equivalence_group = format!("text-{text_source_id}");
                        if existing_glyph_groups.contains(&equivalence_group) {
                            lowered.push(PaintOp::TextRun { bbox, run });
                            *next_text_source_id = (*next_text_source_id).saturating_add(1);
                            continue;
                        }
                        let (diagnostic, glyph_run) =
                            self.lower_text_run(bbox, &run, text_source_id);
                        report.diagnostics.push(diagnostic);
                        lowered.push(PaintOp::TextRun { bbox, run });
                        if let Some(glyph_run) = glyph_run {
                            lowered.push(PaintOp::GlyphRun {
                                bbox,
                                run: Box::new(glyph_run),
                            });
                        }
                        *next_text_source_id = (*next_text_source_id).saturating_add(1);
                    } else {
                        lowered.push(op);
                    }
                }
                *ops = lowered;
            }
        }
    }

    fn analyze_text_run(&self, run: &TextRunNode) -> TextShapeDiagnostic {
        self.evaluate_text_run(None, run, 0).0
    }

    fn lower_text_run(
        &self,
        bbox: BoundingBox,
        run: &TextRunNode,
        text_source_id: u32,
    ) -> (TextShapeDiagnostic, Option<LayerGlyphRunPaint>) {
        self.evaluate_text_run(Some(bbox), run, text_source_id)
    }

    fn evaluate_text_run(
        &self,
        bbox: Option<BoundingBox>,
        run: &TextRunNode,
        text_source_id: u32,
    ) -> (TextShapeDiagnostic, Option<LayerGlyphRunPaint>) {
        if run.char_overlap.is_some() || run.text.is_empty() {
            return (
                TextShapeDiagnostic {
                    text: run.text.clone(),
                    attempted: false,
                    public_glyph_run_emitted: false,
                    quality: GlyphRunQuality::Omitted,
                    replay_eligibility: GlyphRunReplayEligibility::NotReplayable,
                    strict_visual_eligible: false,
                    reason: Some("notShapingCandidate".to_string()),
                },
                None,
            );
        }

        let request = FontRequest::from(run);
        let resolved = self.resolver.resolve_font(&request);
        let replay_eligibility = GlyphRunReplayEligibility::from(resolved.portability);
        let attempted = matches!(
            replay_eligibility,
            GlyphRunReplayEligibility::Portable
                | GlyphRunReplayEligibility::ConditionalExternalFont
                | GlyphRunReplayEligibility::LocalDiagnosticOnly
        );
        let mut diagnostic_quality = if attempted {
            GlyphRunQuality::DiagnosticOnly
        } else {
            GlyphRunQuality::Omitted
        };
        let mut reason = match replay_eligibility {
            GlyphRunReplayEligibility::Portable => Some("diagnosticsOnlySkeleton".to_string()),
            GlyphRunReplayEligibility::ConditionalExternalFont => {
                Some("externalFontRequiresConsumerVerification".to_string())
            }
            GlyphRunReplayEligibility::LocalDiagnosticOnly => {
                Some("localDiagnosticOnly".to_string())
            }
            GlyphRunReplayEligibility::NotReplayable => Some("fontResourceUnavailable".to_string()),
        };

        let mut public_glyph_run = None;
        let mut public_glyph_run_emitted = false;
        let mut strict_visual_eligible = false;
        let paint_style = PaintTextStyle::from(&run.style);

        if matches!(
            replay_eligibility,
            GlyphRunReplayEligibility::Portable
                | GlyphRunReplayEligibility::ConditionalExternalFont
        ) {
            if let Some(bbox) = bbox {
                if let Some(shaped) = self.resolver.shape_glyph_run(&request, run, &resolved) {
                    if !paint_style.is_fill_only_glyph_replay() {
                        reason = Some("unsupportedGlyphRunPaintEffect".to_string());
                    } else if glyph_run_is_exportable(&shaped) {
                        let equivalence_group = format!("text-{text_source_id}");
                        let mut glyph_variant =
                            crate::paint::PaintVariantMeta::text_run_default(equivalence_group);
                        glyph_variant.variant_id = "glyphRun".to_string();
                        glyph_variant.variant_kind = TextVariantKind::GlyphRun;
                        glyph_variant.is_default_fallback = false;
                        glyph_variant.requires =
                            vec!["fontResources".to_string(), "text.glyphRun".to_string()];
                        glyph_variant.quality = Some(shaped.diagnostics.quality);
                        diagnostic_quality = glyph_quality_from_variant(shaped.diagnostics.quality);
                        strict_visual_eligible = shaped.diagnostics.strict_visual_eligible;
                        reason = shaped.diagnostics.reason.clone();
                        public_glyph_run_emitted = true;
                        public_glyph_run = Some(LayerGlyphRunPaint {
                            source: crate::paint::TextSourceSpan {
                                id: crate::paint::TextSourceId(text_source_id),
                                utf8_range: crate::paint::TextSourceRange::new(
                                    0,
                                    run.text.len() as u32,
                                ),
                                utf16_range: crate::paint::TextSourceRange::new(
                                    0,
                                    run.text.encode_utf16().count() as u32,
                                ),
                                stable_source_key: None,
                            },
                            variant: glyph_variant,
                            paint_style: paint_style.clone(),
                            shape_key: shaped.shape_key.clone(),
                            placement: text_run_placement(bbox, run),
                            glyph_ids: shaped.glyph_ids,
                            positions: shaped.positions,
                            advances: shaped.advances,
                            clusters: shaped.clusters,
                            direction: shaped.shape_key.direction,
                            bidi_level: None,
                            writing_mode: shaped.shape_key.writing_mode,
                            orientation: GlyphRunOrientation::from_text_run(run),
                            glyph_transforms: None,
                            diagnostics: shaped.diagnostics,
                        });
                    } else {
                        reason = Some("glyphRunDiagnosticsNotExportable".to_string());
                    }
                }
            }
        }

        (
            TextShapeDiagnostic {
                text: run.text.clone(),
                attempted,
                public_glyph_run_emitted,
                quality: diagnostic_quality,
                replay_eligibility,
                strict_visual_eligible,
                reason,
            },
            public_glyph_run,
        )
    }
}

pub(crate) fn text_run_placement(bbox: BoundingBox, run: &TextRunNode) -> TextRunPlacement {
    let radians = run.rotation.to_radians();
    let (sin, cos) = radians.sin_cos();
    let local_origin_x = -bbox.width / 2.0;
    let local_origin_y = -bbox.height / 2.0 + run.baseline;
    let center_x = bbox.x + bbox.width / 2.0;
    let center_y = bbox.y + bbox.height / 2.0;
    TextRunPlacement {
        run_to_page: LayerAffineTransform {
            a: cos,
            b: sin,
            c: -sin,
            d: cos,
            e: center_x + cos * local_origin_x - sin * local_origin_y,
            f: center_y + sin * local_origin_x + cos * local_origin_y,
        },
        baseline_y: 0.0,
    }
}

fn glyph_run_is_exportable(shaped: &ResolvedGlyphRun) -> bool {
    !shaped.glyph_ids.is_empty()
        && shaped.glyph_ids.len() == shaped.positions.len()
        && shaped
            .advances
            .as_ref()
            .map_or(true, |advances| advances.len() == shaped.glyph_ids.len())
        && !shaped.clusters.is_empty()
        && matches!(
            shaped.diagnostics.replay_eligibility,
            GlyphRunReplayEligibility::Portable
                | GlyphRunReplayEligibility::ConditionalExternalFont
        )
        && matches!(
            shaped.diagnostics.quality,
            TextVariantQuality::Exact | TextVariantQuality::PositionAdjusted
        )
        && shaped.diagnostics.missing_glyph_count == 0
        && shaped.diagnostics.cluster_mismatch_count == 0
}

fn glyph_quality_from_variant(quality: TextVariantQuality) -> GlyphRunQuality {
    match quality {
        TextVariantQuality::Exact => GlyphRunQuality::Exact,
        TextVariantQuality::PositionAdjusted => GlyphRunQuality::PositionAdjusted,
        TextVariantQuality::Approximate => GlyphRunQuality::Approximate,
        TextVariantQuality::DiagnosticOnly => GlyphRunQuality::DiagnosticOnly,
        TextVariantQuality::Omitted => GlyphRunQuality::Omitted,
    }
}
