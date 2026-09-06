//! `<hp:tbl>` 표 직렬화.
//!
//! Stage 3 (#182): `Control::Table` IR → `<hp:tbl>` + `<hp:tr>` + `<hp:tc>` + `<hp:subList>` + 문단 재귀.
//!
//! 속성·자식 순서는 한컴 OWPML 공식 (hancom-io/hwpx-owpml-model, Apache 2.0)
//! `Class/Para/TableType.cpp` 의 `WriteElement()`, `InitMap()` 기준:
//!
//! ### `<hp:tbl>` 속성 순서 (부모 AbstractShapeObjectType + 자신)
//! id, zOrder, numberingType, textWrap, textFlow, lock, dropcapstyle,
//! pageBreak, repeatHeader, rowCnt, colCnt, cellSpacing, borderFillIDRef, noAdjust
//!
//! ### `<hp:tbl>` 자식 순서
//! sz, pos, outMargin, (caption, shapeComment, parameterset, metaTag — 옵셔널),
//! inMargin, (cellzoneList — 옵셔널), tr (루프), (label — 옵셔널)
//!
//! ### `<hp:tc>` 속성 순서
//! name, header, hasMargin, protect, editable, dirty, borderFillIDRef
//!
//! ### `<hp:tc>` 자식 순서
//! subList, cellAddr, cellSpan, cellSz, cellMargin
//!
//! ## 중요: table.attr 비트 연산 금지
//!
//! HWPX에서 `table.attr` 는 0인 경우가 많으므로 비트 연산으로 `textWrap/textFlow/pageBreak` 등을
//! 추출하면 안 된다. 반드시 `table.common.text_wrap`, `table.page_break` 등 파싱된 IR 필드를 사용.

use std::io::Write;

use quick_xml::Writer;

use crate::model::shape::{
    CommonObjAttr, HorzAlign, HorzRelTo, TextFlow, TextWrap, VertAlign, VertRelTo,
};
use crate::model::table::{Cell, Table, TablePageBreak, VerticalAlign};

use super::context::SerializeContext;
use super::section::{render_hp_p_open, render_paragraph_parts};
use super::utils::{empty_tag, end_tag, start_tag, start_tag_attrs};
use super::SerializeError;

/// `<hp:tbl>` 직렬화.
pub fn write_table<W: Write>(
    w: &mut Writer<W>,
    table: &Table,
    ctx: &mut SerializeContext,
) -> Result<(), SerializeError> {
    // borderFillIDRef 참조 등록 (assert_all_refs_resolved 검증 대상)
    ctx.border_fill_ids.reference(table.border_fill_id);
    for zone in &table.zones {
        ctx.border_fill_ids.reference(zone.border_fill_id);
    }
    for cell in &table.cells {
        ctx.border_fill_ids.reference(cell.border_fill_id);
    }

    // --- <hp:tbl> 시작 태그 + 속성 ---
    let id_str = table.common.instance_id.to_string();
    let z_order = table.common.z_order.to_string();
    let text_wrap = text_wrap_str(table.common.text_wrap);
    let text_flow = text_flow_str(table.common.text_flow);
    let lock = bool01(false);
    let page_break = table_page_break_str(table.page_break);
    let repeat_header = bool01(table.repeat_header);
    let row_cnt = table.row_count.to_string();
    let col_cnt = table.col_count.to_string();
    let cell_spacing = table.cell_spacing.to_string();
    let border_fill_id_ref = table.border_fill_id.to_string();
    // noAdjust = HWPTAG_TABLE 레코드 attr bit3. `table.attr` 은 **CommonObjAttr FLAGS**
    // 의 미러이고 그 bit3 은 vert_rel_to 하위 비트라 여기서 OR 하면 안 된다
    // (vertRelTo=PAGE 표가 noAdjust=1 로 새어나갔다). 모든 Table.attr 기록자가 FLAGS 를
    // 쓴다: parser/control.rs:161 · hwp3/mod.rs:606 · hwpx/section.rs:1853.
    let no_adjust = bool01(table.raw_table_record_attr & 0x08 != 0);

    start_tag_attrs(
        w,
        "hp:tbl",
        &[
            ("id", &id_str),
            ("zOrder", &z_order),
            ("numberingType", "TABLE"),
            ("textWrap", text_wrap),
            ("textFlow", text_flow),
            ("lock", lock),
            ("dropcapstyle", "None"),
            ("pageBreak", page_break),
            ("repeatHeader", repeat_header),
            ("rowCnt", &row_cnt),
            ("colCnt", &col_cnt),
            ("cellSpacing", &cell_spacing),
            ("borderFillIDRef", &border_fill_id_ref),
            ("noAdjust", no_adjust),
        ],
    )?;

    // --- 자식: sz, pos, outMargin, (caption — 옵셔널), inMargin, tr[] ---
    write_sz(w, &table.common)?;
    write_pos(w, &table.common)?;
    write_out_margin(w, table)?;
    if let Some(caption) = &table.caption {
        write_caption(w, caption, ctx)?;
    }
    write_in_margin(w, table)?;
    // cellzoneList: inMargin 다음, tr 앞 (OWPML 자식 순서). 셀 영역 테두리/배경
    // 오버레이를 정의한다. 파서가 table.zones 로 보존하므로 그대로 되살린다.
    write_cellzone_list(w, table)?;

    // tr[]: 행 단위 반복. 각 행에 속한 셀 (cell.row == r) 을 col 오름차순으로 출력.
    for row_idx in 0..table.row_count {
        start_tag(w, "hp:tr")?;
        let mut row_cells: Vec<&Cell> = table.cells.iter().filter(|c| c.row == row_idx).collect();
        row_cells.sort_by_key(|c| c.col);
        for cell in row_cells {
            write_cell(w, cell, ctx)?;
        }
        end_tag(w, "hp:tr")?;
    }

    // label: tr 뒤(OWPML 자식 순서 마지막). 파서가 table.hwpx_label 로 보존한 속성을 그대로 되살린다.
    if !table.hwpx_label.is_empty() {
        let attrs: Vec<(&str, &str)> = table.hwpx_label.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        empty_tag(w, "hp:label", &attrs)?;
    }
    end_tag(w, "hp:tbl")?;
    Ok(())
}

fn write_sz<W: Write>(w: &mut Writer<W>, c: &CommonObjAttr) -> Result<(), SerializeError> {
    let width = c.width.to_string();
    let height = c.height.to_string();
    empty_tag(
        w,
        "hp:sz",
        &[
            ("width", &width),
            ("widthRelTo", "ABSOLUTE"),
            ("height", &height),
            ("heightRelTo", "ABSOLUTE"),
            ("protect", "0"),
        ],
    )
}

fn write_pos<W: Write>(w: &mut Writer<W>, c: &CommonObjAttr) -> Result<(), SerializeError> {
    let treat = bool01(c.treat_as_char);
    let vert_offset = c.vertical_offset.to_string();
    let horz_offset = c.horizontal_offset.to_string();
    // [#1594] holdAnchorAndSO 는 IR(prevent_page_break)을 보존한다. 종전 "0" 하드코딩은
    // 페이지 하단 앵커 개체(발신명의 footer 등)의 1→0 드롭으로 한글 페이지 붕괴를 유발했다.
    let hold = bool01(c.prevent_page_break != 0);
    empty_tag(
        w,
        "hp:pos",
        &[
            ("treatAsChar", treat),
            ("affectLSpacing", "0"),
            // [#1637] flowWithText 는 IR(flow_with_text)을 보존한다. 종전 "1" 하드코딩은
            // treatAsChar 표의 1→0 케이스에서 0→1 드롭으로 표 partial-split 임계를 흔들어
            // 페이지네이션이 달라졌다(IR-invisible). #1594 holdAnchorAndSO 와 동형.
            ("flowWithText", bool01(c.flow_with_text)),
            // [officex] allowOverlap 도 IR(allow_overlap)을 보존한다 — 파서는 이 속성을
            // 읽는데(parser/hwpx/section.rs:1692) 표만 "0" 하드코딩이라 HWPX 왕복에서
            // "개체 겹침 허용"이 무조건 꺼졌다. 그림/도형 직렬화기는 이미 IR 을 쓴다.
            ("allowOverlap", bool01(c.allow_overlap)),
            ("holdAnchorAndSO", hold),
            ("vertRelTo", vert_rel_to_str(c.vert_rel_to)),
            ("horzRelTo", horz_rel_to_str(c.horz_rel_to)),
            ("vertAlign", vert_align_str(c.vert_align)),
            ("horzAlign", horz_align_str(c.horz_align)),
            ("vertOffset", &vert_offset),
            ("horzOffset", &horz_offset),
        ],
    )
}

fn write_out_margin<W: Write>(w: &mut Writer<W>, t: &Table) -> Result<(), SerializeError> {
    let left = t.outer_margin_left.to_string();
    let right = t.outer_margin_right.to_string();
    let top = t.outer_margin_top.to_string();
    let bottom = t.outer_margin_bottom.to_string();
    empty_tag(
        w,
        "hp:outMargin",
        &[
            ("left", &left),
            ("right", &right),
            ("top", &top),
            ("bottom", &bottom),
        ],
    )
}

fn write_in_margin<W: Write>(w: &mut Writer<W>, t: &Table) -> Result<(), SerializeError> {
    let left = t.padding.left.to_string();
    let right = t.padding.right.to_string();
    let top = t.padding.top.to_string();
    let bottom = t.padding.bottom.to_string();
    empty_tag(
        w,
        "hp:inMargin",
        &[
            ("left", &left),
            ("right", &right),
            ("top", &top),
            ("bottom", &bottom),
        ],
    )
}

/// `<hp:cellzoneList>` + `<hp:cellzone>` 자식들. 셀 영역(범위) 단위 테두리/배경
/// 오버레이. `table.zones` 가 비어 있으면 원본에 없었던 것이므로 미방출.
/// 속성 순서는 OWPML 관찰 순서: startRowAddr, startColAddr, endRowAddr,
/// endColAddr, borderFillIDRef. borderFillIDRef 는 셀과 동일하게 raw 값(오프셋
/// 없음)으로 쓴다 (parser `parse_u16`, 헤더 borderFill id 와 1:1).
fn write_cellzone_list<W: Write>(w: &mut Writer<W>, t: &Table) -> Result<(), SerializeError> {
    if t.zones.is_empty() {
        return Ok(());
    }
    start_tag(w, "hp:cellzoneList")?;
    for zone in &t.zones {
        let start_row = zone.start_row.to_string();
        let start_col = zone.start_col.to_string();
        let end_row = zone.end_row.to_string();
        let end_col = zone.end_col.to_string();
        let border_fill = zone.border_fill_id.to_string();
        empty_tag(
            w,
            "hp:cellzone",
            &[
                ("startRowAddr", &start_row),
                ("startColAddr", &start_col),
                ("endRowAddr", &end_row),
                ("endColAddr", &end_col),
                ("borderFillIDRef", &border_fill),
            ],
        )?;
    }
    end_tag(w, "hp:cellzoneList")
}

fn write_cell<W: Write>(
    w: &mut Writer<W>,
    cell: &Cell,
    ctx: &mut SerializeContext,
) -> Result<(), SerializeError> {
    let name = cell.field_name.as_deref().unwrap_or("");
    let header = bool01(cell.is_header);
    let has_margin = bool01(cell.apply_inner_margin);
    let protect = bool01(cell.cell_protect());
    let editable = bool01(cell.editable_in_form());
    let border_ref = cell.border_fill_id.to_string();

    start_tag_attrs(
        w,
        "hp:tc",
        &[
            ("name", name),
            ("header", header),
            ("hasMargin", has_margin),
            ("protect", protect),
            ("editable", editable),
            ("dirty", "0"),
            ("borderFillIDRef", &border_ref),
        ],
    )?;

    // 자식 순서: subList, cellAddr, cellSpan, cellSz, cellMargin
    write_sub_list(w, cell, ctx)?;
    write_cell_addr(w, cell)?;
    write_cell_span(w, cell)?;
    write_cell_sz(w, cell)?;
    write_cell_margin(w, cell)?;

    end_tag(w, "hp:tc")?;
    Ok(())
}

fn write_sub_list<W: Write>(
    w: &mut Writer<W>,
    cell: &Cell,
    ctx: &mut SerializeContext,
) -> Result<(), SerializeError> {
    start_tag_attrs(
        w,
        "hp:subList",
        &[
            ("id", ""),
            (
                "textDirection",
                if cell.text_direction == 1 {
                    "VERTICAL"
                } else {
                    "HORIZONTAL"
                },
            ),
            ("lineWrap", "BREAK"),
            ("vertAlign", cell_vert_align_str(cell.vertical_align)),
            ("linkListIDRef", "0"),
            ("linkListNextIDRef", "0"),
            ("textWidth", "0"),
            ("textHeight", "0"),
            ("hasTextRef", "0"),
            ("hasNumRef", "0"),
        ],
    )?;

    write_sub_list_paragraphs(w, &cell.paragraphs, ctx)?;

    end_tag(w, "hp:subList")?;
    Ok(())
}

/// subList 내부 문단 시퀀스 방출 — 셀(#1379)과 표 캡션(#1387)이 공유.
///
/// 본문과 동일한 공유 직렬화 경로(render_paragraph_parts)로 컨트롤 슬롯(표 재귀
/// 포함) 방출 + run 분할 + lineseg IR 보존/fallback (#1379 2단계).
/// sub_list_depth: subList 경로 한정 colPr 인라인 방출 스코프 (#1379 3단계).
fn write_sub_list_paragraphs<W: Write>(
    w: &mut Writer<W>,
    paragraphs: &[crate::model::paragraph::Paragraph],
    ctx: &mut SerializeContext,
) -> Result<(), SerializeError> {
    ctx.sub_list_depth += 1;
    let mut vert_cursor: u32 = 0;
    for para in paragraphs {
        ctx.para_shape_ids.reference(para.para_shape_id);
        let sid = ctx.effective_style_id(para.style_id);
        ctx.style_ids.reference(sid as u16);

        let (runs, linesegs, advance) = render_paragraph_parts(para, vert_cursor, ctx);
        vert_cursor = advance;
        let pid = ctx.next_para_id();
        let mut p_xml = render_hp_p_open(para, pid, sid);
        p_xml.push_str(&runs);
        p_xml.push_str(&linesegs);
        p_xml.push_str("</hp:p>");
        w.get_mut()
            .write_all(p_xml.as_bytes())
            .map_err(|e| SerializeError::XmlError(e.to_string()))?;
    }
    ctx.sub_list_depth -= 1;
    Ok(())
}

/// `<hp:caption>` 직렬화 (#1387) — 자식 순서상 outMargin 과 inMargin 사이 (모듈 doc).
///
/// 속성 순서는 한컴 실물(ta-pic-001-r) 기준: side, fullSz, width, gap, lastWidth.
/// 캡션 subList 속성은 파서가 적재하지 않으며 samples/hwpx 전수 17건 동일
/// (vertAlign=TOP lineWrap=BREAK textDirection=HORIZONTAL — 1단계 측정) — 실물 고정값 방출.
/// 그림/도형/묶음 캡션(#1403)도 동일 경로를 공유한다 (pub(super)).
pub(super) fn write_caption<W: Write>(
    w: &mut Writer<W>,
    caption: &crate::model::shape::Caption,
    ctx: &mut SerializeContext,
) -> Result<(), SerializeError> {
    use crate::model::shape::CaptionDirection;

    let side = match caption.direction {
        CaptionDirection::Left => "LEFT",
        CaptionDirection::Right => "RIGHT",
        CaptionDirection::Top => "TOP",
        CaptionDirection::Bottom => "BOTTOM",
    };
    let full_sz = bool01(caption.include_margin);
    let width = caption.width.to_string();
    let gap = caption.spacing.to_string();
    let last_width = caption.max_width.to_string();
    start_tag_attrs(
        w,
        "hp:caption",
        &[
            ("side", side),
            ("fullSz", full_sz),
            ("width", &width),
            ("gap", &gap),
            ("lastWidth", &last_width),
        ],
    )?;
    start_tag_attrs(
        w,
        "hp:subList",
        &[
            ("id", ""),
            ("textDirection", "HORIZONTAL"),
            ("lineWrap", "BREAK"),
            ("vertAlign", "TOP"),
            ("linkListIDRef", "0"),
            ("linkListNextIDRef", "0"),
            ("textWidth", "0"),
            ("textHeight", "0"),
            ("hasTextRef", "0"),
            ("hasNumRef", "0"),
        ],
    )?;
    write_sub_list_paragraphs(w, &caption.paragraphs, ctx)?;
    end_tag(w, "hp:subList")?;
    end_tag(w, "hp:caption")?;
    Ok(())
}

fn write_cell_addr<W: Write>(w: &mut Writer<W>, cell: &Cell) -> Result<(), SerializeError> {
    let col = cell.col.to_string();
    let row = cell.row.to_string();
    empty_tag(w, "hp:cellAddr", &[("colAddr", &col), ("rowAddr", &row)])
}

fn write_cell_span<W: Write>(w: &mut Writer<W>, cell: &Cell) -> Result<(), SerializeError> {
    let cs = cell.col_span.max(1).to_string();
    let rs = cell.row_span.max(1).to_string();
    empty_tag(w, "hp:cellSpan", &[("colSpan", &cs), ("rowSpan", &rs)])
}

fn write_cell_sz<W: Write>(w: &mut Writer<W>, cell: &Cell) -> Result<(), SerializeError> {
    let w_s = cell.width.to_string();
    let h_s = cell.height.to_string();
    empty_tag(w, "hp:cellSz", &[("width", &w_s), ("height", &h_s)])
}

fn write_cell_margin<W: Write>(w: &mut Writer<W>, cell: &Cell) -> Result<(), SerializeError> {
    let l = cell.padding.left.to_string();
    let r = cell.padding.right.to_string();
    let t = cell.padding.top.to_string();
    let b = cell.padding.bottom.to_string();
    empty_tag(
        w,
        "hp:cellMargin",
        &[("left", &l), ("right", &r), ("top", &t), ("bottom", &b)],
    )
}

// ---------- enum 변환 헬퍼 ----------

fn bool01(b: bool) -> &'static str {
    if b {
        "1"
    } else {
        "0"
    }
}

fn text_wrap_str(w: TextWrap) -> &'static str {
    use TextWrap::*;
    match w {
        Square => "SQUARE",
        Tight => "TIGHT",
        Through => "THROUGH",
        TopAndBottom => "TOP_AND_BOTTOM",
        BehindText => "BEHIND_TEXT",
        InFrontOfText => "IN_FRONT_OF_TEXT",
    }
}

fn text_flow_str(f: TextFlow) -> &'static str {
    match f {
        TextFlow::BothSides => "BOTH_SIDES",
        TextFlow::LeftOnly => "LEFT_ONLY",
        TextFlow::RightOnly => "RIGHT_ONLY",
        TextFlow::LargestOnly => "LARGEST_ONLY",
    }
}

fn table_page_break_str(pb: TablePageBreak) -> &'static str {
    use TablePageBreak::*;
    // HWPX 문자열 매핑은 파서(parser/hwpx/section.rs `pageBreak`)의 정확한 역이어야
    // HWPX→HWPX 왕복이 충실하다. 파서는 한컴 관찰 기준으로
    // `"TABLE"→CellBreak`, `"CELL"/"ROW"→RowBreak` 로 매핑하므로 (한컴 HWPX
    // "CELL" = HWP5 row-break bit 1 = RowBreak), 직렬화도 그 역으로 방출한다.
    // (이전 구현은 CellBreak→"CELL", RowBreak→"TABLE" 로 파서와 불일치해 왕복 시
    //  CELL↔TABLE 이 뒤바뀌었다.) enum→HWP5 bit 매핑(hwpx_to_hwp.rs)은 enum 을
    // 직접 쓰므로 이 변경의 영향을 받지 않는다.
    match pb {
        None => "NONE",
        CellBreak => "TABLE",
        RowBreak => "CELL",
    }
}

fn vert_rel_to_str(v: VertRelTo) -> &'static str {
    use VertRelTo::*;
    match v {
        Paper => "PAPER",
        Page => "PAGE",
        Para => "PARA",
    }
}

fn horz_rel_to_str(h: HorzRelTo) -> &'static str {
    use HorzRelTo::*;
    match h {
        Paper => "PAPER",
        Page => "PAGE",
        Column => "COLUMN",
        Para => "PARA",
    }
}

fn vert_align_str(v: VertAlign) -> &'static str {
    use VertAlign::*;
    match v {
        Top => "TOP",
        Center => "CENTER",
        Bottom => "BOTTOM",
        Inside => "INSIDE",
        Outside => "OUTSIDE",
    }
}

fn horz_align_str(h: HorzAlign) -> &'static str {
    use HorzAlign::*;
    match h {
        Left => "LEFT",
        Center => "CENTER",
        Right => "RIGHT",
        Inside => "INSIDE",
        Outside => "OUTSIDE",
    }
}

fn cell_vert_align_str(v: VerticalAlign) -> &'static str {
    use VerticalAlign::*;
    match v {
        Top => "TOP",
        Center => "CENTER",
        Bottom => "BOTTOM",
    }
}
