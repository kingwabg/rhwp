//! header.xml 파싱 — HWPX 문서 메타데이터를 DocInfo로 변환
//!
//! header.xml은 글꼴, 글자모양, 문단모양, 스타일, 테두리/배경 등
//! 문서 전체에서 참조하는 리소스 테이블을 포함한다.

use quick_xml::events::Event;
use quick_xml::Reader;

use crate::model::document::{DocInfo, DocProperties, RawRecord};
use crate::model::style::*;
use crate::parser::tags;

use super::utils::{
    attr_str, local_name, parse_bool, parse_color, parse_gradient_type, parse_hatch_style,
    parse_i16, parse_i32, parse_i8, parse_u16, parse_u32, parse_u8, skip_element,
};
use super::HwpxError;

/// `<hh:strikeout shape="..."/>` 의 shape 값이 실제 렌더링되는 취소선인지
/// 판정한다 (화이트리스트).
///
/// ## 배경
///
/// 한컴오피스 HWPX 익스포터는 본문 charPr 정의에 `<hh:strikeout shape="3D"/>`
/// 를 placeholder 기본값으로 넣어두는 경우가 많다. "3D"는 OWPML 스펙상
/// 유효한 취소선 모양이 아니며, 한컴 뷰어에서도 취소선으로 그리지 않는다.
/// 따라서 이를 진짜 strikethrough로 해석하면 정상 본문 전체가 취소선으로
/// 렌더링되는 버그가 생긴다.
///
/// 또한 한컴이 향후 다른 placeholder 값("Ghost", "4D" 등)을 추가할 가능성이
/// 있으므로, 블랙리스트(\"NONE\" | \"3D\" 제외)보다는 화이트리스트가 더
/// 안전하다. 알 수 없는 값은 fail-closed로 no-strike 처리한다.
///
/// ## 허용 값
///
/// 본 함수가 `true`를 반환하는 값은 OWPML `LineSym2` 열거(표 27 선 종류)와
/// shape.rs 의 `strike_shape` 매핑 표에서 모두 실제 선으로 인정되는 13종:
///
/// `SOLID`, `DASH`, `DOT`, `DASH_DOT`, `DASH_DOT_DOT`, `LONG_DASH`,
/// `CIRCLE`, `DOUBLE_SLIM`, `SLIM_THICK`, `THICK_SLIM`, `SLIM_THICK_SLIM`,
/// `WAVE`, `DOUBLE_WAVE`.
///
/// `NONE`, `3D`, 기타 모든 값은 `false` (취소선 없음).
pub(crate) fn is_real_strike_shape(shape: &str) -> bool {
    matches!(
        shape,
        "SOLID"
            | "DASH"
            | "DOT"
            | "DASH_DOT"
            | "DASH_DOT_DOT"
            | "LONG_DASH"
            | "CIRCLE"
            | "DOUBLE_SLIM"
            | "SLIM_THICK"
            | "THICK_SLIM"
            | "SLIM_THICK_SLIM"
            | "WAVE"
            | "DOUBLE_WAVE"
    )
}

/// header.xml의 `<hh:head version="X.Y">` 속성에서 hwpml 스키마 버전을 추출한다.
///
/// 이 값은 HWPML **스키마 버전**일 뿐 HWP3→HWPX 변환 지표가 아니다. 네이티브 한글2022
/// HWPX 도 head version "1.4" 로 저장되므로 변환본 판별에 쓰면 안 된다(Task #1608 — 과거
/// Task #554 의 `== "1.4"` 판별은 오탐지로 제거됨). 현재 용도는 직렬화 무손실을 위한 원본
/// 버전 보존(`doc_info.hwpml_version`)뿐이다.
///
/// 본 함수는 헤더 root element 만 읽고 즉시 반환하므로 비용이 매우 낮다.
pub fn parse_hwpx_hwpml_version(xml: &str) -> Option<String> {
    let mut reader = Reader::from_str(xml);
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                let name = e.name();
                if local_name(name.as_ref()) == b"head" {
                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref() == b"version" {
                            return Some(attr_str(&attr));
                        }
                    }
                    return None;
                }
            }
            Ok(Event::Eof) | Err(_) => return None,
            _ => {}
        }
        buf.clear();
    }
}

/// header.xml을 파싱하여 DocInfo와 DocProperties를 생성한다.
pub fn parse_hwpx_header(xml: &str) -> Result<(DocInfo, DocProperties), HwpxError> {
    let mut doc_info = DocInfo::default();
    let mut doc_props = DocProperties::default();

    let mut reader = Reader::from_str(xml);
    let mut buf = Vec::new();

    // 기본값: 7개 언어별 빈 글꼴 목록
    doc_info.font_faces = vec![Vec::new(); 7];

    // 현재 <fontface lang="..."> 컨텍스트 추적
    // HANGUL=0, LATIN=1, HANJA=2, JAPANESE=3, OTHER=4, SYMBOL=5, USER=6
    let mut current_font_group: usize = 0;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let name = e.name();
                let local = local_name(name.as_ref());
                match local {
                    b"fontface" => {
                        // <hh:fontface lang="HANGUL"> → 언어 그룹 설정
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"lang" {
                                current_font_group = match attr_str(&attr).as_str() {
                                    "HANGUL" => 0,
                                    "LATIN" => 1,
                                    "HANJA" => 2,
                                    "JAPANESE" => 3,
                                    "OTHER" => 4,
                                    "SYMBOL" => 5,
                                    "USER" => 6,
                                    _ => 0,
                                };
                            }
                        }
                    }
                    b"beginNum" => parse_begin_num(e, &mut doc_props),
                    b"font" => {
                        parse_font(e, &mut reader, &mut doc_info, current_font_group, true)?;
                    }
                    b"charPr" => {
                        parse_char_shape(e, &mut reader, &mut doc_info)?;
                    }
                    b"paraPr" => {
                        parse_para_shape(e, &mut reader, &mut doc_info)?;
                    }
                    b"style" => parse_style(e, &mut doc_info),
                    b"borderFill" => {
                        parse_border_fill(e, &mut reader, &mut doc_info)?;
                    }
                    b"tabPr" => {
                        parse_tab_def(e, &mut reader, &mut doc_info)?;
                    }
                    b"numbering" => {
                        parse_numbering(e, &mut reader, &mut doc_info, xml)?;
                    }
                    b"memoPr" => {
                        parse_memo_shape(e, &mut doc_info);
                    }
                    b"linkinfo" => {
                        parse_doc_option_linkinfo(e, &mut doc_info);
                    }
                    // [Task #1058 후속] BULLET 누락 시 한컴이 default 글머리표를 본문
                    // paragraph 에 자동 부여. HWPX `<hh:bullet id="N" char="❏" useImage="0">`
                    // 4개 → HWP BULLET record 4개. 누락 시 일반 문단 시작 글머리표 부작용.
                    b"bullet" => {
                        let bullet = parse_bullet_hwpx(e, &mut reader)?;
                        doc_info.bullets.push(bullet);
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(ref e)) => {
                let name = e.name();
                let local = local_name(name.as_ref());
                match local {
                    b"beginNum" => parse_begin_num(e, &mut doc_props),
                    b"font" => {
                        parse_font(e, &mut reader, &mut doc_info, current_font_group, false)?;
                    }
                    b"style" => parse_style(e, &mut doc_info),
                    b"tabPr" => {
                        // 자기 닫힘 태그: 빈 TabDef만 push
                        let mut td = TabDef::default();
                        for attr in e.attributes().flatten() {
                            match attr.key.as_ref() {
                                b"autoTabLeft" => td.auto_tab_left = attr_str(&attr) == "1",
                                b"autoTabRight" => td.auto_tab_right = attr_str(&attr) == "1",
                                _ => {}
                            }
                        }
                        doc_info.tab_defs.push(td);
                    }
                    b"memoPr" => {
                        parse_memo_shape(e, &mut doc_info);
                    }
                    b"linkinfo" => {
                        parse_doc_option_linkinfo(e, &mut doc_info);
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(HwpxError::XmlError(format!("header.xml: {}", e))),
            _ => {}
        }
        buf.clear();
    }

    doc_props.section_count = 1; // content.hpf에서 갱신됨

    // 문서 설정 tail(`</hh:refList>` ~ `</hh:head>`)을 원본 그대로 보존.
    // compatibleDocument/docOption/trackchageConfig 등은 본문과 무관한 전역
    // 설정이라 헤더 재생성 시 splice 로 무손실 복원한다.
    doc_info.hwpx_head_tail = extract_head_tail(xml);

    Ok((doc_info, doc_props))
}

/// HWPX 헤더 문자열에서 `</hh:refList>` 닫는 태그와 `</hh:head>` 사이 구간을
/// 그대로 추출한다(빈 구간이면 `Some("")` — 원본에 설정이 없었음을 보존).
/// 두 경계 마커가 없으면(비-HWPX/HWP5 경로) `None` 을 돌려 하드코딩 폴백.
fn extract_head_tail(xml: &str) -> Option<String> {
    const REF_END: &str = "</hh:refList>";
    const HEAD_END: &str = "</hh:head>";
    let start = xml.find(REF_END)? + REF_END.len();
    let end = xml[start..].find(HEAD_END)? + start;
    Some(xml[start..end].to_string())
}

fn parse_doc_option_linkinfo(e: &quick_xml::events::BytesStart, doc_info: &mut DocInfo) {
    let mut page_inherit = false;
    let mut footnote_inherit = false;

    for attr in e.attributes().flatten() {
        match attr.key.as_ref() {
            b"pageInherit" => page_inherit = parse_bool(&attr),
            b"footnoteInherit" => footnote_inherit = parse_bool(&attr),
            _ => {}
        }
    }

    let mut data = Vec::with_capacity(80);
    data.extend_from_slice(&0x021c_u16.to_le_bytes());
    data.extend_from_slice(&1_u16.to_le_bytes());
    data.extend_from_slice(&0_u16.to_le_bytes());
    data.extend_from_slice(&0x0207_u16.to_le_bytes());
    data.extend_from_slice(&0x8000_u16.to_le_bytes());
    data.extend_from_slice(&0x0207_u16.to_le_bytes());
    data.extend_from_slice(&8_u16.to_le_bytes());
    data.extend_from_slice(&0_u16.to_le_bytes());

    let items = [
        (0x400a_u16, 0x0006_u16, 0_u32),
        (0x400e_u16, 0x0006_u16, 0_u32),
        (0x4006_u16, 0x0006_u16, u32::from(page_inherit)),
        (0x4010_u16, 0x0007_u16, u32::from(footnote_inherit)),
        (0x401a_u16, 0x0006_u16, 0_u32),
        (0x401d_u16, 0x0006_u16, 0_u32),
        (0x401f_u16, 0x0007_u16, 100_u32),
        (0x4020_u16, 0x0007_u16, 100_u32),
    ];

    for (item_id, item_type, value) in items {
        data.extend_from_slice(&item_id.to_le_bytes());
        data.extend_from_slice(&item_type.to_le_bytes());
        data.extend_from_slice(&value.to_le_bytes());
    }

    doc_info.extra_records.push(RawRecord {
        tag_id: tags::HWPTAG_DOC_DATA,
        level: 0,
        data,
    });
    doc_info.extra_records.push(RawRecord {
        tag_id: tags::HWPTAG_FORBIDDEN_CHAR,
        level: 1,
        data: vec![0; 16],
    });

    doc_info.extra_records.push(RawRecord {
        tag_id: tags::HWPTAG_COMPATIBLE_DOCUMENT,
        level: 0,
        data: vec![0; 4],
    });
    doc_info.extra_records.push(RawRecord {
        tag_id: tags::HWPTAG_LAYOUT_COMPATIBILITY,
        level: 1,
        data: vec![0; 20],
    });

    let mut track_change = vec![0; 1032];
    track_change[..4].copy_from_slice(&56_u32.to_le_bytes());
    doc_info.extra_records.push(RawRecord {
        tag_id: tags::HWPTAG_TRACKCHANGE,
        level: 1,
        data: track_change,
    });
}

fn parse_memo_shape(e: &quick_xml::events::BytesStart, doc_info: &mut DocInfo) {
    let mut width = 0u32;
    let mut line_width = 0u8;
    let mut line_type = 0u8;
    let mut line_color = 0u32;
    let mut fill_color = 0u32;
    let mut active_color = 0u32;
    let mut memo_type = 0u32;

    for attr in e.attributes().flatten() {
        match attr.key.as_ref() {
            b"width" => width = parse_u32(&attr),
            b"lineWidth" => line_width = parse_u8(&attr),
            b"lineType" => line_type = parse_memo_line_type(&attr_str(&attr)),
            b"lineColor" => line_color = parse_color(&attr),
            b"fillColor" => fill_color = parse_color(&attr),
            b"activeColor" => active_color = parse_color(&attr),
            b"memoType" => memo_type = parse_memo_type(&attr_str(&attr)),
            _ => {}
        }
    }

    let mut data = Vec::with_capacity(22);
    data.extend_from_slice(&width.to_le_bytes());
    data.push(line_type);
    data.push(line_width);
    data.extend_from_slice(&line_color.to_le_bytes());
    data.extend_from_slice(&fill_color.to_le_bytes());
    data.extend_from_slice(&active_color.to_le_bytes());
    data.extend_from_slice(&memo_type.to_le_bytes());

    doc_info.extra_records.push(RawRecord {
        tag_id: tags::HWPTAG_MEMO_SHAPE,
        level: 1,
        data,
    });
    doc_info.memo_shape_count = doc_info
        .extra_records
        .iter()
        .filter(|record| record.tag_id == tags::HWPTAG_MEMO_SHAPE)
        .count() as u32;
}

fn parse_memo_line_type(value: &str) -> u8 {
    match value {
        // HWPX memoPr lineType uses the OWPML name, but HWP5 MEMO_SHAPE stores
        // the Hancom memo line enum where 0 means "no/unknown" and SOLID is 1.
        "SOLID" => 1,
        "DOT" => 1,
        "DASH_DOT" => 2,
        "DASH" => 3,
        "DASH_DOT_DOT" => 4,
        "LONG_DASH" => 5,
        "CIRCLE" => 6,
        "DOUBLE_SLIM" => 7,
        "SLIM_THICK" => 8,
        "THICK_SLIM" => 9,
        "SLIM_THICK_SLIM" => 10,
        "WAVE" => 11,
        "DOUBLE_WAVE" => 12,
        _ => 0,
    }
}

fn parse_memo_type(value: &str) -> u32 {
    match value {
        "NOMAL" | "NORMAL" | "" => 0,
        _ => 0,
    }
}

// ─── beginNum ───

fn parse_begin_num(e: &quick_xml::events::BytesStart, props: &mut DocProperties) {
    for attr in e.attributes().flatten() {
        match attr.key.as_ref() {
            b"page" => props.page_start_num = parse_u16(&attr),
            b"footnote" => props.footnote_start_num = parse_u16(&attr),
            b"endnote" => props.endnote_start_num = parse_u16(&attr),
            b"pic" => props.picture_start_num = parse_u16(&attr),
            b"tbl" => props.table_start_num = parse_u16(&attr),
            b"equation" => props.equation_start_num = parse_u16(&attr),
            _ => {}
        }
    }
}

// ─── Font ───

fn parse_font(
    e: &quick_xml::events::BytesStart,
    reader: &mut Reader<&[u8]>,
    doc_info: &mut DocInfo,
    font_group: usize,
    has_children: bool,
) -> Result<(), HwpxError> {
    let mut name = String::new();
    let mut font_type = 0u8;
    let mut is_embedded = false;
    let mut bin_item_id_ref = String::new();
    let mut type_info = None;
    let mut subst_font = None;

    for attr in e.attributes().flatten() {
        match attr.key.as_ref() {
            b"face" => name = attr_str(&attr),
            b"type" => {
                font_type = match attr_str(&attr).as_str() {
                    "TTF" => 1,
                    "HFT" => 2,
                    _ => 0,
                };
            }
            b"isEmbedded" => is_embedded = attr_str(&attr) == "1",
            b"binaryItemIDRef" => bin_item_id_ref = attr_str(&attr),
            _ => {}
        }
    }

    if has_children {
        let mut buf = Vec::new();
        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Empty(ref ce)) => match local_name(ce.name().as_ref()) {
                    b"typeInfo" => type_info = Some(parse_font_type_info(ce, &name, font_type)),
                    b"substFont" => subst_font = Some(parse_subst_font(ce)),
                    _ => {}
                },
                Ok(Event::Start(ref ce)) => {
                    if local_name(ce.name().as_ref()) == b"typeInfo" {
                        type_info = Some(parse_font_type_info(ce, &name, font_type));
                    } else {
                        let tag = local_name(ce.name().as_ref()).to_vec();
                        skip_element(reader, &tag)?;
                    }
                }
                Ok(Event::End(ref ce)) if local_name(ce.name().as_ref()) == b"font" => break,
                Ok(Event::Eof) => break,
                Err(e) => return Err(HwpxError::XmlError(e.to_string())),
                _ => {}
            }
            buf.clear();
        }
    }

    if !name.is_empty() {
        let default_name = hwp5_default_font_name(&name).map(str::to_string);
        let font = Font {
            name,
            alt_type: font_type,
            is_embedded,
            bin_item_id_ref,
            resolved_bin_data_id: None,
            type_info,
            default_name,
            subst_font,
            ..Default::default()
        };
        // fontface lang 컨텍스트에 따라 해당 언어 그룹에 추가
        if font_group < doc_info.font_faces.len() {
            doc_info.font_faces[font_group].push(font);
        }
    }

    Ok(())
}

fn parse_font_type_info(
    e: &quick_xml::events::BytesStart,
    font_name: &str,
    font_type: u8,
) -> [u8; 10] {
    let mut info = [0u8; 10];
    info[1] = synthesize_serif_type(font_name, font_type);

    for attr in e.attributes().flatten() {
        let value = attr_str(&attr);
        match attr.key.as_ref() {
            b"familyType" => info[0] = font_family_type_to_u8(&value),
            b"weight" => info[2] = value.parse::<u8>().unwrap_or(0),
            b"proportion" => info[3] = value.parse::<u8>().unwrap_or(0),
            b"contrast" => info[4] = value.parse::<u8>().unwrap_or(0),
            b"strokeVariation" => info[5] = value.parse::<u8>().unwrap_or(0),
            b"armStyle" => info[6] = value.parse::<u8>().unwrap_or(0),
            b"letterform" => info[7] = value.parse::<u8>().unwrap_or(0),
            b"midline" => info[8] = value.parse::<u8>().unwrap_or(0),
            b"xHeight" => info[9] = value.parse::<u8>().unwrap_or(0),
            _ => {}
        }
    }

    info
}

/// `<hh:substFont face="…" type="…" isEmbedded="…" binaryItemIDRef="…"/>` 4개
/// 속성을 보존한다. 부모 `<hh:font>` 와 별개의 type/임베드 정보를 가질 수 있다.
fn parse_subst_font(e: &quick_xml::events::BytesStart) -> SubstFont {
    let mut sf = SubstFont::default();
    for attr in e.attributes().flatten() {
        let value = attr_str(&attr);
        match attr.key.as_ref() {
            b"face" => sf.face = value,
            b"type" => {
                sf.font_type = match value.as_str() {
                    "TTF" => 1,
                    "HFT" => 2,
                    _ => 0,
                };
            }
            b"isEmbedded" => sf.is_embedded = value == "1",
            b"binaryItemIDRef" => sf.bin_item_id_ref = value,
            _ => {}
        }
    }
    sf
}

fn font_family_type_to_u8(value: &str) -> u8 {
    match value {
        "FCAT_MYUNGJO" => 1,
        "FCAT_GOTHIC" => 2,
        "FCAT_SSERIF" => 3,
        "FCAT_BRUSHSCRIPT" => 4,
        "FCAT_DECORATIVE" => 5,
        "FCAT_NONRECTMJ" => 6,
        "FCAT_NONRECTGT" => 7,
        _ => 0,
    }
}

fn synthesize_serif_type(font_name: &str, font_type: u8) -> u8 {
    if font_type == 2 {
        return 0;
    }

    match font_name {
        "굴림" => 11,
        name if name.contains("바탕") || name.contains("명조") => 3,
        _ => 0,
    }
}

fn hwp5_default_font_name(name: &str) -> Option<&'static str> {
    match name {
        "굴림" => Some("Gulim"),
        "한컴바탕" => Some("Haansoft Batang"),
        "함초롬바탕" => Some("HCR Batang"),
        "함초롬돋움" => Some("HCR Dotum"),
        "신명 견명조" => Some("Sinmyeong Gyeonmyeongjo"),
        "신명 디나루" => Some("Sinmyeong Dinaru"),
        "신명 신그래픽" => Some("Sinmyeong Singraphic"),
        "신명 신명조" => Some("Sinmyeong Sinmyeongjo"),
        "신명 중고딕" => Some("Sinmyeong JungGothic"),
        "신명 중명조" => Some("Sinmyeong Jungmyeongjo"),
        "신명 태고딕" => Some("Sinmyeong TaeGothic"),
        "신명 태그래픽" => Some("Sinmyeong TaeGraphic"),
        "한양견명조" => Some("HY GyeonMyeongJo"),
        "한양신명조" => Some("HY ShinMyungJo"),
        "한양중고딕" => Some("HY JungGothic"),
        "명조" => Some("Myeongjo"),
        _ => None,
    }
}

// ─── CharShape ───

fn parse_char_shape(
    e: &quick_xml::events::BytesStart,
    reader: &mut Reader<&[u8]>,
    doc_info: &mut DocInfo,
) -> Result<(), HwpxError> {
    let mut cs = CharShape::default();

    for attr in e.attributes().flatten() {
        match attr.key.as_ref() {
            b"height" => cs.base_size = parse_i32(&attr),
            b"textColor" => cs.text_color = parse_color(&attr),
            b"shadeColor" => cs.shade_color = parse_color(&attr),
            b"useFontSpace" => cs.use_font_space = parse_bool(&attr),
            b"useKerning" => cs.kerning = parse_bool(&attr),
            // symMark(강조점)은 현재 코퍼스에서 항상 "NONE" 이라 emphasis_dot 기본값
            // (NONE)과 일치 → 무손실. 비-NONE 값이 발견되면 별도 수집 필요.
            b"symMark" => {}
            b"borderFillIDRef" => cs.border_fill_id = parse_u16(&attr),
            _ => {}
        }
    }

    // 자식 요소 파싱
    if !is_empty_event(e) {
        let mut buf = Vec::new();
        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Empty(ref ce)) | Ok(Event::Start(ref ce)) => {
                    let cname = ce.name();
                    let local = local_name(cname.as_ref());
                    match local {
                        b"fontRef" => {
                            for attr in ce.attributes().flatten() {
                                let val = parse_u16(&attr);
                                match attr.key.as_ref() {
                                    b"hangul" => cs.font_ids[0] = val,
                                    b"latin" => cs.font_ids[1] = val,
                                    b"hanja" => cs.font_ids[2] = val,
                                    b"japanese" => cs.font_ids[3] = val,
                                    b"other" => cs.font_ids[4] = val,
                                    b"symbol" => cs.font_ids[5] = val,
                                    b"user" => cs.font_ids[6] = val,
                                    _ => {}
                                }
                            }
                        }
                        b"ratio" => {
                            for attr in ce.attributes().flatten() {
                                let val = parse_u8(&attr);
                                match attr.key.as_ref() {
                                    b"hangul" => cs.ratios[0] = val,
                                    b"latin" => cs.ratios[1] = val,
                                    b"hanja" => cs.ratios[2] = val,
                                    b"japanese" => cs.ratios[3] = val,
                                    b"other" => cs.ratios[4] = val,
                                    b"symbol" => cs.ratios[5] = val,
                                    b"user" => cs.ratios[6] = val,
                                    _ => {}
                                }
                            }
                        }
                        b"spacing" => {
                            for attr in ce.attributes().flatten() {
                                let val = parse_i8(&attr);
                                match attr.key.as_ref() {
                                    b"hangul" => cs.spacings[0] = val,
                                    b"latin" => cs.spacings[1] = val,
                                    b"hanja" => cs.spacings[2] = val,
                                    b"japanese" => cs.spacings[3] = val,
                                    b"other" => cs.spacings[4] = val,
                                    b"symbol" => cs.spacings[5] = val,
                                    b"user" => cs.spacings[6] = val,
                                    _ => {}
                                }
                            }
                        }
                        b"relSz" => {
                            for attr in ce.attributes().flatten() {
                                let val = parse_u8(&attr);
                                match attr.key.as_ref() {
                                    b"hangul" => cs.relative_sizes[0] = val,
                                    b"latin" => cs.relative_sizes[1] = val,
                                    b"hanja" => cs.relative_sizes[2] = val,
                                    b"japanese" => cs.relative_sizes[3] = val,
                                    b"other" => cs.relative_sizes[4] = val,
                                    b"symbol" => cs.relative_sizes[5] = val,
                                    b"user" => cs.relative_sizes[6] = val,
                                    _ => {}
                                }
                            }
                        }
                        b"offset" => {
                            for attr in ce.attributes().flatten() {
                                let val = parse_i8(&attr);
                                match attr.key.as_ref() {
                                    b"hangul" => cs.char_offsets[0] = val,
                                    b"latin" => cs.char_offsets[1] = val,
                                    b"hanja" => cs.char_offsets[2] = val,
                                    b"japanese" => cs.char_offsets[3] = val,
                                    b"other" => cs.char_offsets[4] = val,
                                    b"symbol" => cs.char_offsets[5] = val,
                                    b"user" => cs.char_offsets[6] = val,
                                    _ => {}
                                }
                            }
                        }
                        b"bold" => cs.bold = true,
                        b"italic" => cs.italic = true,
                        b"underline" => {
                            for attr in ce.attributes().flatten() {
                                match attr.key.as_ref() {
                                    b"type" => {
                                        cs.underline_type = match attr_str(&attr).as_str() {
                                            "BOTTOM" => UnderlineType::Bottom,
                                            "TOP" => UnderlineType::Top,
                                            _ => UnderlineType::None,
                                        };
                                    }
                                    b"color" => {
                                        cs.underline_color = parse_color(&attr);
                                    }
                                    b"shape" => {
                                        // 밑줄 모양 13종 (표 27 선 종류 + 물결선)
                                        cs.underline_shape = match attr_str(&attr).as_str() {
                                            "SOLID" => 0,
                                            "DASH" => 1,
                                            "DOT" => 2,
                                            "DASH_DOT" => 3,
                                            "DASH_DOT_DOT" => 4,
                                            "LONG_DASH" => 5,
                                            "CIRCLE" => 6,
                                            "DOUBLE_SLIM" => 7,
                                            "SLIM_THICK" => 8,
                                            "THICK_SLIM" => 9,
                                            "SLIM_THICK_SLIM" => 10,
                                            "WAVE" => 11,
                                            "DOUBLE_WAVE" => 12,
                                            _ => 0,
                                        };
                                    }
                                    _ => {}
                                }
                            }
                        }
                        b"strikeout" => {
                            for attr in ce.attributes().flatten() {
                                match attr.key.as_ref() {
                                    b"shape" => {
                                        let val = attr_str(&attr);
                                        // 화이트리스트 방식: 한컴이 실제 렌더링하는
                                        // OWPML LineSym2 값만 취소선으로 인정한다.
                                        // "NONE", "3D" 같은 placeholder 및 알 수 없는
                                        // 값은 fail-closed로 no-strike 처리.
                                        // is_real_strike_shape() 독스트링 참고.
                                        cs.strikethrough = is_real_strike_shape(&val);
                                        cs.strike_shape = match val.as_str() {
                                            "SOLID" => 0,
                                            "DASH" => 1,
                                            "DOT" => 2,
                                            "DASH_DOT" => 3,
                                            "DASH_DOT_DOT" => 4,
                                            "LONG_DASH" => 5,
                                            "CIRCLE" => 6,
                                            "DOUBLE_SLIM" => 7,
                                            "SLIM_THICK" => 8,
                                            "THICK_SLIM" => 9,
                                            "SLIM_THICK_SLIM" => 10,
                                            "WAVE" => 11,
                                            "DOUBLE_WAVE" => 12,
                                            _ => 0,
                                        };
                                    }
                                    b"color" => {
                                        cs.strike_color = parse_color(&attr);
                                    }
                                    _ => {}
                                }
                            }
                        }
                        b"outline" => {
                            for attr in ce.attributes().flatten() {
                                if attr.key.as_ref() == b"type" {
                                    let val = attr_str(&attr);
                                    cs.outline_type = match val.as_str() {
                                        "NONE" => 0,
                                        "SOLID" => 1,
                                        "DASH" => 2,
                                        "DOT" => 3,
                                        _ => 0,
                                    };
                                }
                            }
                        }
                        b"shadow" => {
                            for attr in ce.attributes().flatten() {
                                match attr.key.as_ref() {
                                    b"type" => {
                                        let val = attr_str(&attr);
                                        cs.shadow_type = match val.as_str() {
                                            "NONE" => 0,
                                            "DROP" | "CONTINUOUS" => 1,
                                            _ => 0,
                                        };
                                    }
                                    b"color" => cs.shadow_color = parse_color(&attr),
                                    b"offsetX" => cs.shadow_offset_x = parse_i8(&attr),
                                    b"offsetY" => cs.shadow_offset_y = parse_i8(&attr),
                                    _ => {}
                                }
                            }
                        }
                        b"emboss" => {
                            cs.attr |= 1 << 13;
                            cs.emboss = true;
                        }
                        b"engrave" => {
                            cs.attr |= 1 << 14;
                            cs.engrave = true;
                        }
                        b"supscript" => cs.superscript = true,
                        b"subscript" => cs.subscript = true,
                        _ => {}
                    }
                }
                Ok(Event::End(ref ee)) => {
                    let ename = ee.name();
                    if local_name(ename.as_ref()) == b"charPr" {
                        break;
                    }
                }
                Ok(Event::Eof) => break,
                Err(e) => return Err(HwpxError::XmlError(format!("charPr: {}", e))),
                _ => {}
            }
            buf.clear();
        }
    }

    doc_info.char_shapes.push(cs);
    Ok(())
}

// ─── ParaShape ───

fn parse_para_shape(
    e: &quick_xml::events::BytesStart,
    reader: &mut Reader<&[u8]>,
    doc_info: &mut DocInfo,
) -> Result<(), HwpxError> {
    let mut ps = ParaShape::default();
    // OWPML ParaShapeType의 snapToGrid 기본값은 true.
    ps.attr1 |= 1 << 8;

    for attr in e.attributes().flatten() {
        match attr.key.as_ref() {
            b"tabPrIDRef" => ps.tab_def_id = parse_u16(&attr),
            b"condense" => {
                let condense = parse_u32(&attr).min(75);
                ps.attr1 = (ps.attr1 & !(0x7f << 9)) | (condense << 9);
            }
            b"fontLineHeight" => {
                if parse_bool(&attr) {
                    ps.attr1 |= 1 << 22;
                } else {
                    ps.attr1 &= !(1 << 22);
                }
            }
            b"snapToGrid" => {
                if parse_bool(&attr) {
                    ps.attr1 |= 1 << 8;
                } else {
                    ps.attr1 &= !(1 << 8);
                }
            }
            _ => {}
        }
    }

    if !is_empty_event(e) {
        let mut buf = Vec::new();
        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Empty(ref ce)) => {
                    parse_para_shape_child(ce, &mut ps);
                }
                Ok(Event::Start(ref ce)) => {
                    match parse_para_shape_child(ce, &mut ps) {
                        ParaShapeChildKind::Margin => {
                            parse_para_shape_margin_children(reader, &mut ps)?;
                        }
                        ParaShapeChildKind::Switch => {
                            // <switch>/<case>/<default> 네임스페이스 분기 처리
                            // HwpUnitChar case를 우선 적용, 없으면 default 사용
                            parse_para_shape_switch(reader, &mut ps)?;
                        }
                        ParaShapeChildKind::Other => {}
                    }
                }
                Ok(Event::End(ref ee)) => {
                    let ename = ee.name();
                    if local_name(ename.as_ref()) == b"paraPr" {
                        break;
                    }
                }
                Ok(Event::Eof) => break,
                Err(e) => return Err(HwpxError::XmlError(format!("paraPr: {}", e))),
                _ => {}
            }
            buf.clear();
        }
    }

    // [Task #1058 후속] line_spacing_v2 (UINT32, 5.0.2.5 이상) 보정.
    // HWPX 미명시 시 line_spacing (INT32) 값과 동기화 — 한컴 정답지의 모든 ParaShape 가
    // line_spacing_v2 보유.
    //
    // 주의: attr1 bit 7 강제 set 은 부작용 (정답지가 0 인 ParaShape (예: ps[5]) 까지
    // 한컴 default 와 다르게 만들어 글머리표 등 부정합 trigger). 본 본질은 Style record 의
    // lang_id 필드 (Task #1058 의 Style 정정) 영역.
    if ps.line_spacing_v2 == 0 {
        ps.line_spacing_v2 = ps.line_spacing as u32;
    }

    doc_info.para_shapes.push(ps);
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParaShapeChildKind {
    Margin,
    Switch,
    Other,
}

fn parse_para_shape_child(
    ce: &quick_xml::events::BytesStart,
    ps: &mut ParaShape,
) -> ParaShapeChildKind {
    let cname = ce.name();
    let local = local_name(cname.as_ref());
    match local {
        b"align" => {
            for attr in ce.attributes().flatten() {
                match attr.key.as_ref() {
                    b"horizontal" => ps.alignment = parse_alignment(&attr),
                    b"vertical" => {
                        ps.attr1 = (ps.attr1 & !(0x03 << 20))
                            | (parse_vertical_alignment_bits(&attr) << 20);
                    }
                    _ => {}
                }
            }
            ParaShapeChildKind::Other
        }
        b"heading" => {
            for attr in ce.attributes().flatten() {
                match attr.key.as_ref() {
                    b"type" => {
                        let val = attr_str(&attr);
                        ps.head_type = match val.as_str() {
                            "OUTLINE" => HeadType::Outline,
                            "NUMBER" | "NUMBERING" => HeadType::Number,
                            "BULLET" => HeadType::Bullet,
                            _ => HeadType::None,
                        };
                    }
                    b"idRef" => ps.numbering_id = parse_u16(&attr),
                    b"level" => ps.para_level = parse_u8(&attr),
                    _ => {}
                }
            }
            ParaShapeChildKind::Other
        }
        b"margin" => {
            parse_para_shape_margin_attrs(ce, ps);
            ParaShapeChildKind::Margin
        }
        b"lineSpacing" => {
            for attr in ce.attributes().flatten() {
                match attr.key.as_ref() {
                    b"type" => {
                        let val = attr_str(&attr);
                        ps.line_spacing_type = match val.as_str() {
                            "PERCENT" => LineSpacingType::Percent,
                            "FIXED" => LineSpacingType::Fixed,
                            "SPACEONLY" | "SPACE_ONLY" => LineSpacingType::SpaceOnly,
                            "MINIMUM" | "AT_LEAST" => LineSpacingType::Minimum,
                            _ => LineSpacingType::Percent,
                        };
                    }
                    b"value" => ps.line_spacing = parse_i32(&attr),
                    _ => {}
                }
            }
            ParaShapeChildKind::Other
        }
        b"border" => {
            for attr in ce.attributes().flatten() {
                match attr.key.as_ref() {
                    b"borderFillIDRef" => ps.border_fill_id = parse_u16(&attr),
                    b"offsetLeft" => ps.border_spacing[0] = parse_i16(&attr),
                    b"offsetRight" => ps.border_spacing[1] = parse_i16(&attr),
                    b"offsetTop" => ps.border_spacing[2] = parse_i16(&attr),
                    b"offsetBottom" => ps.border_spacing[3] = parse_i16(&attr),
                    b"connect" => {
                        if parse_bool(&attr) {
                            ps.attr1 |= 1 << 28;
                        } else {
                            ps.attr1 &= !(1 << 28);
                        }
                    }
                    b"ignoreMargin" => {
                        if parse_bool(&attr) {
                            ps.attr1 |= 1 << 29;
                        } else {
                            ps.attr1 &= !(1 << 29);
                        }
                    }
                    _ => {}
                }
            }
            ParaShapeChildKind::Other
        }
        b"breakSetting" => {
            for attr in ce.attributes().flatten() {
                match attr.key.as_ref() {
                    b"breakLatinWord" => {
                        // [#1986] 값 3종(BREAK_WORD/KEEP_WORD/HYPHENATION) — 원문 보존.
                        // 미보존 시 직렬화가 KEEP_WORD 로 고정해 꼬리말·표셀 재계산
                        // 줄나눔이 바뀌고 레이아웃(페이지 수)이 갈린다.
                        ps.break_latin_word = Some(attr_str(&attr));
                    }
                    b"breakNonLatinWord" => {
                        // HWP5 ParaShape attr1 bit 7: non-Latin line-break unit.
                        //
                        // Do not force this bit globally. Some Hancom-exported
                        // HWPX paraPr records require it (for example master-page
                        // page-number AutoNumber paragraphs), while other records
                        // explicitly do not. Preserve the HWPX contract only when
                        // the attribute says so.
                        let value = attr_str(&attr);
                        if value == "KEEP_WORD" {
                            ps.attr1 |= 1 << 7;
                        } else {
                            ps.attr1 &= !(1 << 7);
                        }
                    }
                    // 문단 보호 4종 정본 = attr1 bit16-19 (HWP5 표 44 — HWP5 파서·편집
                    // 경로(ParaShapeMods)와 같은 비트). 종전 attr2 bit5-8 사수는 표 45
                    // autoSpaceKrNum(bit5)과 충돌했고 HWP5 저장(attr2 원시 기록)으로도 샜다.
                    b"widowOrphan" => {
                        if parse_bool(&attr) {
                            ps.attr1 |= 1 << 16;
                        }
                    }
                    b"keepWithNext" => {
                        if parse_bool(&attr) {
                            ps.attr1 |= 1 << 17;
                        }
                    }
                    b"keepLines" => {
                        if parse_bool(&attr) {
                            ps.attr1 |= 1 << 18;
                        }
                    }
                    b"pageBreakBefore" => {
                        if parse_bool(&attr) {
                            ps.attr1 |= 1 << 19;
                        }
                    }
                    _ => {}
                }
            }
            ParaShapeChildKind::Other
        }
        b"autoSpacing" => {
            // HWP5 표 45: attr2 bit4=한글·영어, bit5=한글·숫자 자동 간격 — 적용
            // (model/style.rs)·판독(formatting.rs) 경로와 같은 정본 비트.
            // (attr1 bit20-21 은 문단 세로 정렬 — <align vertical> 소관.)
            for attr in ce.attributes().flatten() {
                match attr.key.as_ref() {
                    b"eAsianEng" => {
                        if parse_bool(&attr) {
                            ps.attr2 |= 1 << 4;
                        } else {
                            ps.attr2 &= !(1 << 4);
                        }
                    }
                    b"eAsianNum" => {
                        if parse_bool(&attr) {
                            ps.attr2 |= 1 << 5;
                        } else {
                            ps.attr2 &= !(1 << 5);
                        }
                    }
                    _ => {}
                }
            }
            ParaShapeChildKind::Other
        }
        b"switch" => ParaShapeChildKind::Switch,
        _ => ParaShapeChildKind::Other,
    }
}

fn parse_para_shape_margin_attrs(ce: &quick_xml::events::BytesStart, ps: &mut ParaShape) {
    for attr in ce.attributes().flatten() {
        match attr.key.as_ref() {
            b"left" => ps.margin_left = parse_i32(&attr),
            b"right" => ps.margin_right = parse_i32(&attr),
            b"indent" => ps.indent = parse_i32(&attr),
            b"prev" => ps.spacing_before = parse_i32(&attr),
            b"next" => ps.spacing_after = parse_i32(&attr),
            _ => {}
        }
    }
}

fn parse_para_shape_margin_value_child(ce: &quick_xml::events::BytesStart, ps: &mut ParaShape) {
    let cname = ce.name();
    let local = local_name(cname.as_ref());
    if !matches!(local, b"intent" | b"left" | b"right" | b"prev" | b"next") {
        return;
    }

    for attr in ce.attributes().flatten() {
        if attr.key.as_ref() != b"value" {
            continue;
        }
        let value = parse_i32(&attr);
        match local {
            b"intent" => ps.indent = value,
            b"left" => ps.margin_left = value,
            b"right" => ps.margin_right = value,
            b"prev" => ps.spacing_before = value,
            b"next" => ps.spacing_after = value,
            _ => {}
        }
    }
}

fn parse_para_shape_margin_children(
    reader: &mut Reader<&[u8]>,
    ps: &mut ParaShape,
) -> Result<(), HwpxError> {
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Empty(ref ce)) | Ok(Event::Start(ref ce)) => {
                parse_para_shape_margin_value_child(ce, ps);
            }
            Ok(Event::End(ref ee)) => {
                let ename = ee.name();
                if local_name(ename.as_ref()) == b"margin" {
                    break;
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(HwpxError::XmlError(format!("paraPr margin: {}", e))),
            _ => {}
        }
        buf.clear();
    }
    Ok(())
}

/// `<switch>` 내부의 `<case>`/`<default>` 분기에서 margin, lineSpacing을 파싱.
/// HwpUnitChar 네임스페이스 case를 우선 적용한다.
fn parse_para_shape_switch(
    reader: &mut Reader<&[u8]>,
    ps: &mut ParaShape,
) -> Result<(), HwpxError> {
    let mut buf = Vec::new();
    let mut in_hwpunitchar_case = false;
    let mut in_default = false;
    let mut found_case = false;
    // default 값을 임시 저장 (case가 없을 때 폴백)
    let mut def_margin_left: Option<i32> = None;
    let mut def_margin_right: Option<i32> = None;
    let mut def_indent: Option<i32> = None;
    let mut def_prev: Option<i32> = None;
    let mut def_next: Option<i32> = None;
    let mut def_line_spacing_type: Option<LineSpacingType> = None;
    let mut def_line_spacing: Option<i32> = None;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref ce)) => {
                let cname = ce.name();
                let local = local_name(cname.as_ref());
                match local {
                    b"case" => {
                        // required-namespace 속성 확인
                        let is_hwpunitchar = ce.attributes().flatten().any(|attr| {
                            let val = attr_str(&attr);
                            val.contains("HwpUnitChar")
                        });
                        if is_hwpunitchar {
                            in_hwpunitchar_case = true;
                        }
                    }
                    b"default" => {
                        in_default = true;
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(ref ce)) => {
                let cname = ce.name();
                let local = local_name(cname.as_ref());
                if in_hwpunitchar_case || in_default {
                    match local {
                        b"margin" | b"intent" | b"left" | b"right" | b"prev" | b"next" => {
                            // margin 하위 요소들: <left value="..." />, <prev value="..." /> 등
                            let tag_name = local;
                            for attr in ce.attributes().flatten() {
                                if attr.key.as_ref() == b"value" {
                                    let val = parse_i32(&attr);
                                    if in_hwpunitchar_case {
                                        // HwpUnitChar 값은 실제 HWPUNIT(1× 스케일)이므로
                                        // HWP 바이너리와 동일한 2× 스케일로 변환
                                        let val2x = val * 2;
                                        match tag_name {
                                            b"left" => ps.margin_left = val2x,
                                            b"right" => ps.margin_right = val2x,
                                            b"intent" => ps.indent = val2x,
                                            b"prev" => ps.spacing_before = val2x,
                                            b"next" => ps.spacing_after = val2x,
                                            _ => {}
                                        }
                                        found_case = true;
                                    } else if in_default {
                                        match tag_name {
                                            b"left" => def_margin_left = Some(val),
                                            b"right" => def_margin_right = Some(val),
                                            b"intent" => def_indent = Some(val),
                                            b"prev" => def_prev = Some(val),
                                            b"next" => def_next = Some(val),
                                            _ => {}
                                        }
                                    }
                                }
                            }
                        }
                        b"lineSpacing" => {
                            let mut ls_type = None;
                            let mut ls_val = None;
                            for attr in ce.attributes().flatten() {
                                match attr.key.as_ref() {
                                    b"type" => {
                                        ls_type = Some(match attr_str(&attr).as_str() {
                                            "PERCENT" => LineSpacingType::Percent,
                                            "FIXED" => LineSpacingType::Fixed,
                                            "SPACEONLY" | "SPACE_ONLY" => {
                                                LineSpacingType::SpaceOnly
                                            }
                                            "MINIMUM" | "AT_LEAST" => LineSpacingType::Minimum,
                                            _ => LineSpacingType::Percent,
                                        });
                                    }
                                    b"value" => ls_val = Some(parse_i32(&attr)),
                                    _ => {}
                                }
                            }
                            if in_hwpunitchar_case {
                                if let Some(t) = ls_type {
                                    ps.line_spacing_type = t;
                                }
                                if let Some(v) = ls_val {
                                    // Fixed/SpaceOnly/Minimum은 HWPUNIT이므로 2× 스케일 변환
                                    let effective_type = ls_type.unwrap_or(ps.line_spacing_type);
                                    ps.line_spacing = match effective_type {
                                        LineSpacingType::Percent => v,
                                        _ => v * 2,
                                    };
                                }
                                found_case = true;
                            } else if in_default {
                                def_line_spacing_type = ls_type;
                                def_line_spacing = ls_val;
                            }
                        }
                        _ => {}
                    }
                }
            }
            Ok(Event::End(ref ee)) => {
                let ename = ee.name();
                let local = local_name(ename.as_ref());
                match local {
                    b"case" => {
                        in_hwpunitchar_case = false;
                    }
                    b"default" => {
                        in_default = false;
                    }
                    b"switch" => break,
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(HwpxError::XmlError(format!("switch: {}", e))),
            _ => {}
        }
        buf.clear();
    }

    // HwpUnitChar case가 없으면 default 값 적용
    if !found_case {
        if let Some(v) = def_margin_left {
            ps.margin_left = v;
        }
        if let Some(v) = def_margin_right {
            ps.margin_right = v;
        }
        if let Some(v) = def_indent {
            ps.indent = v;
        }
        if let Some(v) = def_prev {
            ps.spacing_before = v;
        }
        if let Some(v) = def_next {
            ps.spacing_after = v;
        }
        if let Some(t) = def_line_spacing_type {
            ps.line_spacing_type = t;
        }
        if let Some(v) = def_line_spacing {
            ps.line_spacing = v;
        }
    }

    Ok(())
}

// ─── Style ───

fn parse_style(e: &quick_xml::events::BytesStart, doc_info: &mut DocInfo) {
    let mut style = Style::default();
    style.lang_id = 1042; // default 한국어 (HWPX 미지정 시)
    for attr in e.attributes().flatten() {
        match attr.key.as_ref() {
            b"name" => style.local_name = attr_str(&attr),
            b"engName" => style.english_name = attr_str(&attr),
            b"type" => {
                let val = attr_str(&attr);
                style.style_type = match val.as_str() {
                    "PARA" | "PARAGRAPH" => 0,
                    "CHAR" | "CHARACTER" => 1,
                    _ => 0,
                };
            }
            b"paraPrIDRef" => style.para_shape_id = parse_u16(&attr),
            b"charPrIDRef" => style.char_shape_id = parse_u16(&attr),
            b"nextStyleIDRef" => style.next_style_id = parse_u8(&attr),
            // [Task #1058 후속] HWPX `langID` → Style.lang_id (spec 표 47).
            // HWPX 의 `langID="1042"` 가 한컴 정답지의 Style record 의 INT16 lang_id.
            b"langID" => {
                if let Ok(s) = std::str::from_utf8(&attr.value) {
                    if let Ok(v) = s.parse::<i16>() {
                        style.lang_id = v;
                    }
                }
            }
            _ => {}
        }
    }
    doc_info.styles.push(style);
}

// ─── BorderFill ───

fn parse_border_fill(
    e: &quick_xml::events::BytesStart,
    reader: &mut Reader<&[u8]>,
    doc_info: &mut DocInfo,
) -> Result<(), HwpxError> {
    let mut bf = BorderFill::default();
    for attr in e.attributes().flatten() {
        if attr.key.as_ref() == b"centerLine" {
            bf.center_line = parse_center_line(&attr);
            if bf.center_line == CenterLine::None {
                bf.attr &= !(1 << 13);
            } else {
                bf.attr |= 1 << 13;
            }
        }
    }

    if !is_empty_event(e) {
        let mut buf = Vec::new();
        let mut border_idx = 0usize;
        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Empty(ref ce)) | Ok(Event::Start(ref ce)) => {
                    let cname = ce.name();
                    let local = local_name(cname.as_ref());
                    match local {
                        b"leftBorder" | b"rightBorder" | b"topBorder" | b"bottomBorder" => {
                            let idx = match local {
                                b"leftBorder" => 0,
                                b"rightBorder" => 1,
                                b"topBorder" => 2,
                                b"bottomBorder" => 3,
                                _ => {
                                    border_idx += 1;
                                    border_idx - 1
                                }
                            };
                            if idx < 4 {
                                for attr in ce.attributes().flatten() {
                                    match attr.key.as_ref() {
                                        b"type" => {
                                            bf.borders[idx].line_type =
                                                parse_border_line_type(&attr)
                                        }
                                        b"width" => {
                                            bf.borders[idx].width = parse_border_width(&attr)
                                        }
                                        b"color" => bf.borders[idx].color = parse_color(&attr),
                                        _ => {}
                                    }
                                }
                            }
                        }
                        b"diagonal" => {
                            for attr in ce.attributes().flatten() {
                                match attr.key.as_ref() {
                                    b"type" => {
                                        bf.diagonal.diagonal_type =
                                            parse_border_line_type_code(&attr)
                                    }
                                    b"width" => bf.diagonal.width = parse_border_width(&attr),
                                    b"color" => bf.diagonal.color = parse_color(&attr),
                                    _ => {}
                                }
                            }
                        }
                        b"fillBrush" => {
                            // fillBrush 자식 요소를 파싱
                            // Start 이벤트이면 자식을 읽어야 함
                        }
                        b"winBrush" => {
                            bf.fill.fill_type = FillType::Solid;
                            let mut solid = SolidFill {
                                pattern_type: -1,
                                ..SolidFill::default()
                            };
                            // [Issue #1172] faceColor="none" 은 "배경 채우기 없음" 을 뜻한다.
                            // 무늬(hatchStyle) 도 없으면 이 winBrush 는 빈 채우기이므로
                            // FillType::None 으로 둔다. 종전엔 winBrush 존재만으로 무조건
                            // Solid 로 처리해 faceColor="none" 배경이 흰색 Solid 로 잘못
                            // 해석되어, 문단모양/렌더에 의도치 않은 배경이 생겼다.
                            let mut face_is_none = false;
                            for attr in ce.attributes().flatten() {
                                match attr.key.as_ref() {
                                    b"faceColor" => {
                                        if attr_str(&attr).eq_ignore_ascii_case("none") {
                                            face_is_none = true;
                                        }
                                        solid.background_color = parse_color(&attr);
                                    }
                                    b"hatchColor" => solid.pattern_color = parse_color(&attr),
                                    b"hatchStyle" => {
                                        if let Some(pattern_type) =
                                            parse_hatch_style(&attr_str(&attr))
                                        {
                                            solid.pattern_type = pattern_type;
                                        }
                                    }
                                    b"alpha" => {
                                        // HWPX alpha: 0.0=완전투명 ~ 1.0=불투명 (float string)
                                        let val = attr_str(&attr);
                                        if let Ok(f) = val.parse::<f64>() {
                                            bf.fill.alpha = (f.clamp(0.0, 1.0) * 255.0) as u8;
                                        }
                                    }
                                    _ => {}
                                }
                            }
                            // faceColor=none + 무늬 없음 → 렌더는 "채우기 없음"(FillType::None).
                            // 단, winBrush 요소 자체는 원본에 존재하므로 solid 데이터를
                            // 항상 보존해 직렬화 때 그대로 복원한다(round-trip 무손실).
                            // 의미 분리: fill_type 은 "어떻게 렌더되는가"(None=빈 채우기),
                            // solid.is_some() 는 "winBrush 요소가 원본에 있었는가". 렌더 소비자는
                            // fill_type==Solid 로만 채우기를 그리므로 None+solid 조합은 렌더상
                            // 무채움이며, 동시에 직렬화기는 solid 로 winBrush 를 되살린다.
                            if face_is_none && solid.pattern_type < 0 {
                                bf.fill.fill_type = FillType::None;
                            }
                            bf.fill.solid = Some(solid);
                        }
                        b"gradation" => {
                            bf.fill.fill_type = FillType::Gradient;
                            let mut grad = GradientFill::default();
                            for attr in ce.attributes().flatten() {
                                match attr.key.as_ref() {
                                    b"type" => {
                                        grad.gradient_type = parse_gradient_type(&attr_str(&attr))
                                    }
                                    b"angle" => grad.angle = parse_i16(&attr),
                                    b"centerX" => grad.center_x = parse_i16(&attr),
                                    b"centerY" => grad.center_y = parse_i16(&attr),
                                    b"blur" | b"step" => grad.blur = parse_i16(&attr),
                                    b"stepCenter" => grad.step_center = parse_u8(&attr),
                                    b"alpha" => {
                                        let val = attr_str(&attr);
                                        if let Ok(f) = val.parse::<f64>() {
                                            bf.fill.alpha = (f.clamp(0.0, 1.0) * 255.0) as u8;
                                        }
                                    }
                                    _ => {}
                                }
                            }
                            bf.fill.gradient = Some(grad);
                        }
                        b"color" => {
                            // <hh:color value="#RRGGBB"/> — gradation 자식
                            if let Some(ref mut grad) = bf.fill.gradient {
                                for attr in ce.attributes().flatten() {
                                    if attr.key.as_ref() == b"value" {
                                        grad.colors.push(parse_color(&attr));
                                    }
                                }
                            }
                        }
                        b"imgBrush" => {
                            bf.fill.fill_type = FillType::Image;
                            let mut img_fill = ImageFill::default();
                            for attr in ce.attributes().flatten() {
                                match attr.key.as_ref() {
                                    b"mode" => {
                                        img_fill.fill_mode = match attr_str(&attr).as_str() {
                                            "TILE" | "TILE_ALL" => ImageFillMode::TileAll,
                                            "TILE_HORZ_TOP" => ImageFillMode::TileHorzTop,
                                            "TILE_HORZ_BOTTOM" => ImageFillMode::TileHorzBottom,
                                            "TILE_VERT_LEFT" => ImageFillMode::TileVertLeft,
                                            "TILE_VERT_RIGHT" => ImageFillMode::TileVertRight,
                                            "CENTER" => ImageFillMode::Center,
                                            "CENTER_TOP" => ImageFillMode::CenterTop,
                                            "CENTER_BOTTOM" => ImageFillMode::CenterBottom,
                                            "FIT" | "FIT_TO_SIZE" | "STRETCH" => {
                                                ImageFillMode::FitToSize
                                            }
                                            "TOTAL" => ImageFillMode::Total,
                                            "TOP_LEFT_ALIGN" => ImageFillMode::LeftTop,
                                            _ => ImageFillMode::TileAll,
                                        };
                                    }
                                    b"bright" => img_fill.brightness = parse_i8(&attr),
                                    b"contrast" => img_fill.contrast = parse_i8(&attr),
                                    _ => {}
                                }
                            }
                            bf.fill.image = Some(img_fill);
                        }
                        b"img" | b"image" => {
                            // imgBrush 내부의 이미지 참조.
                            // [Issue #1156] 쪽 테두리/배경 그림의 "워터마크 효과" 는
                            // <hc:img> 의 bright/contrast/effect 로 표현된다 (한컴 UI:
                            // 쪽 테두리/배경 > 그림 > 워터마크 효과). 종전에는
                            // binaryItemIDRef 만 읽어 bright/contrast/effect 가 손실되어
                            // 배경 워터마크 반투명 합성이 빠졌다 (SVG/PNG 회귀).
                            if let Some(ref mut img_fill) = bf.fill.image {
                                for attr in ce.attributes().flatten() {
                                    match attr.key.as_ref() {
                                        b"binaryItemIDRef" => {
                                            let val = attr_str(&attr);
                                            let num: String = val
                                                .chars()
                                                .filter(|c| c.is_ascii_digit())
                                                .collect();
                                            img_fill.bin_data_id = num.parse().unwrap_or(0);
                                        }
                                        b"bright" => img_fill.brightness = parse_i8(&attr),
                                        b"contrast" => img_fill.contrast = parse_i8(&attr),
                                        b"effect" => {
                                            img_fill.effect = match attr_str(&attr).as_str() {
                                                "GRAY_SCALE" => 1,
                                                "BLACK_WHITE" => 2,
                                                _ => 0, // REAL_PIC
                                            };
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                        b"slash" => {
                            // slash/backSlash 의 type 은 대각선 "방향/형태" enum 이며
                            // 선 종류가 아니다. 방향 비트(attr bits 2~4)만 설정하고,
                            // 선 종류/굵기/색은 <hh:diagonal> 요소가 전담한다.
                            for attr in ce.attributes().flatten() {
                                match attr.key.as_ref() {
                                    b"type" => {
                                        let code = parse_slash_shape_code(&attr);
                                        set_diagonal_attr_bits(&mut bf, 2, code);
                                    }
                                    b"Crooked" => set_diagonal_attr_field(
                                        &mut bf.attr,
                                        8,
                                        0x03,
                                        parse_u16(&attr),
                                    ),
                                    b"isCounter" => set_bit(&mut bf.attr, 11, parse_bool(&attr)),
                                    _ => {}
                                }
                            }
                        }
                        b"backSlash" => {
                            // backSlash 방향 비트(attr bits 5~7)만 설정.
                            for attr in ce.attributes().flatten() {
                                match attr.key.as_ref() {
                                    b"type" => {
                                        let code = parse_slash_shape_code(&attr);
                                        set_diagonal_attr_bits(&mut bf, 5, code);
                                    }
                                    b"Crooked" => set_bit(&mut bf.attr, 10, parse_bool(&attr)),
                                    b"isCounter" => set_bit(&mut bf.attr, 12, parse_bool(&attr)),
                                    _ => {}
                                }
                            }
                        }
                        _ => {}
                    }
                }
                Ok(Event::End(ref ee)) => {
                    let ename = ee.name();
                    if local_name(ename.as_ref()) == b"borderFill" {
                        break;
                    }
                }
                Ok(Event::Eof) => break,
                Err(e) => return Err(HwpxError::XmlError(format!("borderFill: {}", e))),
                _ => {}
            }
            buf.clear();
        }
    }

    doc_info.border_fills.push(bf);
    Ok(())
}

// ─── TabDef ───

fn parse_tab_item(ce: &quick_xml::events::BytesStart) -> TabItem {
    let mut item = TabItem::default();
    for attr in ce.attributes().flatten() {
        match attr.key.as_ref() {
            b"pos" => item.position = parse_u32(&attr),
            b"type" => {
                item.tab_type = match attr_str(&attr).as_str() {
                    "LEFT" => 0,
                    "RIGHT" => 1,
                    "CENTER" => 2,
                    "DECIMAL" => 3,
                    _ => 0,
                };
            }
            b"leader" => {
                // HWP fill_type: 0=없음, 1=실선, 2=파선, 3=점선,
                // 4=일점쇄선, 5=이점쇄선, 6=긴파선, 7=원형점선,
                // 8=이중실선, 9=얇고굵은이중선, 10=굵고얇은이중선, 11=삼중선
                // HWPML leader 명칭은 HWP 바이너리 fill_type과 직접 대응
                // "DASH"=점선(3), "DOT"=파선(2) — HWPML 명명이 직관과 반대
                item.fill_type = match attr_str(&attr).as_str() {
                    "NONE" => 0,
                    "SOLID" => 1,
                    "DOT" => 2,          // 파선
                    "DASH" => 3,         // 점선
                    "DASH_DOT" => 4,     // 일점쇄선
                    "DASH_DOT_DOT" => 5, // 이점쇄선
                    "LONG_DASH" => 6,    // 긴파선
                    "CIRCLE" => 7,       // 원형점선
                    "DOUBLE_LINE" => 8,  // 이중실선
                    "THIN_THICK" => 9,   // 얇고 굵은 이중선
                    "THICK_THIN" => 10,  // 굵고 얇은 이중선
                    "TRIM" => 11,        // 얇고 굵고 얇은 삼중선
                    _ => 0,
                };
            }
            _ => {}
        }
    }
    item
}

fn parse_tab_def(
    e: &quick_xml::events::BytesStart,
    reader: &mut Reader<&[u8]>,
    doc_info: &mut DocInfo,
) -> Result<(), HwpxError> {
    let mut td = TabDef::default();

    for attr in e.attributes().flatten() {
        match attr.key.as_ref() {
            b"autoTabLeft" => td.auto_tab_left = attr_str(&attr) == "1",
            b"autoTabRight" => td.auto_tab_right = attr_str(&attr) == "1",
            _ => {}
        }
    }

    if !is_empty_event(e) {
        let mut buf = Vec::new();
        let mut in_hwpunitchar_case = false;
        let mut in_default = false;
        let mut found_case = false;
        let mut default_tabs: Vec<TabItem> = Vec::new();
        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(ref ce)) => {
                    let cname = ce.name();
                    let local = local_name(cname.as_ref());
                    match local {
                        b"case" => {
                            let is_hwpunitchar = ce
                                .attributes()
                                .flatten()
                                .any(|attr| attr_str(&attr).contains("HwpUnitChar"));
                            if is_hwpunitchar {
                                in_hwpunitchar_case = true;
                            }
                        }
                        b"default" => {
                            in_default = true;
                        }
                        _ => {}
                    }
                }
                Ok(Event::Empty(ref ce)) => {
                    let cname = ce.name();
                    let local = local_name(cname.as_ref());
                    if local == b"tabItem" {
                        let mut item = parse_tab_item(ce);
                        if in_hwpunitchar_case {
                            // HwpUnitChar 값은 실제 HWPUNIT(1× 스케일)이므로
                            // HWP 바이너리와 동일한 2× 스케일로 변환
                            item.position *= 2;
                            td.tabs.push(item);
                            found_case = true;
                        } else if in_default {
                            // default 값은 이미 2× 스케일
                            default_tabs.push(item);
                        } else {
                            // switch 바깥의 직접 tabItem (단위 불명, 그대로 사용)
                            td.tabs.push(item);
                        }
                    }
                }
                Ok(Event::End(ref ee)) => {
                    let ename = ee.name();
                    let local = local_name(ename.as_ref());
                    match local {
                        b"case" => {
                            in_hwpunitchar_case = false;
                        }
                        b"default" => {
                            in_default = false;
                        }
                        b"tabPr" => break,
                        _ => {}
                    }
                }
                Ok(Event::Eof) => break,
                Err(e) => return Err(HwpxError::XmlError(format!("tabPr: {}", e))),
                _ => {}
            }
            buf.clear();
        }
        // HwpUnitChar case가 없으면 default 값 적용
        if !found_case && !default_tabs.is_empty() {
            td.tabs = default_tabs;
        }
    }

    doc_info.tab_defs.push(td);
    Ok(())
}

// ─── Numbering ───

/// [Task #1058 후속] `<hh:bullet>` 파싱 — HWP BULLET record (표 44).
/// HWPX 의 4개 bullet 정의를 IR 의 bullets 에 매핑. 누락 시 한컴이 default 글머리표를
/// 본문 paragraph 에 자동 부여 (글머리표 부작용).
fn parse_bullet_hwpx(
    e: &quick_xml::events::BytesStart,
    reader: &mut Reader<&[u8]>,
) -> Result<Bullet, HwpxError> {
    let mut bullet = Bullet::default();

    for attr in e.attributes().flatten() {
        match attr.key.as_ref() {
            b"char" => {
                if let Ok(s) = std::str::from_utf8(&attr.value) {
                    if let Some(c) = s.chars().next() {
                        bullet.bullet_char = c;
                    }
                }
            }
            b"useImage" => {
                if let Ok(s) = std::str::from_utf8(&attr.value) {
                    if s == "1" {
                        bullet.image_bullet = 1;
                    }
                }
            }
            _ => {}
        }
    }

    // 자식 <hh:paraHead>, <hh:image> 등 skip
    if !is_empty_event(e) {
        let mut buf = Vec::new();
        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::End(ref ee)) => {
                    if local_name(ee.name().as_ref()) == b"bullet" {
                        break;
                    }
                }
                Ok(Event::Eof) => break,
                Err(e) => return Err(HwpxError::XmlError(format!("bullet: {}", e))),
                _ => {}
            }
            buf.clear();
        }
    }

    Ok(bullet)
}

fn parse_numbering(
    e: &quick_xml::events::BytesStart,
    reader: &mut Reader<&[u8]>,
    doc_info: &mut DocInfo,
    xml: &str,
) -> Result<(), HwpxError> {
    let mut num = Numbering::default();

    for attr in e.attributes().flatten() {
        if attr.key.as_ref() == b"start" {
            num.start_number = parse_u16(&attr);
        }
    }

    if !is_empty_event(e) {
        // 여는 태그 `<hh:numbering ...>` 직후 = 자식 paraHead 영역 시작 오프셋.
        // reader 는 Reader::from_str(xml) 이므로 buffer_position 이 곧 xml 바이트
        // 오프셋이다. 닫는 태그 직전 위치(이번 이터레이션 read 직전 오프셋)를
        // 끝으로 잡아 닫는 태그 길이 계산 없이 byte-exact 구간을 얻는다.
        let inner_start = reader.buffer_position() as usize;
        let mut inner_end = inner_start;
        let mut buf = Vec::new();
        loop {
            let pos_before = reader.buffer_position() as usize;
            match reader.read_event_into(&mut buf) {
                Ok(Event::Empty(ref ce)) => {
                    let cname = ce.name();
                    let local = local_name(cname.as_ref());
                    if local == b"paraHead" {
                        let (level, head, start, format_str) = parse_numbering_para_head_attrs(ce);
                        apply_numbering_para_head(&mut num, level, head, start, format_str);
                    }
                }
                Ok(Event::Start(ref ce)) => {
                    let cname = ce.name();
                    let local = local_name(cname.as_ref());
                    if local == b"paraHead" {
                        let (level, head, start, mut format_str) =
                            parse_numbering_para_head_attrs(ce);
                        format_str.push_str(&read_numbering_para_head_text(reader)?);
                        apply_numbering_para_head(&mut num, level, head, start, format_str);
                    }
                }
                Ok(Event::End(ref ee)) => {
                    let ename = ee.name();
                    if local_name(ename.as_ref()) == b"numbering" {
                        inner_end = pos_before;
                        break;
                    }
                }
                Ok(Event::Eof) => {
                    inner_end = pos_before;
                    break;
                }
                Err(e) => return Err(HwpxError::XmlError(format!("numbering: {}", e))),
                _ => {}
            }
            buf.clear();
        }

        // 무손실 splice 용 원본 paraHead 구간 보존(7수준 모델로 표현 못하는
        // 8~10수준/checkable/형식문자열 포함). 경계가 깨졌으면 보존하지 않는다.
        if inner_end >= inner_start && inner_end <= xml.len() {
            num.raw_para_heads = Some(xml[inner_start..inner_end].to_string());
        }
    }

    doc_info.numberings.push(num);
    Ok(())
}

fn parse_numbering_para_head_attrs(
    e: &quick_xml::events::BytesStart,
) -> (usize, NumberingHead, Option<u32>, String) {
    let mut level: usize = 0;
    let mut start: Option<u32> = None;
    let mut head = NumberingHead::default();
    let mut format_str = String::new();

    for attr in e.attributes().flatten() {
        match attr.key.as_ref() {
            b"level" => level = parse_u32(&attr) as usize,
            b"start" => start = Some(parse_u32(&attr)),
            b"text" => format_str = attr_str(&attr),
            b"numFormat" => {
                head.number_format = parse_numbering_format_code(&attr_str(&attr));
                head.attr = (head.attr & !(0x0f << 5)) | ((head.number_format as u32) << 5);
            }
            b"charPrIDRef" => head.char_shape_id = parse_u32(&attr),
            b"widthAdjust" => head.width_adjust = parse_i16(&attr),
            b"textOffset" => head.text_distance = parse_i16(&attr),
            _ => {}
        }
    }

    (level, head, start, format_str)
}

fn read_numbering_para_head_text(reader: &mut Reader<&[u8]>) -> Result<String, HwpxError> {
    let mut buf = Vec::new();
    let mut text = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Text(ref t)) => {
                text.push_str(&t.decode().unwrap_or_default());
            }
            Ok(Event::CData(ref t)) => {
                text.push_str(&String::from_utf8_lossy(t.as_ref()));
            }
            Ok(Event::End(ref ee)) => {
                if local_name(ee.name().as_ref()) == b"paraHead" {
                    break;
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(HwpxError::XmlError(format!("numbering paraHead: {}", e))),
            _ => {}
        }
        buf.clear();
    }

    Ok(text)
}

fn apply_numbering_para_head(
    num: &mut Numbering,
    level: usize,
    head: NumberingHead,
    start: Option<u32>,
    format_str: String,
) {
    if !(1..=7).contains(&level) {
        return;
    }

    let idx = level - 1;
    num.heads[idx] = head;
    if let Some(start) = start {
        num.level_start_numbers[idx] = start;
    }
    num.level_formats[idx] = format_str;
}

fn parse_numbering_format_code(value: &str) -> u8 {
    match value {
        "DIGIT" | "ARABIC" => 0,
        "CIRCLED_DIGIT" => 1,
        "ROMAN_CAPITAL" | "ROMAN_UPPER" | "ROMAN" => 2,
        "ROMAN_SMALL" | "ROMAN_LOWER" => 3,
        "LATIN_CAPITAL" | "LATIN_UPPER" | "ALPHA_CAPITAL" => 4,
        "LATIN_SMALL" | "LATIN_LOWER" | "ALPHA_SMALL" => 5,
        "HANGUL_SYLLABLE" | "HANGUL_JAMO" => 8,
        "HANGUL_NUMBER" => 12,
        "HANJA_NUMBER" | "IDEOGRAPH" => 13,
        _ => value.parse().unwrap_or(0),
    }
}

// ─── 유틸리티 함수 (header 전용) ───

fn is_empty_event(_e: &quick_xml::events::BytesStart) -> bool {
    // quick-xml의 Event::Empty vs Event::Start 구분으로 판단
    // 호출측에서 Empty/Start 구분 없이 패턴 매칭하므로 항상 false 반환
    // (자식 파싱 루프가 End 태그에서 break하므로 안전)
    false
}

fn parse_alignment(attr: &quick_xml::events::attributes::Attribute) -> Alignment {
    match attr_str(attr).as_str() {
        "JUSTIFY" => Alignment::Justify,
        "LEFT" => Alignment::Left,
        "RIGHT" => Alignment::Right,
        "CENTER" => Alignment::Center,
        "DISTRIBUTE" => Alignment::Distribute,
        "DISTRIBUTE_SPACE" => Alignment::Justify,
        _ => Alignment::Justify,
    }
}

fn parse_vertical_alignment_bits(attr: &quick_xml::events::attributes::Attribute) -> u32 {
    match attr_str(attr).as_str() {
        "TOP" => 1,
        "CENTER" => 2,
        "BOTTOM" => 3,
        "BASELINE" => 0,
        _ => 0,
    }
}

fn parse_border_line_type(attr: &quick_xml::events::attributes::Attribute) -> BorderLineType {
    match attr_str(attr).as_str() {
        "NONE" => BorderLineType::None,
        "SOLID" => BorderLineType::Solid,
        "DASH" => BorderLineType::Dash,
        "DOT" => BorderLineType::Dot,
        "DASH_DOT" => BorderLineType::DashDot,
        "DASH_DOT_DOT" => BorderLineType::DashDotDot,
        "LONG_DASH" => BorderLineType::LongDash,
        "CIRCLE" => BorderLineType::Circle,
        "DOUBLE_SLIM" | "DOUBLE" => BorderLineType::Double,
        "SLIM_THICK" => BorderLineType::ThinThickDouble,
        "THICK_SLIM" => BorderLineType::ThickThinDouble,
        "SLIM_THICK_SLIM" => BorderLineType::ThinThickThinTriple,
        "WAVE" => BorderLineType::Wave,
        "DOUBLE_WAVE" => BorderLineType::DoubleWave,
        _ => BorderLineType::Solid,
    }
}

fn parse_border_line_type_code(attr: &quick_xml::events::attributes::Attribute) -> u8 {
    match parse_border_line_type(attr) {
        BorderLineType::None => 0,
        BorderLineType::Solid => 1,
        BorderLineType::Dash => 2,
        BorderLineType::Dot => 3,
        BorderLineType::DashDot => 4,
        BorderLineType::DashDotDot => 5,
        BorderLineType::LongDash => 6,
        BorderLineType::Circle => 7,
        BorderLineType::Double => 8,
        BorderLineType::ThinThickDouble => 9,
        BorderLineType::ThickThinDouble => 10,
        BorderLineType::ThinThickThinTriple => 11,
        BorderLineType::Wave => 12,
        BorderLineType::DoubleWave => 13,
        BorderLineType::Thick3D => 14,
        BorderLineType::Thick3DReverse => 15,
        BorderLineType::Thin3D => 16,
        BorderLineType::Thin3DReverse => 17,
    }
}

/// HWPX slash/backSlash 의 형태(type) enum 을 HWP5 BORDER_FILL attr 의
/// 3비트 방향 코드로 변환한다. (선 종류가 아니라 대각선 방향/형태)
///
/// | HWPX type     | 3비트 | 의미              |
/// |---------------|-------|-------------------|
/// | NONE          | 0     | 없음              |
/// | CENTER        | 0b010 | 단순 슬래시       |
/// | CENTER_BELOW  | 0b011 | 가운데 + 아래     |
/// | CENTER_ABOVE  | 0b110 | 가운데 + 위       |
/// | 기타/ALL      | 0b111 | 3방향             |
fn parse_slash_shape_code(attr: &quick_xml::events::attributes::Attribute) -> u8 {
    match attr_str(attr).as_str() {
        "NONE" => 0,
        "CENTER" => 0b010,
        "CENTER_BELOW" => 0b011,
        "CENTER_ABOVE" => 0b110,
        _ => 0b111,
    }
}

fn parse_center_line(attr: &quick_xml::events::attributes::Attribute) -> CenterLine {
    CenterLine::from_hwpx(&attr_str(attr))
}

/// HWP5 BORDER_FILL attr 의 대각선 방향 비트 필드를 설정한다.
/// slash 는 shift=2, backSlash 는 shift=5 위치의 3비트.
/// `code` 는 [`parse_slash_shape_code`] 가 반환한 3비트 방향 코드.
fn set_diagonal_attr_bits(bf: &mut BorderFill, shift: u16, code: u8) {
    let mask = 0x07u16 << shift;
    bf.attr &= !mask;
    bf.attr |= ((code as u16) & 0x07) << shift;
}

fn set_diagonal_attr_field(attr: &mut u16, shift: u16, mask_value: u16, value: u16) {
    let mask = mask_value << shift;
    *attr &= !mask;
    *attr |= (value & mask_value) << shift;
}

fn set_bit(attr: &mut u16, bit: u16, enabled: bool) {
    let mask = 1u16 << bit;
    if enabled {
        *attr |= mask;
    } else {
        *attr &= !mask;
    }
}

fn parse_border_width(attr: &quick_xml::events::attributes::Attribute) -> u8 {
    let s = attr_str(attr);
    // "0.12 mm", "0.4 mm" 등에서 mm 값을 뽑아 한컴 표준 16단계 굵기 index 로 최근접
    // 매핑한다. 직렬화기 border_width_mm 과 동일한 BORDER_WIDTHS 테이블을 공유해야
    // 라운드트립이 무손실이다.
    let mm: f64 = s
        .split_whitespace()
        .next()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.1);
    border_width_index(mm)
}
