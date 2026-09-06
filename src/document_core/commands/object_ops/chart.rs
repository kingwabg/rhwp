//! 차트 삽입·조회·편집 명령 (OOXML 차트).
//!
//! 차트는 HWPX 에서 표준 OOXML `c:chartSpace` XML 을 **별도 파트**(`Chart/chart{N}.xml`)로
//! 담고 본문에서 `chartIDRef` 로 참조한다. 공통 IR 에서는 그 XML 을 BinDataContent
//! (id = [`OOXML_CHART_ID_BASE`] + N, extension = [`OOXML_CHART_EXT`])로 싣고, 개체는
//! `ShapeObject::Ole` 로 표현한다 — 렌더러가 이미 그 조합을 SVG 로 그리므로(shape_layout
//! Ole 분기) 삽입만 하면 화면·PDF 가 함께 따라온다. 새 개체 타입을 만들지 않는 이유다.

use crate::document_core::DocumentCore;
use crate::error::HwpError;
use crate::model::bin_data::{BinDataContent, OOXML_CHART_EXT, OOXML_CHART_ID_BASE};
use crate::model::control::Control;
use crate::model::shape::{OleShape, ShapeObject};
use crate::ooxml_chart::writer::{build_chart_xml, patch_chart_xml, ChartSeriesSpec, ChartSpec};
use crate::ooxml_chart::{OoxmlChart, OoxmlChartType};

/// 차트 기본 크기(HWPUNIT) — 한컴 기본 차트와 같은 비율(약 8×5cm).
const DEFAULT_CHART_W: u32 = 32250;
const DEFAULT_CHART_H: u32 = 18750;

fn kind_from_str(s: &str) -> OoxmlChartType {
    match s.to_ascii_lowercase().as_str() {
        "bar" => OoxmlChartType::Bar,
        "line" => OoxmlChartType::Line,
        "pie" => OoxmlChartType::Pie,
        _ => OoxmlChartType::Column,
    }
}

fn kind_to_str(k: OoxmlChartType) -> &'static str {
    match k {
        OoxmlChartType::Bar => "bar",
        OoxmlChartType::Line => "line",
        OoxmlChartType::Pie => "pie",
        _ => "column",
    }
}

/// 스튜디오가 보내는 JSON → ChartSpec.
///
/// ```json
/// { "type":"column", "title":"제목",
///   "categories":["1분기","2분기"],
///   "series":[{"name":"서울","values":[1,2]}] }
/// ```
fn spec_from_json(json: &str) -> Result<ChartSpec, HwpError> {
    let v: serde_json::Value = serde_json::from_str(json)
        .map_err(|e| HwpError::InvalidField(format!("차트 JSON 파싱 실패: {e}")))?;
    // style 은 갤러리에서 고른 세부 종류(예: "column-stacked"). 없으면 type 에서 유도.
    let style = v["style"].as_str().unwrap_or_default().to_string();
    let chart_type = kind_from_str(v["type"].as_str().unwrap_or("column"));
    let title = v["title"]
        .as_str()
        .filter(|t| !t.is_empty())
        .map(String::from);
    let categories: Vec<String> = v["categories"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|x| x.as_str().unwrap_or_default().to_string())
                .collect()
        })
        .unwrap_or_default();
    let series: Vec<ChartSeriesSpec> = v["series"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|s| ChartSeriesSpec {
                    name: s["name"].as_str().unwrap_or_default().to_string(),
                    values: s["values"]
                        .as_array()
                        .map(|vals| vals.iter().map(|x| x.as_f64().unwrap_or(0.0)).collect())
                        .unwrap_or_default(),
                })
                .collect()
        })
        .unwrap_or_default();

    if series.is_empty() {
        return Err(HwpError::InvalidField("계열이 비어 있습니다".into()));
    }
    if categories.is_empty() {
        return Err(HwpError::InvalidField("항목이 비어 있습니다".into()));
    }
    Ok(ChartSpec {
        style,
        chart_type,
        title,
        categories,
        series,
    })
}

fn spec_to_json(chart: &OoxmlChart, style: &str) -> String {
    let series: Vec<serde_json::Value> = chart
        .series
        .iter()
        .map(|s| {
            serde_json::json!({
                "name": s.name,
                "values": s.values,
            })
        })
        .collect();
    serde_json::json!({
        "ok": true,
        "type": kind_to_str(chart.chart_type),
        "style": style,
        "title": chart.title.clone().unwrap_or_default(),
        "categories": chart.categories,
        "series": series,
    })
    .to_string()
}

impl DocumentCore {
    /// 다음으로 쓸 차트 파트 번호(1-based) — 기존 차트와 겹치지 않게.
    fn next_chart_part_no(&self) -> u16 {
        let max = self
            .document
            .bin_data_content
            .iter()
            .filter(|b| b.extension == OOXML_CHART_EXT)
            .map(|b| b.id.saturating_sub(OOXML_CHART_ID_BASE))
            .max()
            .unwrap_or(0);
        max + 1
    }

    /// 차트를 삽입한다 — spec JSON 으로 OOXML XML 을 만들어 파트로 싣고 OLE 개체를 놓는다.
    ///
    /// 반환: `{"ok":true,"paraIdx":N,"controlIdx":N}`
    pub fn insert_chart_native(
        &mut self,
        section_idx: usize,
        para_idx: usize,
        spec_json: &str,
        width: u32,
        height: u32,
        treat_as_char: bool,
    ) -> Result<String, HwpError> {
        if section_idx >= self.document.sections.len() {
            return Err(HwpError::RenderError(format!(
                "구역 인덱스 {section_idx} 범위 초과"
            )));
        }
        if para_idx >= self.document.sections[section_idx].paragraphs.len() {
            return Err(HwpError::RenderError(format!(
                "문단 인덱스 {para_idx} 범위 초과"
            )));
        }
        let spec = spec_from_json(spec_json)?;
        let xml = build_chart_xml(&spec);

        let w = if width == 0 { DEFAULT_CHART_W } else { width };
        let h = if height == 0 { DEFAULT_CHART_H } else { height };

        // 차트 XML 을 파트로 등록
        let part_no = self.next_chart_part_no();
        let bin_id = OOXML_CHART_ID_BASE + part_no;
        self.document.bin_data_content.push(BinDataContent {
            id: bin_id,
            data: xml.into_bytes().into(),
            extension: OOXML_CHART_EXT.to_string(),
        });

        // CommonObjAttr: vert=Para(2), horz=Column(1), width/height=Absolute, wrap=Square
        // — 한컴이 저장하는 차트와 같은 배치(hp:pos vertRelTo="PARA" horzRelTo="COLUMN").
        let attr: u32 = (2 << 3) | (1 << 8) | (4 << 15) | (2 << 18);
        let mut common = crate::model::shape::CommonObjAttr {
            attr,
            treat_as_char,
            vert_rel_to: crate::model::shape::VertRelTo::Para,
            horz_rel_to: crate::model::shape::HorzRelTo::Column,
            text_wrap: crate::model::shape::TextWrap::Square,
            width: w,
            height: h,
            z_order: 1,
            numbering_type: crate::model::shape::ObjectNumberingType::Picture,
            ..Default::default()
        };
        // instance_id 는 비-0 이어야 한다(한컴 규약) — 크기·번호 기반 해시.
        common.instance_id = 0x7c15_0000u32
            .wrapping_add(part_no as u32)
            .wrapping_add(w.wrapping_mul(3))
            .wrapping_add(h.wrapping_mul(7))
            | 1;

        let ole = OleShape {
            common,
            extent_x: w as i32,
            extent_y: h as i32,
            bin_data_id: bin_id as u32,
            drawing_aspect: crate::model::shape::OleDrawingAspect::Content,
            ..Default::default()
        };

        self.document.sections[section_idx].raw_stream = None;
        let parent = &mut self.document.sections[section_idx].paragraphs[para_idx];
        let new_ctrl_idx = parent.controls.len();
        parent
            .controls
            .push(Control::Shape(Box::new(ShapeObject::Ole(Box::new(ole)))));
        parent.ctrl_data_records.push(None);

        self.mark_section_dirty(section_idx);
        self.recompose_section(section_idx);
        self.paginate_if_needed();
        self.invalidate_page_tree_cache();

        Ok(crate::document_core::helpers::json_ok_with(&format!(
            "\"paraIdx\":{para_idx},\"controlIdx\":{new_ctrl_idx},\"chartPart\":{part_no}"
        )))
    }

    /// 차트 개체의 데이터를 JSON 으로 돌려준다(편집 대화상자가 채울 값).
    pub fn get_chart_spec_native(
        &self,
        section_idx: usize,
        para_idx: usize,
        control_idx: usize,
    ) -> Result<String, HwpError> {
        let xml = self.chart_xml_of(section_idx, para_idx, control_idx)?;
        let chart = OoxmlChart::parse(&xml)
            .ok_or_else(|| HwpError::RenderError("차트 XML 을 해석하지 못했습니다".into()))?;
        // 저장된 XML 의 플롯 서명으로 갤러리 스타일을 되짚는다(편집 시 종류 유지)
        let sig = crate::ooxml_chart::writer::plot_signature(&String::from_utf8_lossy(&xml));
        let style = crate::ooxml_chart::writer::CHART_STYLES
            .iter()
            .find(|(id, _)| {
                crate::ooxml_chart::writer::plot_signature(
                    crate::ooxml_chart::writer::template_for_style(id),
                ) == sig
            })
            .map(|(id, _)| *id)
            .unwrap_or("");
        Ok(spec_to_json(&chart, style))
    }

    /// 차트 개체의 데이터를 교체한다 — **기존 XML 을 패치**하므로 서식이 보존된다.
    pub fn set_chart_spec_native(
        &mut self,
        section_idx: usize,
        para_idx: usize,
        control_idx: usize,
        spec_json: &str,
    ) -> Result<String, HwpError> {
        let spec = spec_from_json(spec_json)?;
        let bin_id = self.chart_bin_id_of(section_idx, para_idx, control_idx)?;
        let old = self.chart_xml_of(section_idx, para_idx, control_idx)?;
        let old_str = String::from_utf8_lossy(&old).to_string();

        // 스타일(플롯 서명)이 바뀌면 그 템플릿에서 새로 만든다 — 막대 XML 에 원형 데이터는
        // 못 넣는다. 같으면 패치해 사용자 서식을 보존한다.
        let want_style = if spec.style.is_empty() {
            crate::ooxml_chart::writer::default_style_of(spec.chart_type)
        } else {
            spec.style.as_str()
        };
        let same_kind = crate::ooxml_chart::writer::plot_signature(&old_str)
            == crate::ooxml_chart::writer::plot_signature(
                crate::ooxml_chart::writer::template_for_style(want_style),
            );
        let new_xml = if same_kind {
            patch_chart_xml(&old_str, &spec)
        } else {
            build_chart_xml(&spec)
        };

        let Some(slot) = self
            .document
            .bin_data_content
            .iter_mut()
            .find(|b| b.id == bin_id)
        else {
            return Err(HwpError::RenderError("차트 파트를 찾지 못했습니다".into()));
        };
        slot.data = new_xml.into_bytes().into();

        self.mark_section_dirty(section_idx);
        self.recompose_section(section_idx);
        self.paginate_if_needed();
        self.invalidate_page_tree_cache();
        Ok(crate::document_core::helpers::json_ok())
    }

    fn chart_bin_id_of(
        &self,
        section_idx: usize,
        para_idx: usize,
        control_idx: usize,
    ) -> Result<u16, HwpError> {
        let para = self
            .document
            .sections
            .get(section_idx)
            .and_then(|s| s.paragraphs.get(para_idx))
            .ok_or_else(|| HwpError::RenderError("문단을 찾지 못했습니다".into()))?;
        match para.controls.get(control_idx) {
            Some(Control::Shape(shape)) => match shape.as_ref() {
                ShapeObject::Ole(ole) if ole.bin_data_id as u16 >= OOXML_CHART_ID_BASE => {
                    Ok(ole.bin_data_id as u16)
                }
                _ => Err(HwpError::RenderError("차트 개체가 아닙니다".into())),
            },
            _ => Err(HwpError::RenderError("차트 개체가 아닙니다".into())),
        }
    }

    fn chart_xml_of(
        &self,
        section_idx: usize,
        para_idx: usize,
        control_idx: usize,
    ) -> Result<Vec<u8>, HwpError> {
        let bin_id = self.chart_bin_id_of(section_idx, para_idx, control_idx)?;
        self.document
            .bin_data_content
            .iter()
            .find(|b| b.id == bin_id)
            .map(|b| b.data.load().to_vec())
            .ok_or_else(|| HwpError::RenderError("차트 파트를 찾지 못했습니다".into()))
    }
}
