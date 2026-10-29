//! [캔버스 한컴 포크] 원본 문단 객체를 옮기는 쪽 묶음 재정렬.
use crate::document_core::DocumentCore;
use crate::error::HwpError;
use crate::model::control::Control;
use crate::model::paragraph::{ColumnBreakType, Paragraph};
use crate::renderer::pagination::PageItem;
use std::collections::BTreeSet;

#[derive(Clone, Debug)]
struct PageRange {
    start: usize,
    end: usize,
    section: usize,
    para_start: usize,
    para_end: usize,
}
fn error(message: &str) -> HwpError {
    HwpError::RenderError(message.into())
}
fn paragraph_of(item: &PageItem) -> Option<usize> {
    match item {
        PageItem::FullParagraph { para_index }
        | PageItem::PartialParagraph { para_index, .. }
        | PageItem::Table { para_index, .. }
        | PageItem::PartialTable { para_index, .. }
        | PageItem::Shape { para_index, .. } => Some(*para_index),
        PageItem::EndnoteSeparator { .. } => None,
    }
}
fn structural(control: &Control) -> bool {
    matches!(control, Control::SectionDef(_) | Control::ColumnDef(_))
}

// 선행 구역/단 슬롯만 재배치한다. 텍스트·글자 모양의 UTF16 위치를 함께 평행 이동한다.
fn take_prefix(para: &mut Paragraph) -> Vec<(Control, Option<Vec<u8>>)> {
    let n = para.controls.iter().take_while(|c| structural(c)).count();
    let mut result = Vec::new();
    for _ in 0..n {
        result.push((
            para.controls.remove(0),
            if para.ctrl_data_records.is_empty() {
                None
            } else {
                para.ctrl_data_records.remove(0)
            },
        ));
    }
    shift_slots(para, -(n as i64 * 8), -(n as i64));
    if !para.controls.iter().any(structural) {
        para.control_mask &= !0x0004;
    }
    result
}
fn shift_slots(para: &mut Paragraph, units: i64, controls: i64) {
    let shift = |v: u32| (i64::from(v) + units).max(0) as u32;
    for offset in &mut para.char_offsets {
        *offset = shift(*offset);
    }
    for shape in &mut para.char_shapes {
        if shape.start_pos != 0 {
            shape.start_pos = shift(shape.start_pos);
        }
    }
    for seg in &mut para.line_segs {
        if seg.text_start != 0 {
            seg.text_start = shift(seg.text_start);
        }
    }
    for tag in &mut para.range_tags {
        tag.start = shift(tag.start);
        tag.end = shift(tag.end);
    }
    for mark in &mut para.track_marks {
        mark.start_pos = shift(mark.start_pos);
        mark.end_pos = shift(mark.end_pos);
    }
    for field in &mut para.field_ranges {
        field.control_idx = (field.control_idx as i64 + controls).max(0) as usize;
    }
    para.char_count = shift(para.char_count);
}
fn put_prefix(para: &mut Paragraph, prefix: Vec<(Control, Option<Vec<u8>>)>) {
    let n = prefix.len();
    shift_slots(para, n as i64 * 8, n as i64);
    while para.ctrl_data_records.len() < para.controls.len() {
        para.ctrl_data_records.push(None);
    }
    for (control, raw) in prefix.into_iter().rev() {
        para.controls.insert(0, control);
        para.ctrl_data_records.insert(0, raw);
    }
    if n > 0 {
        para.control_mask |= 0x0004;
    }
}
fn visit_shape_paragraphs(
    shape: &mut crate::model::shape::ShapeObject,
    visit: &mut impl FnMut(&mut Paragraph),
) {
    use crate::model::shape::ShapeObject;
    if let Some(drawing) = shape.drawing_mut() {
        if let Some(text) = &mut drawing.text_box {
            visit_paragraphs(&mut text.paragraphs, visit);
        }
        if let Some(caption) = &mut drawing.caption {
            visit_paragraphs(&mut caption.paragraphs, visit);
        }
    }
    match shape {
        ShapeObject::Group(group) => {
            for shape in &mut group.children {
                visit_shape_paragraphs(shape, visit);
            }
            if let Some(caption) = &mut group.caption {
                visit_paragraphs(&mut caption.paragraphs, visit);
            }
        }
        ShapeObject::Picture(pic) => {
            if let Some(caption) = &mut pic.caption {
                visit_paragraphs(&mut caption.paragraphs, visit);
            }
        }
        _ => {}
    }
}
fn visit_paragraphs(paragraphs: &mut [Paragraph], visit: &mut impl FnMut(&mut Paragraph)) {
    for para in paragraphs {
        visit(para);
        for control in &mut para.controls {
            match control {
                Control::Table(table) => {
                    for cell in &mut table.cells {
                        visit_paragraphs(&mut cell.paragraphs, visit);
                    }
                    if let Some(caption) = &mut table.caption {
                        visit_paragraphs(&mut caption.paragraphs, visit);
                    }
                }
                Control::Shape(shape) => visit_shape_paragraphs(shape, visit),
                Control::Picture(pic) => {
                    if let Some(caption) = &mut pic.caption {
                        visit_paragraphs(&mut caption.paragraphs, visit);
                    }
                }
                Control::Footnote(note) => visit_paragraphs(&mut note.paragraphs, visit),
                Control::Endnote(note) => visit_paragraphs(&mut note.paragraphs, visit),
                _ => {}
            }
        }
    }
}
fn visit_shape_ids(shape: &mut crate::model::shape::ShapeObject, visit: &mut impl FnMut(&mut u32)) {
    use crate::model::shape::ShapeObject;
    visit(&mut shape.common_mut().instance_id);
    if let Some(drawing) = shape.drawing_mut() {
        visit(&mut drawing.inst_id);
    }
    match shape {
        ShapeObject::Group(group) => {
            for child in &mut group.children {
                visit_shape_ids(child, visit);
            }
        }
        ShapeObject::Picture(pic) => visit(&mut pic.instance_id),
        _ => {}
    }
}
fn visit_raw_id(raw: &mut [u8], offset: usize, visit: &mut impl FnMut(&mut u32)) {
    if let Some(bytes) = raw.get_mut(offset..offset + 4) {
        let mut id = u32::from_le_bytes(bytes.try_into().unwrap());
        visit(&mut id);
        bytes.copy_from_slice(&id.to_le_bytes());
    }
}
fn visit_common_raw_id(
    common: &mut crate::model::shape::CommonObjAttr,
    raw: &mut [u8],
    visit: &mut impl FnMut(&mut u32),
) {
    let original = if common.instance_id != 0 {
        common.instance_id
    } else {
        raw.get(32..36)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
            .unwrap_or(0)
    };
    let mut id = original;
    visit(&mut id);
    if id != original {
        common.instance_id = id;
        if let Some(bytes) = raw.get_mut(32..36) {
            bytes.copy_from_slice(&id.to_le_bytes());
        }
    }
}
fn visit_ids(paragraphs: &mut [Paragraph], visit: &mut impl FnMut(&mut u32)) {
    visit_paragraphs(paragraphs, &mut |para| {
        visit_raw_id(&mut para.raw_header_extra, 6, visit);
        for control in &mut para.controls {
            match control {
                Control::Table(table) => {
                    visit_common_raw_id(&mut table.common, &mut table.raw_ctrl_data, visit)
                }
                Control::Picture(pic) => {
                    visit(&mut pic.common.instance_id);
                    visit(&mut pic.instance_id);
                }
                Control::Shape(shape) => visit_shape_ids(shape, visit),
                Control::Equation(eq) => {
                    visit_common_raw_id(&mut eq.common, &mut eq.raw_ctrl_data, visit)
                }
                Control::Footnote(note) => visit(&mut note.instance_id),
                Control::Endnote(note) => visit(&mut note.instance_id),
                _ => {}
            }
        }
    });
}
fn opaque_shape(shape: &crate::model::shape::ShapeObject) -> bool {
    use crate::model::shape::ShapeObject;
    match shape {
        ShapeObject::Line(line) => line.connector.is_some(),
        ShapeObject::Chart(_) | ShapeObject::Ole(_) => true,
        ShapeObject::Group(group) => group.children.iter().any(opaque_shape),
        _ => false,
    }
}
fn check_page_edit_support(paragraphs: &[Paragraph], copying: bool) -> Result<(), HwpError> {
    for (pi, p) in paragraphs.iter().enumerate() {
        if p.controls.iter().enumerate().any(|(ci, c)| {
            matches!(c, Control::SectionDef(_) | Control::ColumnDef(_))
                && (pi != 0 || p.controls[..ci].iter().any(|c| !structural(c)))
        }) {
            return Err(error(
                "중간에 구역·단 설정이 있는 문서는 쪽 편집을 지원하지 않습니다.",
            ));
        }
    }
    let mut copy = paragraphs.to_vec();
    let mut failure = None;
    visit_paragraphs(&mut copy, &mut |para| {
        if !para.orphan_field_ends.is_empty()
            || para.controls.iter().any(|c| {
                matches!(
                    c,
                    Control::Field(_) | Control::Header(_) | Control::Footer(_)
                )
            })
        {
            failure = Some("머리말·꼬리말 또는 필드가 있는 문서는 연결 범위 보존을 위해 쪽 편집을 지원하지 않습니다.");
        }
        if copying
            && para.controls.iter().any(|c| {
                matches!(
                    c,
                    Control::Bookmark(_) | Control::Form(_) | Control::Unknown(_)
                ) || matches!(c,Control::Shape(shape) if opaque_shape(shape))
            })
        {
            failure = Some("책갈피·양식·연결선·차트·OLE 등 연결 정보를 안전하게 복제할 수 없는 개체가 있습니다.");
        }
    });
    if let Some(message) = failure {
        return Err(error(message));
    }
    Ok(())
}

fn fingerprints(paragraphs: &[Paragraph]) -> Vec<String> {
    let mut fingerprints: Vec<_> = paragraphs
        .iter()
        .map(|p| {
            let mut p = p.clone();
            visit_ids(std::slice::from_mut(&mut p), &mut |id| *id = 0);
            take_prefix(&mut p);
            p.column_type = ColumnBreakType::None;
            p.raw_break_type = 0;
            p.line_segs.clear();
            p.reflow_seg_plan.clear();
            while p.ctrl_data_records.last() == Some(&None) {
                p.ctrl_data_records.pop();
            }
            format!("{p:?}")
        })
        .collect();
    fingerprints.sort();
    fingerprints
}
fn digest(values: &[String]) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for value in values {
        for byte in value.bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    format!("{hash:016x}")
}

impl DocumentCore {
    fn page_move_ranges(&self) -> Result<Vec<PageRange>, HwpError> {
        if self.render_normalized.iter().any(Option::is_some) {
            return Err(error(
                "자동 보정된 문서는 쪽 순서 이동을 지원하지 않습니다.",
            ));
        }
        let mut result = Vec::new();
        let mut offset = 0;
        for (section, pagination) in self.pagination.iter().enumerate() {
            let paragraphs = &self.document.sections[section].paragraphs;
            let mut pages = Vec::new();
            for page in &pagination.pages {
                let mut ids = BTreeSet::new();
                for col in &page.column_contents {
                    if col.endnote_flow {
                        return Err(error(
                            "미주가 이어진 문서는 쪽 경계 보존을 위해 이동을 지원하지 않습니다.",
                        ));
                    }
                    ids.extend(col.items.iter().filter_map(paragraph_of));
                    ids.extend(col.wrap_around_paras.iter().map(|p| p.para_index));
                    ids.extend(col.wrap_anchors.keys().copied());
                }
                if ids.iter().any(|&pi| pi >= paragraphs.len()) {
                    return Err(error("쪽과 원본 문단의 경계를 확인할 수 없습니다."));
                }
                pages.push(ids);
            }
            let mut starts = vec![(0usize, 0usize)];
            for boundary in 1..pages.len() {
                let left = pages[..boundary].iter().flat_map(|p| p.iter()).max();
                let right = pages[boundary..].iter().flat_map(|p| p.iter()).min();
                if !pages[boundary - 1].is_empty()
                    && !pages[boundary].is_empty()
                    && matches!((left,right),(Some(a),Some(b)) if a < b)
                {
                    starts.push((boundary, *right.unwrap()));
                }
            }
            for (i, &(page, para)) in starts.iter().enumerate() {
                if pages.is_empty() {
                    break;
                }
                let (page_end, para_end) = starts
                    .get(i + 1)
                    .copied()
                    .unwrap_or((pages.len(), paragraphs.len()));
                result.push(PageRange {
                    start: offset + page,
                    end: offset + page_end - 1,
                    section,
                    para_start: para,
                    para_end,
                });
            }
            offset += pages.len();
        }
        Ok(result)
    }

    pub fn get_page_move_ranges_native(&self) -> Result<String, HwpError> {
        let ranges = self.page_move_ranges()?;
        Ok(serde_json::json!({"ok":true,"pageCount":self.page_count(),"ranges":ranges.iter().map(|r|serde_json::json!({"startPage":r.start,"endPage":r.end})).collect::<Vec<_>>()}).to_string())
    }

    pub fn move_page_range_native(
        &mut self,
        source: usize,
        before: usize,
    ) -> Result<String, HwpError> {
        self.edit_pages_native(&format!("[{source}]"), "move", Some(before))
    }

    pub fn move_pages_native(&mut self, pages: &str, before: usize) -> Result<String, HwpError> {
        self.edit_pages_native(pages, "move", Some(before))
    }
    pub fn duplicate_pages_native(&mut self, pages: &str) -> Result<String, HwpError> {
        self.edit_pages_native(pages, "duplicate", None)
    }
    pub fn delete_pages_native(&mut self, pages: &str) -> Result<String, HwpError> {
        self.edit_pages_native(pages, "delete", None)
    }

    pub fn insert_blank_pages_native(&mut self, pages: &str) -> Result<String, HwpError> {
        self.edit_pages_native(pages, "insertBlank", None)
    }

    fn edit_pages_native(
        &mut self,
        pages_json: &str,
        action: &str,
        before: Option<usize>,
    ) -> Result<String, HwpError> {
        let requested: Vec<usize> = serde_json::from_str(pages_json)
            .map_err(|_| error("선택한 쪽 번호는 정수 배열이어야 합니다."))?;
        let count = self.page_count() as usize;
        if requested.is_empty() || requested.len() > count || requested.iter().any(|&p| p >= count)
        {
            return Err(error("선택한 쪽 번호가 올바르지 않습니다."));
        }
        let ranges = self.page_move_ranges()?;
        let selected: Vec<_> = ranges
            .iter()
            .filter(|r| requested.iter().any(|p| *p >= r.start && *p <= r.end))
            .cloned()
            .collect();
        let section = selected
            .first()
            .ok_or_else(|| error("선택한 쪽의 문단 범위를 찾을 수 없습니다."))?
            .section;
        if selected.iter().any(|r| r.section != section) {
            return Err(error(
                "용지·여백 보존을 위해 한 구역 안의 쪽만 함께 편집할 수 있습니다.",
            ));
        }
        let section_ranges: Vec<_> = ranges
            .iter()
            .filter(|r| r.section == section)
            .cloned()
            .collect();
        let section_start = section_ranges.first().unwrap().start;
        let section_end = section_ranges.last().unwrap().end + 1;
        let selected_pages: Vec<_> = selected.iter().flat_map(|r| r.start..=r.end).collect();
        let selected_starts: BTreeSet<_> = selected.iter().map(|r| r.start).collect();
        let selected_count = selected_pages.len();
        let first_selected = selected[0].start;
        let last_selected = selected.last().unwrap().end;
        // (원본 묶음, 복사본 여부, 결과에서 선택할지 여부, 새 빈 쪽 여부)
        let mut order: Vec<(PageRange, bool, bool, bool)>;
        let mut changed = true;
        if action == "move" {
            let before = before.unwrap();
            if before < section_start || before > section_end {
                return Err(error("서로 다른 구역 사이의 쪽 이동은 지원하지 않습니다."));
            }
            if before != section_end && !section_ranges.iter().any(|r| r.start == before) {
                return Err(error(
                    "이어진 문단·표가 있는 쪽 묶음 중간에는 놓을 수 없습니다.",
                ));
            }
            let mut remaining: Vec<_> = section_ranges
                .iter()
                .filter(|r| !selected_starts.contains(&r.start))
                .cloned()
                .collect();
            let insert = remaining.iter().take_while(|r| r.start < before).count();
            remaining.splice(insert..insert, selected.clone());
            changed = remaining
                .iter()
                .map(|r| r.start)
                .ne(section_ranges.iter().map(|r| r.start));
            order = remaining
                .into_iter()
                .map(|r| {
                    let chosen = selected_starts.contains(&r.start);
                    (r, false, chosen, false)
                })
                .collect();
        } else if action == "duplicate" {
            order = Vec::new();
            for range in &section_ranges {
                order.push((range.clone(), false, false, false));
                if range.end == last_selected {
                    order.extend(selected.iter().cloned().map(|r| (r, true, true, false)));
                }
            }
        } else if action == "insertBlank" {
            order = Vec::new();
            for range in &section_ranges {
                order.push((range.clone(), false, false, false));
                if selected_starts.contains(&range.start) {
                    order.push((range.clone(), false, true, true));
                }
            }
        } else {
            order = section_ranges
                .iter()
                .filter(|r| !selected_starts.contains(&r.start))
                .cloned()
                .map(|r| (r, false, false, false))
                .collect();
        }
        let mut new_selected = Vec::new();
        let mut next_page = section_start;
        for (range, _, chosen, blank) in &order {
            let length = if *blank {
                1
            } else {
                range.end - range.start + 1
            };
            if *chosen {
                new_selected.extend(next_page..next_page + length);
            }
            next_page += length;
        }
        let delete_all = action == "delete" && order.is_empty();
        let expected_count = match action {
            "duplicate" => count + selected_count,
            "insertBlank" => count + selected.len(),
            "delete" => count - selected_count + usize::from(delete_all),
            _ => count,
        };
        if action == "delete" {
            new_selected.push(first_selected.min(expected_count - 1));
        }
        let new_start = new_selected.first().copied().unwrap_or(first_selected);
        if !changed {
            return Ok(serde_json::json!({"ok":true,"changed":false,"action":action,"selectedPages":selected_pages,"affectedPages":selected_pages,"newSelectedPages":new_selected,"startPage":first_selected,"endPage":last_selected,"newStartPage":new_start,"pageCount":count}).to_string());
        }
        if !self.hidden_header_footer.is_empty() {
            return Err(error(
                "쪽별 머리말 표시 설정이 있는 문서는 쪽 편집을 지원하지 않습니다.",
            ));
        }
        check_page_edit_support(&self.document.sections[section].paragraphs, false)?;
        if action == "duplicate" {
            let content: Vec<_> = selected
                .iter()
                .flat_map(|r| {
                    self.document.sections[section].paragraphs[r.para_start..r.para_end]
                        .iter()
                        .cloned()
                })
                .collect();
            check_page_edit_support(&content, true)?;
        }
        let before_fingerprints = fingerprints(&self.document.sections[section].paragraphs);
        let before_styles = format!("{:?}", self.document.doc_info);
        let mut backup = self.document.clone();
        let active_field = self.active_field.clone();
        let mut used = BTreeSet::new();
        if action == "duplicate" {
            for section in &mut backup.sections {
                visit_ids(&mut section.paragraphs, &mut |id| {
                    if *id != 0 {
                        used.insert(*id);
                    }
                });
            }
        }
        let mut next_id = 0x40000001u32;
        let mut allocate = |id: &mut u32| {
            while used.contains(&next_id) || next_id == 0 {
                next_id = next_id.wrapping_add(1);
            }
            *id = next_id;
            used.insert(next_id);
            next_id = next_id.wrapping_add(1);
        };
        let original = &backup.sections[section].paragraphs;
        let first_break = original
            .first()
            .map(|p| (p.column_type, p.raw_break_type))
            .ok_or_else(|| error("구역에 편집할 문단이 없습니다."))?;
        let mut original = original.clone();
        let prefix = take_prefix(&mut original[0]);
        let mut rewritten = Vec::new();
        for (i, (range, copy, _, blank)) in order.iter().enumerate() {
            let mut content = if *blank {
                // 새 빈 쪽은 제목의 직접 서식 대신 문서 바탕글 스타일을 사용한다.
                let mut paragraph = Paragraph::new_empty();
                if let Some(style) = self.document.doc_info.styles.first() {
                    paragraph.para_shape_id = style.para_shape_id;
                    paragraph.style_id = 0;
                    paragraph.set_single_char_shape(u32::from(style.char_shape_id));
                }
                vec![paragraph]
            } else {
                original[range.para_start..range.para_end].to_vec()
            };
            if *copy {
                visit_ids(&mut content, &mut allocate);
            }
            let head = &mut content[0];
            if i == 0 {
                head.column_type = if matches!(
                    first_break.0,
                    ColumnBreakType::Section | ColumnBreakType::MultiColumn
                ) {
                    first_break.0
                } else {
                    ColumnBreakType::None
                };
                head.raw_break_type = (head.raw_break_type & !7) | (first_break.1 & 3);
            } else {
                head.column_type = ColumnBreakType::Page;
                head.raw_break_type = (head.raw_break_type & !7) | 4;
            }
            rewritten.extend(content);
        }
        if delete_all {
            let mut blank = Paragraph::new_empty_like(&original[0]);
            if matches!(
                first_break.0,
                ColumnBreakType::Section | ColumnBreakType::MultiColumn
            ) {
                blank.column_type = first_break.0;
            }
            blank.raw_break_type = first_break.1 & 3;
            rewritten.push(blank);
        }
        let expected_fingerprints = fingerprints(&rewritten);
        put_prefix(&mut rewritten[0], prefix);
        self.document.sections[section].paragraphs = rewritten;
        self.document.sections[section].raw_stream = None;
        self.active_field = None;
        self.recompose_section(section);
        self.paginate();
        let after = fingerprints(&self.document.sections[section].paragraphs);
        let expected_groups: Vec<_> = if delete_all {
            vec![1]
        } else {
            order
                .iter()
                .map(|(r, _, _, blank)| if *blank { 1 } else { r.end - r.start + 1 })
                .collect()
        };
        let actual_groups: Vec<_> = self
            .page_move_ranges()
            .map(|ranges| {
                ranges
                    .iter()
                    .filter(|r| r.section == section)
                    .map(|r| r.end - r.start + 1)
                    .collect()
            })
            .unwrap_or_default();
        if self.page_count() as usize != expected_count
            || expected_groups != actual_groups
            || expected_fingerprints != after
            || before_styles != format!("{:?}", self.document.doc_info)
        {
            self.document = backup;
            self.active_field = active_field;
            self.recompose_section(section);
            self.paginate();
            return Err(error(
                "본문·표·서식 또는 쪽 경계 보존을 확인할 수 없어 쪽 편집을 취소했습니다.",
            ));
        }
        self.event_log.clear();
        let paragraphs = &self.document.sections[section].paragraphs;
        let tables = paragraphs
            .iter()
            .flat_map(|p| p.controls.iter())
            .filter(|c| matches!(c, Control::Table(_)))
            .count();
        Ok(serde_json::json!({"ok":true,"changed":true,"action":action,"selectedPages":selected_pages,"affectedPages":selected_pages,"newSelectedPages":new_selected,"startPage":first_selected,"endPage":last_selected,"newStartPage":new_start,"pageCount":expected_count,"sectionIndex":section,"paragraphCount":paragraphs.len(),"tableCount":tables,"preservationVerified":true,"contentDigestBefore":digest(&before_fingerprints),"contentDigestAfter":digest(&after),"expectedContentDigest":digest(&expected_fingerprints),"copiedPageCount":if action=="duplicate"{selected_count}else{0},"deletedPageCount":if action=="delete"{selected_count}else{0},"insertedPageCount":if action=="insertBlank"{selected.len()}else{0}}).to_string())
    }
}
