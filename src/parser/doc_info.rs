//! DocInfo 스트림 파싱
//!
//! DocInfo 스트림에서 참조 테이블(스타일 객체 목록)을 구축한다.
//! BodyText의 각 요소가 ID(인덱스)로 이 테이블을 참조한다.
//!
//! 파싱 순서:
//! DOCUMENT_PROPERTIES → ID_MAPPINGS → BIN_DATA → FACE_NAME →
//! BORDER_FILL → CHAR_SHAPE → TAB_DEF → PARA_SHAPE → STYLE

use super::byte_reader::ByteReader;
use super::record::Record;
use super::tags;

use crate::model::bin_data::{BinData, BinDataCompression, BinDataStatus, BinDataType};
use crate::model::document::{DocInfo, DocProperties, RawRecord};
use crate::model::style::{
    Alignment, BorderFill, BorderLine, BorderLineType, Bullet, CenterLine, CharShape, DiagonalLine,
    Fill, FillType, Font, GradientFill, ImageFill, ImageFillMode, LineSpacingType, Numbering,
    NumberingHead, ParaShape, SolidFill, Style, TabDef, TabItem, UnderlineType,
};

/// DocInfo 파싱 에러
#[derive(Debug)]
pub enum DocInfoError {
    RecordError(String),
    ParseError(String),
    IoError(String),
}

impl std::fmt::Display for DocInfoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DocInfoError::RecordError(e) => write!(f, "DocInfo 레코드 오류: {}", e),
            DocInfoError::ParseError(e) => write!(f, "DocInfo 파싱 오류: {}", e),
            DocInfoError::IoError(e) => write!(f, "DocInfo IO 오류: {}", e),
        }
    }
}

impl std::error::Error for DocInfoError {}

/// ID 매핑 테이블 (각 타입별 개수)
#[derive(Debug, Default)]
struct IdMappings {
    bin_data_count: u32,
    font_counts: [u32; 7], // 한글, 영문, 한자, 일본어, 기타, 기호, 사용자
    border_fill_count: u32,
    char_shape_count: u32,
    tab_def_count: u32,
    numbering_count: u32,
    bullet_count: u32,
    para_shape_count: u32,
    style_count: u32,
    memo_shape_count: u32,
}

/// DocInfo 스트림 파싱
///
/// 압축 해제된 DocInfo 레코드 바이트를 파싱하여 DocInfo, DocProperties를 반환.
pub fn parse_doc_info(data: &[u8]) -> Result<(DocInfo, DocProperties), DocInfoError> {
    let records = Record::read_all(data).map_err(|e| DocInfoError::RecordError(e.to_string()))?;

    let mut doc_info = DocInfo::default();
    let mut doc_props = DocProperties::default();
    let mut id_mappings = IdMappings::default();

    // FACE_NAME 언어 카테고리 추적
    let mut current_lang = 0usize;
    let mut lang_counts_consumed = [0u32; 7];

    // 7개 언어 카테고리 초기화
    doc_info.font_faces = vec![Vec::new(); 7];

    for record in &records {
        match record.tag_id {
            tags::HWPTAG_DOCUMENT_PROPERTIES => {
                doc_props = parse_document_properties(&record.data)?;
            }
            tags::HWPTAG_ID_MAPPINGS => {
                id_mappings = parse_id_mappings(&record.data)?;
                doc_info.bullet_count = id_mappings.bullet_count;
                doc_info.memo_shape_count = id_mappings.memo_shape_count;
            }
            tags::HWPTAG_BIN_DATA => {
                let mut bin_data = parse_bin_data(&record.data)?;
                bin_data.raw_data = Some(record.data.clone());
                doc_info.bin_data_list.push(bin_data);
            }
            tags::HWPTAG_FACE_NAME => {
                let mut font = parse_face_name(&record.data)?;
                font.raw_data = Some(record.data.clone());

                // 현재 언어 카테고리 결정
                while current_lang < 7
                    && lang_counts_consumed[current_lang] >= id_mappings.font_counts[current_lang]
                {
                    current_lang += 1;
                }

                if current_lang < 7 {
                    doc_info.font_faces[current_lang].push(font);
                    lang_counts_consumed[current_lang] += 1;
                }
            }
            tags::HWPTAG_BORDER_FILL => {
                let mut bf = parse_border_fill(&record.data)?;
                bf.raw_data = Some(record.data.clone());
                doc_info.border_fills.push(bf);
            }
            tags::HWPTAG_CHAR_SHAPE => {
                let mut cs = parse_char_shape(&record.data)?;
                cs.raw_data = Some(record.data.clone());
                doc_info.char_shapes.push(cs);
            }
            tags::HWPTAG_TAB_DEF => {
                let mut td = parse_tab_def(&record.data)?;
                td.raw_data = Some(record.data.clone());
                doc_info.tab_defs.push(td);
            }
            tags::HWPTAG_NUMBERING => {
                let mut numbering = parse_numbering(&record.data)?;
                numbering.raw_data = Some(record.data.clone());
                doc_info.numberings.push(numbering);
            }
            tags::HWPTAG_BULLET => {
                let mut bullet = parse_bullet(&record.data)?;
                bullet.raw_data = Some(record.data.clone());
                doc_info.bullets.push(bullet);
            }
            tags::HWPTAG_PARA_SHAPE => {
                let mut ps = parse_para_shape(&record.data)?;
                ps.raw_data = Some(record.data.clone());
                doc_info.para_shapes.push(ps);
            }
            tags::HWPTAG_STYLE => {
                let mut style = parse_style(&record.data)?;
                style.raw_data = Some(record.data.clone());
                doc_info.styles.push(style);
            }
            // 미지원 태그: 원시 데이터 보존 (라운드트립용)
            _ => {
                doc_info.extra_records.push(RawRecord {
                    tag_id: record.tag_id,
                    level: record.level,
                    data: record.data.clone(),
                });
            }
        }
    }

    Ok((doc_info, doc_props))
}

// ============================================================
// 개별 레코드 파서
// ============================================================

fn parse_document_properties(data: &[u8]) -> Result<DocProperties, DocInfoError> {
    let mut r = ByteReader::new(data);
    Ok(DocProperties {
        raw_data: Some(data.to_vec()),
        section_count: r.read_u16().unwrap_or(1),
        page_start_num: r.read_u16().unwrap_or(1),
        footnote_start_num: r.read_u16().unwrap_or(1),
        endnote_start_num: r.read_u16().unwrap_or(1),
        picture_start_num: r.read_u16().unwrap_or(1),
        table_start_num: r.read_u16().unwrap_or(1),
        equation_start_num: r.read_u16().unwrap_or(1),
        caret_list_id: if r.remaining() >= 4 {
            r.read_u32().unwrap_or(0)
        } else {
            0
        },
        caret_para_id: if r.remaining() >= 4 {
            r.read_u32().unwrap_or(0)
        } else {
            0
        },
        caret_char_pos: if r.remaining() >= 4 {
            r.read_u32().unwrap_or(0)
        } else {
            0
        },
    })
}

fn parse_id_mappings(data: &[u8]) -> Result<IdMappings, DocInfoError> {
    let mut r = ByteReader::new(data);
    let mut m = IdMappings::default();

    m.bin_data_count = r.read_u32().unwrap_or(0);
    for i in 0..7 {
        m.font_counts[i] = r.read_u32().unwrap_or(0);
    }
    m.border_fill_count = r.read_u32().unwrap_or(0);
    m.char_shape_count = r.read_u32().unwrap_or(0);
    m.tab_def_count = r.read_u32().unwrap_or(0);
    m.numbering_count = r.read_u32().unwrap_or(0);
    m.bullet_count = r.read_u32().unwrap_or(0);
    m.para_shape_count = r.read_u32().unwrap_or(0);
    m.style_count = r.read_u32().unwrap_or(0);
    // 16번째 필드: MemoShape 개수 (5.0.2.x 이후, 없으면 0)
    m.memo_shape_count = if r.remaining() >= 4 {
        r.read_u32().unwrap_or(0)
    } else {
        0
    };

    Ok(m)
}

fn parse_bin_data(data: &[u8]) -> Result<BinData, DocInfoError> {
    let mut r = ByteReader::new(data);
    let attr = r
        .read_u16()
        .map_err(|e| DocInfoError::IoError(e.to_string()))?;

    let data_type = match attr & 0x000F {
        0 => BinDataType::Link,
        1 => BinDataType::Embedding,
        2 => BinDataType::Storage,
        _ => BinDataType::Link,
    };

    let compression = match (attr >> 4) & 0x0003 {
        0 => BinDataCompression::Default,
        1 => BinDataCompression::Compress,
        2 => BinDataCompression::NoCompress,
        _ => BinDataCompression::Default,
    };

    let status = match (attr >> 8) & 0x0003 {
        0 => BinDataStatus::NotAccessed,
        1 => BinDataStatus::Success,
        2 => BinDataStatus::Error,
        3 => BinDataStatus::Ignored,
        _ => BinDataStatus::NotAccessed,
    };

    let mut bin = BinData {
        raw_data: None,
        attr,
        data_type: data_type.clone(),
        compression,
        status,
        abs_path: None,
        rel_path: None,
        storage_id: 0,
        extension: None,
    };

    match data_type {
        BinDataType::Link => {
            bin.abs_path = r.read_hwp_string().ok();
            bin.rel_path = r.read_hwp_string().ok();
        }
        BinDataType::Embedding | BinDataType::Storage => {
            bin.storage_id = r.read_u16().unwrap_or(0);
            bin.extension = r.read_hwp_string().ok();
        }
    }

    Ok(bin)
}

fn parse_face_name(data: &[u8]) -> Result<Font, DocInfoError> {
    let mut r = ByteReader::new(data);
    let attr = r.read_u8().unwrap_or(0);

    let name = r
        .read_hwp_string()
        .map_err(|e| DocInfoError::IoError(e.to_string()))?;

    let alt_name = if attr & 0x80 != 0 {
        let _alt_type = r.read_u8().unwrap_or(0);
        r.read_hwp_string().ok()
    } else {
        None
    };

    let type_info = if attr & 0x40 != 0 {
        let mut bytes = [0u8; 10];
        let mut ok = true;
        for b in &mut bytes {
            if let Ok(value) = r.read_u8() {
                *b = value;
            } else {
                ok = false;
                break;
            }
        }
        ok.then_some(bytes)
    } else {
        None
    };

    let default_name = if attr & 0x20 != 0 {
        r.read_hwp_string().ok()
    } else {
        None
    };

    Ok(Font {
        raw_data: None,
        name,
        alt_type: attr & 0x03,
        is_embedded: false,
        bin_item_id_ref: String::new(),
        resolved_bin_data_id: None,
        alt_name,
        type_info,
        default_name,
        // HWP5 FACE_NAME 에는 HWPX substFont 개념이 없다.
        subst_font: None,
    })
}

fn parse_border_fill(data: &[u8]) -> Result<BorderFill, DocInfoError> {
    let mut r = ByteReader::new(data);
    let attr = r.read_u16().unwrap_or(0);

    // HWP 실제 바이너리: 인터리브 형식 (각 테두리별 종류+굵기+색상 반복)
    // 순서: 좌, 우, 상, 하 × (종류 1바이트 + 굵기 1바이트 + 색상 4바이트)
    let mut borders = [BorderLine::default(); 4];

    for i in 0..4 {
        let line_type_val = r.read_u8().unwrap_or(0);
        borders[i].width = r.read_u8().unwrap_or(0);
        borders[i].color = r.read_color_ref().unwrap_or(0);
        borders[i].line_type = match line_type_val {
            0 => BorderLineType::None,
            1 => BorderLineType::Solid,
            2 => BorderLineType::Dash,
            3 => BorderLineType::Dot,
            4 => BorderLineType::DashDot,
            5 => BorderLineType::DashDotDot,
            6 => BorderLineType::LongDash,
            7 => BorderLineType::Circle,
            8 => BorderLineType::Double,
            9 => BorderLineType::ThinThickDouble,
            10 => BorderLineType::ThickThinDouble,
            11 => BorderLineType::ThinThickThinTriple,
            12 => BorderLineType::Wave,
            13 => BorderLineType::DoubleWave,
            14 => BorderLineType::Thick3D,
            15 => BorderLineType::Thick3DReverse,
            16 => BorderLineType::Thin3D,
            17 => BorderLineType::Thin3DReverse,
            _ => BorderLineType::Solid,
        };
    }

    // 대각선
    let diagonal = DiagonalLine {
        diagonal_type: r.read_u8().unwrap_or(0),
        width: r.read_u8().unwrap_or(0),
        color: r.read_color_ref().unwrap_or(0),
    };

    // 채우기
    let fill = parse_fill(&mut r);

    Ok(BorderFill {
        raw_data: None,
        attr,
        borders,
        diagonal,
        center_line: CenterLine::from_hwp_attr(attr),
        fill,
    })
}

pub(crate) fn parse_fill(r: &mut ByteReader) -> Fill {
    let fill_type_val = r.read_u32().unwrap_or(0);

    // fill_type_val은 비트마스크: bit0=Solid, bit1=Image, bit2=Gradient

    let mut fill = Fill {
        fill_type: FillType::None,
        solid: None,
        gradient: None,
        image: None,
        alpha: 0,
    };

    if fill_type_val == 0 {
        // 채우기 없음: hwplib 레퍼런스에서 4바이트 추가 skip
        let _ = r.skip(4);
        return fill;
    }

    // bit 0: 단색 채우기
    if fill_type_val & 0x01 != 0 {
        fill.fill_type = FillType::Solid;
        fill.solid = Some(SolidFill {
            background_color: r.read_color_ref().unwrap_or(0xFFFFFF),
            pattern_color: r.read_color_ref().unwrap_or(0),
            pattern_type: r.read_i32().unwrap_or(-1),
        });
    }

    // bit 2: 그라데이션 채우기
    // 참고: HWP 스펙 문서에 필드 크기 오류 있음 (레퍼런스 구현 기준 수정)
    // - kind: u8 (스펙은 INT16), angle/center_x/center_y/step/count: u32 (스펙은 INT16)
    if fill_type_val & 0x04 != 0 {
        fill.fill_type = FillType::Gradient; // Gradient가 Solid보다 우선
        let gtype = r.read_u8().unwrap_or(1) as i16;
        let angle = r.read_u32().unwrap_or(0) as i16;
        let cx = r.read_u32().unwrap_or(50) as i16;
        let cy = r.read_u32().unwrap_or(50) as i16;
        let blur = r.read_u32().unwrap_or(0) as i16;
        let count = r.read_u32().unwrap_or(2).min(64) as usize;

        // change_points: count > 2일 때만 count개의 i32 읽음
        let mut positions = Vec::new();
        if count > 2 {
            for _ in 0..count {
                positions.push(r.read_i32().unwrap_or(0));
            }
        }

        // 색상 배열: count개의 ColorRef
        let mut colors = Vec::with_capacity(count);
        for _ in 0..count {
            colors.push(r.read_color_ref().unwrap_or(0));
        }

        fill.gradient = Some(GradientFill {
            gradient_type: gtype,
            angle,
            center_x: cx,
            center_y: cy,
            blur,
            step_center: 0,
            colors,
            positions,
        });
    }

    // bit 1: 이미지 채우기
    if fill_type_val & 0x02 != 0 {
        fill.fill_type = FillType::Image;
        let mode_val = r.read_u8().unwrap_or(0);
        fill.image = Some(ImageFill {
            fill_mode: match mode_val {
                0 => ImageFillMode::TileAll,
                1 => ImageFillMode::TileHorzTop,
                2 => ImageFillMode::TileHorzBottom,
                3 => ImageFillMode::TileVertLeft,
                4 => ImageFillMode::TileVertRight,
                5 => ImageFillMode::FitToSize,
                6 => ImageFillMode::Center,
                7 => ImageFillMode::CenterTop,
                8 => ImageFillMode::CenterBottom,
                9 => ImageFillMode::LeftCenter,
                10 => ImageFillMode::LeftTop,
                11 => ImageFillMode::LeftBottom,
                12 => ImageFillMode::RightCenter,
                13 => ImageFillMode::RightTop,
                14 => ImageFillMode::RightBottom,
                15 => ImageFillMode::None,
                _ => ImageFillMode::TileAll,
            },
            brightness: r.read_i8().unwrap_or(0),
            contrast: r.read_i8().unwrap_or(0),
            effect: r.read_u8().unwrap_or(0),
            bin_data_id: r.read_u16().unwrap_or(0),
        });
    }

    // 추가 속성 (hwplib ForFillInfo.additionalProperty 참조)
    // DWORD size: 그라데이션이면 1 (blurring center 1바이트), 아니면 0
    let additional_size = r.read_u32().unwrap_or(0) as usize;
    if additional_size > 0 {
        if fill_type_val & 0x04 != 0 {
            // 그라데이션 번짐 정도 중심 (blurring center)
            if let Some(ref mut grad) = fill.gradient {
                grad.step_center = r.read_u8().unwrap_or(0);
            } else {
                let _ = r.read_u8();
            }
        } else {
            let _ = r.skip(additional_size);
        }
    }

    // 미확인 바이트 (hwplib ForFillInfo.unknownBytes 참조)
    // 각 채우기 종류별 1바이트: 채우기 투명도 (alpha)
    // 여러 종류가 동시에 켜져 있으면 각각 1바이트씩 읽으나, 첫 번째를 alpha로 사용
    if fill_type_val & 0x01 != 0 {
        fill.alpha = r.read_u8().unwrap_or(0);
    }
    if fill_type_val & 0x04 != 0 {
        let a = r.read_u8().unwrap_or(0);
        if fill.alpha == 0 {
            fill.alpha = a;
        }
    }
    if fill_type_val & 0x02 != 0 {
        let a = r.read_u8().unwrap_or(0);
        if fill.alpha == 0 {
            fill.alpha = a;
        }
    }

    fill
}

/// 취소선 모양 id(bit 26-29)가 표 27 선 종류 13종에 해당하는지 판정한다.
///
/// 한컴은 취소선이 없는 문자에도 취소선 비트(bit 18-20)에 1을 넣으므로 비트만으로는
/// 판정할 수 없고, 취소선이 없으면 모양 id에 선 종류가 아닌 placeholder(15 등)가
/// 들어온다. HWPX의 `shape="3D"`와 같은 역할이다 (`hwpx::header::is_real_strike_shape`).
/// 알 수 없는 값은 fail-closed로 no-strike 처리한다.
fn is_real_strike_shape_id(shape: u8) -> bool {
    shape <= 12
}

fn parse_char_shape(data: &[u8]) -> Result<CharShape, DocInfoError> {
    let mut r = ByteReader::new(data);

    // 폰트 ID (7개 언어)
    let mut font_ids = [0u16; 7];
    for id in font_ids.iter_mut() {
        *id = r.read_u16().unwrap_or(0);
    }

    // 장평 (7개 언어)
    let mut ratios = [100u8; 7];
    for ratio in ratios.iter_mut() {
        *ratio = r.read_u8().unwrap_or(100);
    }

    // 자간 (7개 언어)
    let mut spacings = [0i8; 7];
    for spacing in spacings.iter_mut() {
        *spacing = r.read_i8().unwrap_or(0);
    }

    // 상대 크기 (7개 언어)
    let mut relative_sizes = [100u8; 7];
    for size in relative_sizes.iter_mut() {
        *size = r.read_u8().unwrap_or(100);
    }

    // 글자 위치 (7개 언어)
    let mut char_offsets = [0i8; 7];
    for offset in char_offsets.iter_mut() {
        *offset = r.read_i8().unwrap_or(0);
    }

    let base_size = r.read_i32().unwrap_or(1000); // 기본 10pt (1000 = 10pt * 100)
    let attr = r.read_u32().unwrap_or(0);

    // 그림자 간격 (i8 x 2)
    let shadow_offset_x = r.read_i8().unwrap_or(0);
    let shadow_offset_y = r.read_i8().unwrap_or(0);

    let text_color = r.read_color_ref().unwrap_or(0);
    let underline_color = r.read_color_ref().unwrap_or(0);
    let shade_color = r.read_color_ref().unwrap_or(0xFFFFFF);
    let shadow_color = r.read_color_ref().unwrap_or(0xB2B2B2);

    // 5.0.2.1 이후: 글자 테두리/배경 ID
    let border_fill_id = if r.remaining() >= 2 {
        r.read_u16().unwrap_or(0)
    } else {
        0
    };

    // 5.0.3.0 이후: 취소선 색
    let strike_color = if r.remaining() >= 4 {
        r.read_color_ref().unwrap_or(0)
    } else {
        0
    };

    // 속성 비트 해석
    let italic = (attr & 0x01) != 0;
    let bold = (attr & 0x02) != 0;

    // 밑줄 종류 (bit 2-3)
    // HWP 스펙: 0=없음, 1=글자 아래, 3=글자 위
    // 값 2는 스펙에 정의되지 않음 (상위 버전 HWP에서 기본값으로 사용되는 경우 있음)
    let underline_type = match (attr >> 2) & 0x03 {
        1 => UnderlineType::Bottom,
        3 => UnderlineType::Top,
        _ => UnderlineType::None, // 0 및 정의되지 않은 값 2는 없음으로 처리
    };

    // hwplib 기준 비트 위치: 8-10=outline, 11-12=shadow, 13=emboss, 14=engrave
    let outline_type = ((attr >> 8) & 0x07) as u8;
    let shadow_type = ((attr >> 11) & 0x03) as u8;
    let emboss = (attr & (1 << 13)) != 0;
    let engrave = (attr & (1 << 14)) != 0;

    // HWP 스펙 표 37: bit 15 = 위첨자, bit 16 = 아래첨자 (개별 플래그)
    let superscript = (attr & (1 << 15)) != 0;
    let subscript = (attr & (1 << 16)) != 0;
    // 밑줄 모양 (bit 4-7, 표 27 선 종류)
    let underline_shape = ((attr >> 4) & 0x0F) as u8;
    // 강조점 종류 (bit 21-24)
    let emphasis_dot = ((attr >> 21) & 0x0F) as u8;
    // 취소선 모양 (bit 26-29, 표 27 선 종류)
    let strike_shape = ((attr >> 26) & 0x0F) as u8;
    // 취소선 여부 (bit 18-20). 비트만으로는 판정할 수 없어 모양 id를 함께 본다
    // (is_real_strike_shape_id 참고).
    let strikethrough = (attr >> 18) & 0x07 != 0 && is_real_strike_shape_id(strike_shape);
    // 커닝 여부 (bit 30)
    let kerning = (attr & (1 << 30)) != 0;
    // 글꼴에 어울리는 빈칸 사용 여부 (bit 25)
    let use_font_space = (attr & (1 << 25)) != 0;

    Ok(CharShape {
        raw_data: None,
        font_ids,
        ratios,
        spacings,
        relative_sizes,
        char_offsets,
        base_size,
        attr,
        italic,
        bold,
        underline_type,
        outline_type,
        shadow_type,
        shadow_offset_x,
        shadow_offset_y,
        text_color,
        underline_color,
        shade_color,
        shadow_color,
        border_fill_id,
        strike_color,
        strikethrough,
        subscript,
        superscript,
        emboss,
        engrave,
        emphasis_dot,
        underline_shape,
        strike_shape,
        kerning,
        use_font_space,
    })
}

fn parse_tab_def(data: &[u8]) -> Result<TabDef, DocInfoError> {
    let mut r = ByteReader::new(data);
    let attr = r.read_u32().unwrap_or(0);
    let tab_count = r.read_u32().unwrap_or(0) as usize;

    let mut tabs = Vec::with_capacity(tab_count);
    for _ in 0..tab_count {
        if r.remaining() < 8 {
            break;
        }
        tabs.push(TabItem {
            position: r.read_u32().unwrap_or(0),
            tab_type: r.read_u8().unwrap_or(0),
            fill_type: r.read_u8().unwrap_or(0),
        });
        let _ = r.skip(2); // 예약
    }

    Ok(TabDef {
        raw_data: None,
        attr,
        tabs,
        auto_tab_left: (attr & 0x01) != 0,
        auto_tab_right: (attr & 0x02) != 0,
    })
}

fn parse_para_shape(data: &[u8]) -> Result<ParaShape, DocInfoError> {
    let mut r = ByteReader::new(data);
    let attr1 = r.read_u32().unwrap_or(0);
    let margin_left = r.read_i32().unwrap_or(0);
    let margin_right = r.read_i32().unwrap_or(0);
    let indent = r.read_i32().unwrap_or(0);
    let spacing_before = r.read_i32().unwrap_or(0);
    let spacing_after = r.read_i32().unwrap_or(0);
    let line_spacing = r.read_i32().unwrap_or(160); // 기본 160%

    // attr1에서 줄간격/정렬 타입 추출 (표 46: 문단 모양 속성1)
    // bit 0~1: 줄 간격 종류
    let line_spacing_type = match attr1 & 0x03 {
        0 => LineSpacingType::Percent,
        1 => LineSpacingType::Fixed,
        2 => LineSpacingType::SpaceOnly,
        3 => LineSpacingType::Minimum,
        _ => LineSpacingType::Percent,
    };

    // bit 2~4: 정렬 방식
    let alignment = match (attr1 >> 2) & 0x07 {
        0 => Alignment::Justify,
        1 => Alignment::Left,
        2 => Alignment::Right,
        3 => Alignment::Center,
        4 => Alignment::Distribute,
        5 => Alignment::Split,
        _ => Alignment::Justify,
    };

    let tab_def_id = r.read_u16().unwrap_or(0);
    let numbering_id = r.read_u16().unwrap_or(0);
    let border_fill_id = r.read_u16().unwrap_or(0);

    let mut border_spacing = [0i16; 4];
    for spacing in border_spacing.iter_mut() {
        *spacing = r.read_i16().unwrap_or(0);
    }

    // 속성2 (5.0.1.7 이상)
    let attr2 = if r.remaining() >= 4 {
        r.read_u32().unwrap_or(0)
    } else {
        0
    };

    // 속성3 - 줄 간격 종류 확장 (5.0.2.5 이상)
    let attr3 = if r.remaining() >= 4 {
        r.read_u32().unwrap_or(0)
    } else {
        0
    };

    // 줄 간격 (5.0.2.5 이상)
    let line_spacing_v2 = if r.remaining() >= 4 {
        r.read_u32().unwrap_or(0)
    } else {
        0
    };

    let head_type = match (attr1 >> 23) & 0x03 {
        1 => crate::model::style::HeadType::Outline,
        2 => crate::model::style::HeadType::Number,
        3 => crate::model::style::HeadType::Bullet,
        _ => crate::model::style::HeadType::None,
    };
    let para_level = ((attr1 >> 25) & 0x07) as u8;

    Ok(ParaShape {
        raw_data: None,
        attr1,
        margin_left,
        margin_right,
        indent,
        spacing_before,
        spacing_after,
        line_spacing,
        alignment,
        line_spacing_type,
        tab_def_id,
        numbering_id,
        border_fill_id,
        border_spacing,
        attr2,
        attr3,
        line_spacing_v2,
        head_type,
        para_level,
        // HWP5 는 breakLatinWord 를 attr1 비트로 갖지만 HWPX 원문 보존 필드는 미사용
        // (None → 직렬화 KEEP_WORD 기본, 기존 동작 유지). (#1986)
        break_latin_word: None,
    })
}

/// HWPTAG_NUMBERING 파싱 (스펙 표 40: 문단 번호)
fn parse_numbering(data: &[u8]) -> Result<Numbering, DocInfoError> {
    let mut r = ByteReader::new(data);
    let mut numbering = Numbering::default();

    // 수준별(1~7) 문단 머리 정보 + 번호 형식 문자열
    for level in 0..7 {
        // 문단 머리 정보 (표 41: 12바이트)
        let attr = r.read_u32().unwrap_or(0);
        let width_adjust = r.read_i16().unwrap_or(0);
        let text_distance = r.read_i16().unwrap_or(0);
        let char_shape_id = r.read_u32().unwrap_or(0);

        let number_format = ((attr >> 5) & 0x0F) as u8;
        numbering.heads[level] = NumberingHead {
            attr,
            width_adjust,
            text_distance,
            char_shape_id,
            number_format,
        };

        // 번호 형식 문자열 (가변 길이)
        let format_len = r.read_u16().unwrap_or(0) as usize;
        if format_len > 0 {
            let mut format_str = String::new();
            for _ in 0..format_len {
                let ch = r.read_u16().unwrap_or(0);
                if ch > 0 {
                    if let Some(c) = char::from_u32(ch as u32) {
                        format_str.push(c);
                    }
                }
            }
            numbering.level_formats[level] = format_str;
        }
    }

    // 시작 번호
    numbering.start_number = r.read_u16().unwrap_or(1);

    // 수준별 시작번호 (5.0.2.5 이상, 7회 반복)
    for level in 0..7 {
        numbering.level_start_numbers[level] = r.read_u32().unwrap_or(1);
    }

    Ok(numbering)
}

/// HWPTAG_BULLET 파싱 (표 44: 글머리표)
fn parse_bullet(data: &[u8]) -> Result<Bullet, DocInfoError> {
    let mut r = ByteReader::new(data);

    // 문단 머리 정보 (12바이트): attr(4) + width_adjust(2) + text_distance(2) + char_shape_id(4)
    let attr = r.read_u32().unwrap_or(0);
    let width_adjust = r.read_i16().unwrap_or(0);
    let text_distance = r.read_i16().unwrap_or(0);
    let char_shape_id = r.read_u32().unwrap_or(0);

    // 글머리표 문자 (WCHAR, 2바이트)
    let bullet_char_u16 = r.read_u16().unwrap_or(0x2022); // 기본: ●(U+2022)
    let bullet_char = char::from_u32(bullet_char_u16 as u32).unwrap_or('●');

    // 이미지 글머리표 여부 (INT32, 4바이트)
    let image_bullet = r.read_i32().unwrap_or(0);

    // 이미지 글머리 데이터 (4바이트)
    let mut image_data = [0u8; 4];
    for byte in &mut image_data {
        *byte = r.read_u8().unwrap_or(0);
    }

    // 체크 글머리표 문자 (WCHAR, 2바이트)
    let check_char_u16 = r.read_u16().unwrap_or(0);
    let check_bullet_char = char::from_u32(check_char_u16 as u32).unwrap_or('\0');

    Ok(Bullet {
        raw_data: None,
        attr,
        width_adjust,
        text_distance,
        char_shape_id,
        bullet_char,
        image_bullet,
        image_data,
        check_bullet_char,
    })
}

fn parse_style(data: &[u8]) -> Result<Style, DocInfoError> {
    let mut r = ByteReader::new(data);

    let local_name = r
        .read_hwp_string()
        .map_err(|e| DocInfoError::IoError(e.to_string()))?;
    let english_name = r
        .read_hwp_string()
        .map_err(|e| DocInfoError::IoError(e.to_string()))?;

    let style_type = r.read_u8().unwrap_or(0);
    let next_style_id = r.read_u8().unwrap_or(0);
    // [Task #1058 후속] HWP5 spec 표 47 정합 — lang_id (INT16, default 1042=한국어)
    let lang_id = r.read_i16().unwrap_or(1042);
    let para_shape_id = r.read_u16().unwrap_or(0);
    let char_shape_id = r.read_u16().unwrap_or(0);
    // [Task #1058 후속] 끝 UINT16 (스펙 미문서화) — 한컴 정답지 STYLE 의 마지막 2 byte zero 흡수.
    let _trailing = r.read_u16().unwrap_or(0);

    Ok(Style {
        raw_data: None,
        local_name,
        english_name,
        style_type,
        next_style_id,
        lang_id,
        para_shape_id,
        char_shape_id,
    })
}
