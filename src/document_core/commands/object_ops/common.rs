//! 공통 개체 속성/헬퍼 + 새 번호 (object_ops 분할, #1904).

use super::MIN_SHAPE_SIZE;
use crate::document_core::helpers::{get_textbox_from_shape, get_textbox_from_shape_mut};
use crate::document_core::DocumentCore;
use crate::error::HwpError;
use crate::model::control::Control;
use crate::model::event::DocumentEvent;
use crate::model::paragraph::Paragraph;
use crate::model::shape::{common_obj_offsets, ShapeObject};

/// [개선 트랙1] 기준계 전환 rebase 계획 — mutation 전 실측(frames/bbox)과
/// old rel/align 스냅샷. apply 후 `DocumentCore::rebased_offsets` 로 확정한다.
pub(crate) struct ObjectRebasePlan {
    pub frames: crate::renderer::float_placement::RebaseFrames,
    pub bbox: crate::renderer::render_tree::BoundingBox,
    pub h_wanted: bool,
    pub v_wanted: bool,
    pub old_horz: (
        crate::model::shape::HorzRelTo,
        crate::model::shape::HorzAlign,
    ),
    pub old_vert: (
        crate::model::shape::VertRelTo,
        crate::model::shape::VertAlign,
    ),
}

impl DocumentCore {
    const COMMON_OBJ_ATTR_KNOWN_MASK: u32 = 0x01
        | (0x03 << 3)
        | (0x07 << 5)
        | (0x03 << 8)
        | (0x07 << 10)
        | (1 << 13)
        | (1 << 14)
        | (0x07 << 15)
        | (0x03 << 18)
        | (1 << 20)
        | (0x07 << 21)
        | (0x03 << 24)
        | (1 << 26)
        | (1 << 28);
    pub(crate) fn sync_common_obj_attr_known_bits(c: &mut crate::model::shape::CommonObjAttr) {
        let packed =
            crate::document_core::converters::common_obj_attr_writer::pack_common_attr_bits(c);
        c.attr = (c.attr & !Self::COMMON_OBJ_ATTR_KNOWN_MASK)
            | (packed & Self::COMMON_OBJ_ATTR_KNOWN_MASK);
    }
    /// 물리(`CommonObjAttr`) → `raw_ctrl_data` 사본 갱신.
    ///
    /// HWP5 파스본은 직렬화기가 raw_ctrl_data 를 그대로 기록하므로(표 CTRL_HEADER ·
    /// 수식 eqed) setter 가 물리를 바꾼 뒤 이 함수를 불러 사본을 따라오게 한다.
    /// **비어 있거나 짧으면 손대지 않는다** — 토막을 만들면 HWPX→HWP 어댑터의
    /// `is_empty` 합성 조건이 무력화된다(표 저장 손상 사고와 같은 기전).
    pub(crate) fn sync_raw_ctrl_data_from_common(
        c: &crate::model::shape::CommonObjAttr,
        raw: &mut [u8],
    ) {
        use crate::model::shape::common_obj_offsets as o;
        if raw.len() < o::MIN_LEN {
            return;
        }
        raw[o::FLAGS].copy_from_slice(&c.attr.to_le_bytes());
        raw[o::V_OFFSET].copy_from_slice(&c.vertical_offset.to_le_bytes());
        raw[o::H_OFFSET].copy_from_slice(&c.horizontal_offset.to_le_bytes());
        raw[o::WIDTH].copy_from_slice(&c.width.to_le_bytes());
        raw[o::HEIGHT].copy_from_slice(&c.height.to_le_bytes());
        raw[o::Z_ORDER].copy_from_slice(&c.z_order.to_le_bytes());
        raw[o::MARGIN_LEFT].copy_from_slice(&c.margin.left.to_le_bytes());
        raw[o::MARGIN_RIGHT].copy_from_slice(&c.margin.right.to_le_bytes());
        raw[o::MARGIN_TOP].copy_from_slice(&c.margin.top.to_le_bytes());
        raw[o::MARGIN_BOTTOM].copy_from_slice(&c.margin.bottom.to_le_bytes());
        if raw.len() >= o::PREVENT_PAGE_BREAK.end {
            raw[o::PREVENT_PAGE_BREAK].copy_from_slice(&c.prevent_page_break.to_le_bytes());
        }
    }
    pub(crate) fn is_structure_only_empty_paragraph(para: &Paragraph) -> bool {
        para.text.is_empty()
            && !para.controls.is_empty()
            && para
                .controls
                .iter()
                .all(|ctrl| matches!(ctrl, Control::SectionDef(_) | Control::ColumnDef(_)))
    }
    /// 컨트롤 삭제 후 문단의 line_segs를 재계산한다.
    ///
    /// 그림/도형 삭제 시 문단의 line_segs에 컨트롤 높이가 그대로 남아,
    /// 레이아웃이 갱신되지 않는 문제를 방지한다.
    pub(crate) fn reflow_paragraph_line_segs_after_control_delete(
        para: &mut Paragraph,
        styles: &crate::renderer::style_resolver::ResolvedStyleSet,
        dpi: f64,
    ) {
        // 남은 컨트롤 중 가장 큰 높이 계산
        let max_remaining_ctrl_height = para
            .controls
            .iter()
            .map(|ctrl| match ctrl {
                Control::Picture(pic) => pic.common.height as i32,
                Control::Shape(shape) => shape.common().height as i32,
                Control::Equation(eq) => eq.common.height as i32,
                _ => 0,
            })
            .max()
            .unwrap_or(0);

        if max_remaining_ctrl_height > 0 {
            // 아직 컨트롤이 남아있으면 가장 큰 컨트롤 높이로 설정
            if let Some(ls) = para.line_segs.first_mut() {
                ls.line_height = max_remaining_ctrl_height;
                ls.text_height = max_remaining_ctrl_height;
                ls.baseline_distance = (max_remaining_ctrl_height * 850) / 1000;
            }
        } else if para.text.is_empty() {
            // 텍스트도 컨트롤도 없음 → 기본 텍스트 높이로 리셋
            if let Some(ls) = para.line_segs.first_mut() {
                ls.line_height = 1000;
                ls.text_height = 1000;
                ls.baseline_distance = 850;
                ls.line_spacing = 600;
            }
        } else {
            // 텍스트가 있으면 reflow_line_segs로 재계산
            let seg_width = para.line_segs.first().map(|s| s.segment_width).unwrap_or(0);
            let available_width_px = crate::renderer::hwpunit_to_px(seg_width, dpi);
            crate::renderer::composer::reflow_line_segs(para, available_width_px, styles, dpi);
        }
    }
    /// CommonObjAttr → JSON 문자열 (Shape/Picture 공용 속성)
    pub(crate) fn common_obj_attr_to_json(c: &crate::model::shape::CommonObjAttr) -> String {
        let vert_rel = match c.vert_rel_to {
            crate::model::shape::VertRelTo::Paper => "Paper",
            crate::model::shape::VertRelTo::Page => "Page",
            crate::model::shape::VertRelTo::Para => "Para",
        };
        let vert_align = match c.vert_align {
            crate::model::shape::VertAlign::Top => "Top",
            crate::model::shape::VertAlign::Center => "Center",
            crate::model::shape::VertAlign::Bottom => "Bottom",
            crate::model::shape::VertAlign::Inside => "Inside",
            crate::model::shape::VertAlign::Outside => "Outside",
        };
        let horz_rel = match c.horz_rel_to {
            crate::model::shape::HorzRelTo::Paper => "Paper",
            crate::model::shape::HorzRelTo::Page => "Page",
            crate::model::shape::HorzRelTo::Column => "Column",
            crate::model::shape::HorzRelTo::Para => "Para",
        };
        let horz_align = match c.horz_align {
            crate::model::shape::HorzAlign::Left => "Left",
            crate::model::shape::HorzAlign::Center => "Center",
            crate::model::shape::HorzAlign::Right => "Right",
            crate::model::shape::HorzAlign::Inside => "Inside",
            crate::model::shape::HorzAlign::Outside => "Outside",
        };
        let text_wrap = match c.text_wrap {
            crate::model::shape::TextWrap::Square => "Square",
            crate::model::shape::TextWrap::Tight => "Tight",
            crate::model::shape::TextWrap::Through => "Through",
            crate::model::shape::TextWrap::TopAndBottom => "TopAndBottom",
            crate::model::shape::TextWrap::BehindText => "BehindText",
            crate::model::shape::TextWrap::InFrontOfText => "InFrontOfText",
        };
        let desc_escaped = crate::document_core::helpers::json_escape(&c.description);
        format!(
            "\"width\":{},\"height\":{},\"treatAsChar\":{},\
             \"vertRelTo\":\"{}\",\"vertAlign\":\"{}\",\
             \"horzRelTo\":\"{}\",\"horzAlign\":\"{}\",\
             \"vertOffset\":{},\"horzOffset\":{},\
             \"textWrap\":\"{}\",\"restrictInPage\":{},\"allowOverlap\":{},\"sizeProtect\":{},\
             \"zOrder\":{},\"instanceId\":{},\
             \"outerMarginLeft\":{},\"outerMarginTop\":{},\
             \"outerMarginRight\":{},\"outerMarginBottom\":{},\
             \"description\":\"{}\"",
            c.width,
            c.height,
            c.treat_as_char,
            vert_rel,
            vert_align,
            horz_rel,
            horz_align,
            // [2026-08-15 신고 "자유이동 되지 않고"] 오프셋은 u32 비트캐스트로 저장된
            // 부호 있는 값이다. unsigned 로 내보내면 앵커 위/왼쪽(음수) 드래그 한 번에
            // 4294967295 가 되고, 스튜디오의 다음 드래그 산술(기존값+델타)이 i32 를
            // 벗어나 setter 가 거부 — 이후 개체 이동이 영구히 죽는다.
            c.vertical_offset as i32,
            c.horizontal_offset as i32,
            text_wrap,
            c.flow_with_text,
            c.allow_overlap,
            c.size_protect,
            c.z_order,
            c.instance_id,
            c.margin.left,
            c.margin.top,
            c.margin.right,
            c.margin.bottom,
            desc_escaped,
        )
    }
    /// JSON → CommonObjAttr 필드 업데이트 (Shape/Picture 공용)
    pub(crate) fn apply_common_obj_attr_from_json(
        c: &mut crate::model::shape::CommonObjAttr,
        props_json: &str,
    ) {
        use crate::document_core::helpers::{json_bool, json_i16, json_str, json_u32};

        if let Some(w) = json_u32(props_json, "width") {
            c.width = w.max(MIN_SHAPE_SIZE);
        }
        if let Some(h) = json_u32(props_json, "height") {
            c.height = h.max(MIN_SHAPE_SIZE);
        }
        if let Some(tac) = json_bool(props_json, "treatAsChar") {
            c.treat_as_char = tac;
            if tac {
                c.attr |= 0x01;
            } else {
                c.attr &= !0x01;
            }
        }
        if let Some(v) = json_str(props_json, "vertRelTo") {
            c.vert_rel_to = match v.as_str() {
                "Paper" => crate::model::shape::VertRelTo::Paper,
                "Page" => crate::model::shape::VertRelTo::Page,
                "Para" => crate::model::shape::VertRelTo::Para,
                _ => c.vert_rel_to,
            };
        }
        if let Some(v) = json_str(props_json, "horzRelTo") {
            c.horz_rel_to = match v.as_str() {
                "Paper" => crate::model::shape::HorzRelTo::Paper,
                "Page" => crate::model::shape::HorzRelTo::Page,
                "Column" => crate::model::shape::HorzRelTo::Column,
                "Para" => crate::model::shape::HorzRelTo::Para,
                _ => c.horz_rel_to,
            };
        }
        if let Some(v) = json_str(props_json, "vertAlign") {
            c.vert_align = match v.as_str() {
                "Top" => crate::model::shape::VertAlign::Top,
                "Center" => crate::model::shape::VertAlign::Center,
                "Bottom" => crate::model::shape::VertAlign::Bottom,
                _ => c.vert_align,
            };
        }
        if let Some(v) = json_str(props_json, "horzAlign") {
            c.horz_align = match v.as_str() {
                "Left" => crate::model::shape::HorzAlign::Left,
                "Center" => crate::model::shape::HorzAlign::Center,
                "Right" => crate::model::shape::HorzAlign::Right,
                _ => c.horz_align,
            };
        }
        if let Some(v) = json_str(props_json, "textWrap") {
            c.text_wrap = match v.as_str() {
                "Square" => crate::model::shape::TextWrap::Square,
                "Tight" => crate::model::shape::TextWrap::Tight,
                "Through" => crate::model::shape::TextWrap::Through,
                "TopAndBottom" => crate::model::shape::TextWrap::TopAndBottom,
                "BehindText" => crate::model::shape::TextWrap::BehindText,
                "InFrontOfText" => crate::model::shape::TextWrap::InFrontOfText,
                _ => c.text_wrap,
            };
        }
        if let Some(v) = json_bool(props_json, "restrictInPage") {
            c.flow_with_text = v;
            if v {
                c.attr |= 1 << 13;
                c.allow_overlap = false;
                c.attr &= !(1 << 14);
            } else {
                c.attr &= !(1 << 13);
            }
        }
        if let Some(v) = json_bool(props_json, "allowOverlap") {
            c.allow_overlap = v;
            if v {
                c.attr |= 1 << 14;
            } else {
                c.attr &= !(1 << 14);
            }
        }
        if let Some(v) = json_bool(props_json, "sizeProtect") {
            c.size_protect = v;
            if v {
                c.attr |= 1 << 20;
            } else {
                c.attr &= !(1 << 20);
            }
        }
        if c.flow_with_text {
            c.allow_overlap = false;
            c.attr &= !(1 << 14);
        }
        // [개선 트랙1 2026-08-05] 오프셋은 부호 있는 HWPUNIT — json_u32 는 '-' 에서
        // 파싱이 끊겨 음수를 조용히 버렸다(그림·표 경로의 json_i32 와 비대칭).
        if let Some(v) = crate::document_core::helpers::json_i32(props_json, "vertOffset") {
            c.vertical_offset = v as u32;
        }
        if let Some(v) = crate::document_core::helpers::json_i32(props_json, "horzOffset") {
            c.horizontal_offset = v as u32;
        }
        if let Some(v) = json_str(props_json, "description") {
            c.description = v;
        }
        if let Some(v) = json_i16(props_json, "outerMarginLeft") {
            c.margin.left = v;
        }
        if let Some(v) = json_i16(props_json, "outerMarginTop") {
            c.margin.top = v;
        }
        if let Some(v) = json_i16(props_json, "outerMarginRight") {
            c.margin.right = v;
        }
        if let Some(v) = json_i16(props_json, "outerMarginBottom") {
            c.margin.bottom = v;
        }
        Self::sync_common_obj_attr_known_bits(c);
    }
    /// 직선 끝점 이동: 글로벌 좌표(HWPUNIT)로 시작/끝점을 직접 설정
    pub fn move_line_endpoint_native(
        &mut self,
        section_idx: usize,
        para_idx: usize,
        control_idx: usize,
        start_x: i32,
        start_y: i32,
        end_x: i32,
        end_y: i32,
    ) -> Result<String, HwpError> {
        let section = self
            .document
            .sections
            .get_mut(section_idx)
            .ok_or_else(|| HwpError::RenderError("구역 범위 초과".to_string()))?;
        let para = section
            .paragraphs
            .get_mut(para_idx)
            .ok_or_else(|| HwpError::RenderError("문단 범위 초과".to_string()))?;
        let ctrl = para
            .controls
            .get_mut(control_idx)
            .ok_or_else(|| HwpError::RenderError("컨트롤 범위 초과".to_string()))?;
        let line = match ctrl {
            Control::Shape(ref mut s) => match s.as_mut() {
                ShapeObject::Line(ref mut l) => l,
                _ => return Err(HwpError::RenderError("직선이 아닙니다".to_string())),
            },
            _ => return Err(HwpError::RenderError("Shape이 아닙니다".to_string())),
        };

        let min_x = start_x.min(end_x);
        let min_y = start_y.min(end_y);
        let w = (start_x - end_x).abs().max(1);
        let h = (start_y - end_y).abs().max(0);

        line.common.horizontal_offset = min_x as u32;
        line.common.vertical_offset = min_y as u32;
        line.common.width = w as u32;
        line.common.height = h.max(1) as u32;
        line.start.x = start_x - min_x;
        line.start.y = start_y - min_y;
        line.end.x = end_x - min_x;
        line.end.y = end_y - min_y;

        line.drawing.shape_attr.current_width = w as u32;
        line.drawing.shape_attr.original_width = w as u32;
        line.drawing.shape_attr.current_height = h.max(1) as u32;
        line.drawing.shape_attr.original_height = h.max(1) as u32;
        line.drawing.shape_attr.rotation_center.x = w / 2;
        line.drawing.shape_attr.rotation_center.y = h / 2;
        line.drawing.shape_attr.raw_rendering = Vec::new();

        section.raw_stream = None;
        self.recompose_section(section_idx);
        self.paginate_if_needed();
        self.update_connectors_in_section(section_idx);

        Ok("{\"ok\":true}".to_string())
    }
    /// [개선 트랙1 2026-08-05] 기준계 전환 rebase 용 실측 프로브 — mutation **전에**
    /// 렌더트리에서 (기준 프레임들, 개체 현재 bbox)를 채취한다.
    ///
    /// 렌더트리 실측이 정본인 근거는 저장소에 이미 두 번 선언돼 있다: 어울림 훅은
    /// 'vpos 축은 float 흐름 소비를 미반영, 렌더트리가 정본'(table_ops.rs 어울림 본편),
    /// Task #1151 셀 좌표도 렌더트리 채취(table.rs compute_cell_page_offset). 별도
    /// 페이지 레이아웃 재계산 경로를 새로 만들지 않는다.
    ///
    /// para_y 는 host 문단 첫 TextLine top — 공백뿐인 host 는 TextLine 이 없어 None 일
    /// 수 있고, 그 경우 Para 축 rebase 는 스킵된다(offset_for_target_y 가 None 반환).
    pub(crate) fn probe_object_frames(
        &self,
        section_idx: usize,
        para_idx: usize,
        control_idx: usize,
    ) -> Option<(
        crate::renderer::float_placement::RebaseFrames,
        crate::renderer::render_tree::BoundingBox,
    )> {
        use crate::renderer::page_layout::LayoutRect;
        use crate::renderer::render_tree::{BoundingBox, RenderNode, RenderNodeType};

        #[derive(Default)]
        struct Probe {
            body: Option<BoundingBox>,
            col: Option<BoundingBox>,
            para_y: Option<f64>,
            bbox: Option<BoundingBox>,
        }

        fn walk(n: &RenderNode, sec: usize, pi: usize, ci: usize, probe: &mut Probe) {
            match &n.node_type {
                // 머리말/꼬리말/바탕쪽/각주는 자체 문단 인덱스 공간 — 본문과 충돌 방지.
                RenderNodeType::Header
                | RenderNodeType::Footer
                | RenderNodeType::MasterPage
                | RenderNodeType::FootnoteArea => return,
                RenderNodeType::Body { .. } => {
                    if probe.body.is_none() {
                        probe.body = Some(n.bbox);
                    }
                }
                RenderNodeType::Column(_) => {
                    if probe.col.is_none() {
                        probe.col = Some(n.bbox);
                    }
                }
                RenderNodeType::TextLine(tl) => {
                    if tl.para_index == Some(pi)
                        && tl.line_index == Some(0)
                        && probe.para_y.is_none()
                    {
                        probe.para_y = Some(n.bbox.y);
                    }
                }
                RenderNodeType::Table(t) => {
                    if t.section_index == Some(sec)
                        && t.para_index == Some(pi)
                        && t.control_index == Some(ci)
                        && probe.bbox.is_none()
                    {
                        probe.bbox = Some(n.bbox);
                    }
                    // 셀 내부는 본문이 아니다 (어울림 프로브와 동일 규칙).
                    return;
                }
                RenderNodeType::Image(img) => {
                    if img.section_index == Some(sec)
                        && img.para_index == Some(pi)
                        && img.control_index == Some(ci)
                        && probe.bbox.is_none()
                    {
                        probe.bbox = Some(n.bbox);
                    }
                }
                RenderNodeType::Line(l) => {
                    if l.section_index == Some(sec)
                        && l.para_index == Some(pi)
                        && l.control_index == Some(ci)
                        && l.cell_para_index.is_none()
                        && probe.bbox.is_none()
                    {
                        probe.bbox = Some(n.bbox);
                    }
                }
                RenderNodeType::Rectangle(r) => {
                    if r.section_index == Some(sec)
                        && r.para_index == Some(pi)
                        && r.control_index == Some(ci)
                        && r.cell_para_index.is_none()
                        && probe.bbox.is_none()
                    {
                        probe.bbox = Some(n.bbox);
                    }
                }
                RenderNodeType::Ellipse(e) => {
                    if e.section_index == Some(sec)
                        && e.para_index == Some(pi)
                        && e.control_index == Some(ci)
                        && e.cell_para_index.is_none()
                        && probe.bbox.is_none()
                    {
                        probe.bbox = Some(n.bbox);
                    }
                }
                RenderNodeType::Path(p) => {
                    if p.section_index == Some(sec)
                        && p.para_index == Some(pi)
                        && p.control_index == Some(ci)
                        && p.cell_para_index.is_none()
                        && probe.bbox.is_none()
                    {
                        probe.bbox = Some(n.bbox);
                    }
                }
                RenderNodeType::Equation(eq) => {
                    if eq.section_index == Some(sec)
                        && eq.para_index == Some(pi)
                        && eq.control_index == Some(ci)
                        && eq.cell_para_index.is_none()
                        && probe.bbox.is_none()
                    {
                        probe.bbox = Some(n.bbox);
                    }
                }
                _ => {}
            }
            for c in &n.children {
                walk(c, sec, pi, ci, probe);
            }
        }

        // 페이지 수 확보(조판 유발) 후 페이지 순회 — compute_cell_page_offset 패턴.
        let page_count = self.page_count().max(1) as usize;
        for pg in 0..page_count {
            let Ok(tree) = self.build_page_render_tree(pg as u32) else {
                continue;
            };
            let mut probe = Probe::default();
            walk(&tree.root, section_idx, para_idx, control_idx, &mut probe);
            let Some(bbox) = probe.bbox else { continue };
            let paper = tree.root.bbox;
            let to_rect = |b: BoundingBox| LayoutRect {
                x: b.x,
                y: b.y,
                width: b.width,
                height: b.height,
            };
            let body = probe.body.map(to_rect)?;
            let col = probe.col.map(to_rect).unwrap_or(body);
            return Some((
                crate::renderer::float_placement::RebaseFrames {
                    paper_w: paper.width,
                    paper_h: paper.height,
                    body,
                    col,
                    para_y: probe.para_y,
                },
                bbox,
            ));
        }
        None
    }
    /// [개선 트랙1] rebase 판정 + 프로브 — mutation **전에** 호출한다.
    ///
    /// 판정: 축별로 (rel_to/align 키가 있고) AND (오프셋 키 부재)일 때만 후보.
    /// 명시 오프셋 동봉 = opt-out (신규 API 불필요). 값이 실제 변했는지는 apply 후
    /// `rebased_offsets` 가 old 스냅샷과 비교해 확정한다. textWrap 단독 변경은 키
    /// 자체가 없어 후보가 안 되고(wrap 은 기준계 불변), treatAsChar 토글은 별도
    /// migration 계약(picture tac)이 맡으므로 제외한다.
    ///
    /// ⚠ 확인 필요(리스크): studio 다이얼로그가 변경 키만이 아니라 전체 키(오프셋
    /// 포함)를 항상 전송하면 이 판정이 영영 안 걸린다 — 그 경우 명시 opt-in 키
    /// ("rebaseOffsets":true)로 전환한다.
    pub(crate) fn plan_object_rebase(
        &self,
        section_idx: usize,
        para_idx: usize,
        control_idx: usize,
        props_json: &str,
        old: &crate::model::shape::CommonObjAttr,
    ) -> Option<ObjectRebasePlan> {
        use crate::document_core::helpers::{json_bool, json_str};

        // tac 개체·tac 토글은 rebase 대상이 아니다.
        if old.treat_as_char {
            return None;
        }
        if json_bool(props_json, "treatAsChar").is_some_and(|v| v != old.treat_as_char) {
            return None;
        }
        let h_wanted = (json_str(props_json, "horzRelTo").is_some()
            || json_str(props_json, "horzAlign").is_some())
            && !props_json.contains("\"horzOffset\"");
        let v_wanted = (json_str(props_json, "vertRelTo").is_some()
            || json_str(props_json, "vertAlign").is_some())
            && !props_json.contains("\"vertOffset\"");
        if !h_wanted && !v_wanted {
            // 조기 탈출 — 프로브(페이지 수 × 렌더트리 1회)를 아예 안 돈다.
            return None;
        }
        let (frames, bbox) = self.probe_object_frames(section_idx, para_idx, control_idx)?;
        Some(ObjectRebasePlan {
            frames,
            bbox,
            h_wanted,
            v_wanted,
            old_horz: (old.horz_rel_to, old.horz_align),
            old_vert: (old.vert_rel_to, old.vert_align),
        })
    }

    /// [개선 트랙1] apply 후 새 기준계로 역산한 오프셋 (h, v). 축별로 값이 실제
    /// 변했을 때만 Some — Para 세로인데 para_y 미채취면 None(해당 축 현행 유지).
    pub(crate) fn rebased_offsets(
        plan: &ObjectRebasePlan,
        new: &crate::model::shape::CommonObjAttr,
        dpi: f64,
    ) -> (Option<i32>, Option<i32>) {
        use crate::renderer::float_placement::{offset_for_target_x, offset_for_target_y};
        if new.treat_as_char {
            return (None, None);
        }
        let h = (plan.h_wanted && plan.old_horz != (new.horz_rel_to, new.horz_align)).then(|| {
            offset_for_target_x(
                new.horz_rel_to,
                new.horz_align,
                plan.bbox.x,
                plan.bbox.width,
                &plan.frames,
                dpi,
            )
        });
        let v = if plan.v_wanted && plan.old_vert != (new.vert_rel_to, new.vert_align) {
            offset_for_target_y(
                new.vert_rel_to,
                new.vert_align,
                plan.bbox.y,
                plan.bbox.height,
                &plan.frames,
                dpi,
            )
        } else {
            None
        };
        (h, v)
    }
    pub(crate) fn first_char_or_nul(value: &str) -> char {
        value.chars().next().unwrap_or('\0')
    }
    pub(crate) fn hwpunit16_from_json(json: &str, key: &str) -> Option<i16> {
        crate::document_core::helpers::json_i32(json, key)
            .map(|v| v.clamp(i16::MIN as i32, i16::MAX as i32) as i16)
    }
}

impl crate::document_core::DocumentCore {
    pub fn insert_new_number_native(
        &mut self,
        section_idx: usize,
        para_idx: usize,
        char_offset: usize,
        start_num: u16,
    ) -> Result<String, crate::error::HwpError> {
        use crate::error::HwpError;
        use crate::model::control::{AutoNumberType, Control, NewNumber};

        if section_idx >= self.document.sections.len() {
            return Err(HwpError::RenderError(format!(
                "구역 인덱스 {} 범위 초과",
                section_idx
            )));
        }
        if para_idx >= self.document.sections[section_idx].paragraphs.len() {
            return Err(HwpError::RenderError(format!(
                "문단 인덱스 {} 범위 초과",
                para_idx
            )));
        }

        let new_number = NewNumber {
            number_type: AutoNumberType::Page,
            number: start_num,
        };

        self.document.sections[section_idx].raw_stream = None;
        let paragraph = &mut self.document.sections[section_idx].paragraphs[para_idx];

        let insert_idx = {
            let positions = crate::document_core::helpers::find_control_text_positions(paragraph);
            let mut idx = paragraph.controls.len();
            for (i, &pos) in positions.iter().enumerate() {
                if pos > char_offset {
                    idx = i;
                    break;
                }
            }
            idx
        };

        paragraph
            .controls
            .insert(insert_idx, Control::NewNumber(new_number));
        paragraph.ctrl_data_records.insert(insert_idx, None);

        if !paragraph.char_offsets.is_empty() {
            let text_len = paragraph.text.chars().count();
            let safe_offset = char_offset.min(text_len);
            for co in paragraph.char_offsets[safe_offset..].iter_mut() {
                *co += 8;
            }
        }
        paragraph.char_count += 8;
        paragraph.control_mask |= 1u32 << 0x0012;
        paragraph.has_para_text = true;

        self.reflow_paragraph(section_idx, para_idx);
        self.recompose_section(section_idx);
        self.paginate_if_needed();
        self.invalidate_page_tree_cache();

        Ok(crate::document_core::helpers::json_ok_with(&format!(
            "\"controlIdx\":{}",
            insert_idx
        )))
    }
}
