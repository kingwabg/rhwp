//! [편집 훅 단일화 2026-08-05] 트랙 2 검증 핀.
//!
//! 어울림(Square) 리플로우 훅을 편집 커밋 단일 지점(paginate 소비)으로 배선한 뒤,
//! 종전 훅 미배선 경로(Enter/병합/붙여넣기/그림 이동)와 batch 일괄 소비, 조기 탈출
//! 판정, 페이지 집합 축소의 전폭 원복 경로를 잠근다.
//! 빌더/워크는 officex_square_move_reflow.rs 의 것을 재사용(복제)한다.
use rhwp::renderer::render_tree::{RenderNode, RenderNodeType};
use rhwp::wasm_api::HwpDocument;

// ─── 공용 빌더/워크 (officex_square_move_reflow.rs 재사용) ───────────────

fn doc_with_five_paras() -> HwpDocument {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    let filler = "가나다라마바사 아자차카타파하 강물이 흐르고 산이 높다 바람이 분다";
    let flen = filler.chars().count();
    doc.insert_text(0, 0, 0, filler).unwrap();
    for i in 0..4u32 {
        doc.split_paragraph_native(0, i as usize, flen).unwrap();
        doc.insert_text(0, i + 1, 0, filler).unwrap();
    }
    doc
}

fn partial_square_table(doc: &mut HwpDocument, para_idx: usize) -> (u32, u32) {
    let c: serde_json::Value = serde_json::from_str(&doc.create_table_ex(&format!(
        r#"{{"sectionIdx":0,"paraIdx":{para_idx},"charOffset":0,"rowCount":2,"colCount":2,"treatAsChar":false,"colWidths":[3000,3000]}}"#
    )).unwrap()).unwrap();
    let (pi, ci) = (
        c["paraIdx"].as_u64().unwrap() as u32,
        c["controlIdx"].as_u64().unwrap() as u32,
    );
    doc.delete_table_column(0, pi, ci, 1).unwrap();
    doc.set_table_properties(0, pi, ci,
        r#"{"treatAsChar":false,"textWrap":"Square","vertRelTo":"Para","horzRelTo":"Column","vertOffset":0}"#
    ).unwrap();
    (pi, ci)
}

/// 본문 TextLine(표 내부 제외)이 주어진 상자와 겹치는 수를 센다.
fn body_overlap_with(doc: &HwpDocument, tx: f64, ty: f64, tw: f64, th: f64) -> usize {
    let mut count = 0;
    for pg in 0..doc.page_count() {
        let tree = doc.build_page_render_tree(pg).unwrap();
        fn walk(n: &RenderNode, in_table: bool, out: &mut Vec<(f64, f64, f64, f64)>) {
            if matches!(n.node_type, RenderNodeType::Table(_)) {
                for c in &n.children {
                    walk(c, true, out);
                }
                return;
            }
            if matches!(n.node_type, RenderNodeType::TextBox) {
                return; // 글상자 내부 줄은 본문이 아니다
            }
            if let RenderNodeType::TextLine(tl) = &n.node_type {
                if !in_table && tl.para_index.is_some() {
                    out.push((n.bbox.x, n.bbox.y, n.bbox.width, n.bbox.height));
                }
            }
            for c in &n.children {
                walk(c, in_table, out);
            }
        }
        let mut lines = Vec::new();
        walk(&tree.root, false, &mut lines);
        count += lines
            .iter()
            .filter(|(x, y, w, h)| {
                *x < tx + tw - 1.0 && x + w > tx + 1.0 && *y < ty + th - 1.0 && y + h > ty + 1.0
            })
            .count();
    }
    count
}

/// 본문 TextLine 이 표 상자와 겹치는 수 (officex_square_move_reflow.rs 와 동일 판정).
fn overlap_count(doc: &HwpDocument, pi: u32, ci: u32) -> usize {
    let bb: serde_json::Value =
        serde_json::from_str(&doc.get_table_bbox(0, pi, ci).unwrap()).unwrap();
    body_overlap_with(
        doc,
        bb["x"].as_f64().unwrap(),
        bb["y"].as_f64().unwrap(),
        bb["width"].as_f64().unwrap(),
        bb["height"].as_f64().unwrap(),
    )
}

/// 문단 line_segs 의 column_start 목록 (debug_line_seg_tags: (vpos, cs, tag)).
fn seg_cs(doc: &HwpDocument, para_idx: u32) -> Vec<i32> {
    let parsed: Vec<(i32, i32, u32)> =
        serde_json::from_str(&doc.debug_line_seg_tags(0, para_idx).unwrap()).unwrap();
    parsed.iter().map(|t| t.1).collect()
}

/// 어느 문단이든 좁힘(cs>0) 줄을 가진 문단 목록.
fn narrowed_paras(doc: &HwpDocument) -> Vec<u32> {
    (0..doc.get_paragraph_count(0).unwrap())
        .filter(|&p| seg_cs(doc, p).iter().any(|&c| c > 0))
        .collect()
}

// ─── 핀 1: Enter (문단 분할 — host 인덱스가 밀리는 구조 편집) ─────────────

#[test]
fn enter_beside_square_table_rewraps() {
    let mut doc = doc_with_five_paras();
    doc.split_paragraph_native(0, 2, 33).unwrap();
    let (pi, ci) = partial_square_table(&mut doc, 3);
    doc.move_table_offset(0, pi, ci, 6000, 3000).unwrap();
    assert_eq!(
        overlap_count(&doc, pi, ci),
        0,
        "사전조건: 이동 직후 겹침 없음"
    );
    assert!(!narrowed_paras(&doc).is_empty(), "사전조건: 좁힘 문단 존재");

    // Enter: 표 **앞** 문단을 갈라 host 인덱스를 3→4 로 민다 — 훅이 stale 인덱스로
    // 돌면 host 미스매치(밴드 0)로 좁힘 흔적이 전폭 오원복된다.
    doc.split_paragraph_native(0, 1, 10).unwrap();
    let (pi, ci) = (pi + 1, ci);
    assert_eq!(
        overlap_count(&doc, pi, ci),
        0,
        "Enter 후 본문 줄이 표를 뚫음"
    );
    assert!(
        !narrowed_paras(&doc).is_empty(),
        "Enter 후 좁힘 cs 기록이 사라짐 — 재줄바꿈 미발화"
    );
}

// ─── 핀 2: Backspace 병합 ────────────────────────────────────────────────

#[test]
fn backspace_merge_beside_square_table_rewraps() {
    let mut doc = doc_with_five_paras();
    doc.split_paragraph_native(0, 2, 33).unwrap();
    let (pi, ci) = partial_square_table(&mut doc, 3);
    doc.move_table_offset(0, pi, ci, 6000, 3000).unwrap();
    assert_eq!(
        overlap_count(&doc, pi, ci),
        0,
        "사전조건: 이동 직후 겹침 없음"
    );

    // 병합: 표 앞 문단 2 를 1 로 — host 인덱스 3→2. 병합 결과 문단이 밴드 기준 재줄바꿈.
    doc.merge_paragraph_native(0, 2).unwrap();
    let (pi, ci) = (pi - 1, ci);
    assert_eq!(
        overlap_count(&doc, pi, ci),
        0,
        "병합 후 본문 줄이 표를 뚫음"
    );
    assert!(
        !narrowed_paras(&doc).is_empty(),
        "병합 후 좁힘 cs 기록이 사라짐 — 재줄바꿈 미발화"
    );
}

// ─── 핀 3: 붙여넣기 (다문단) ─────────────────────────────────────────────

#[test]
fn paste_beside_square_table_rewraps() {
    let mut doc = doc_with_five_paras();
    doc.split_paragraph_native(0, 2, 33).unwrap();
    let (pi, ci) = partial_square_table(&mut doc, 3);
    doc.move_table_offset(0, pi, ci, 6000, 3000).unwrap();
    assert_eq!(
        overlap_count(&doc, pi, ci),
        0,
        "사전조건: 이동 직후 겹침 없음"
    );

    // 두 문단에 걸친 선택을 복사 → 표 옆 문단(4)에 붙여넣기(문단 삽입 경로).
    doc.copy_selection(0, 0, 0, 1, 20).unwrap();
    let r: serde_json::Value = serde_json::from_str(&doc.paste_internal(0, 4, 0).unwrap()).unwrap();
    assert_eq!(r["ok"].as_bool(), Some(true), "붙여넣기 실패: {r}");
    assert_eq!(
        overlap_count(&doc, pi, ci),
        0,
        "붙여넣기 후 본문 줄이 표를 뚫음"
    );
    assert!(
        !narrowed_paras(&doc).is_empty(),
        "붙여넣기 후 좁힘 cs 기록이 사라짐 — 재줄바꿈 미발화"
    );
}

// ─── 핀 4: batch — pending 누적·end_batch 일괄 소비 ──────────────────────

#[test]
fn batch_edits_rewrap_once_at_end_batch() {
    let mut doc = doc_with_five_paras();
    doc.split_paragraph_native(0, 2, 33).unwrap();
    let (pi, ci) = partial_square_table(&mut doc, 3);
    doc.move_table_offset(0, pi, ci, 6000, 3000).unwrap();
    let narrowed = narrowed_paras(&doc);
    assert!(!narrowed.is_empty(), "사전조건: 좁힘 문단 존재");
    let target = narrowed[0];

    doc.begin_batch().unwrap();
    for _ in 0..3 {
        doc.insert_text(0, target, 0, "가나 ").unwrap();
    }
    // batch 중: insert 경로의 전폭 reflow 가 cs 를 리셋한 채 유지 — 훅이 돌지 않는다.
    assert!(
        seg_cs(&doc, target).iter().all(|&c| c == 0),
        "batch 중에 좁힘 훅이 돌았다 — paginate 스킵 위반"
    );
    doc.end_batch().unwrap();
    // end_batch 의 paginate 1회에서 pending 일괄 소비 → 좁힘 확정.
    assert!(
        seg_cs(&doc, target).iter().any(|&c| c > 0),
        "end_batch 에서 좁힘이 확정되지 않음 — pending 소비 실패"
    );
    assert_eq!(
        overlap_count(&doc, pi, ci),
        0,
        "end_batch 후 본문 줄이 표를 뚫음"
    );
}

// ─── 핀 5: 조기 탈출 흔적 판정 단위 핀 (비용 회귀 방지) ──────────────────

#[test]
fn full_width_stored_segs_do_not_trip_trace() {
    use rhwp::document_core::paragraph_has_narrow_trace;
    use rhwp::model::paragraph::{LineSeg, Paragraph};
    let full_hu = 42520;
    let mut para = Paragraph::default();
    para.line_segs = vec![
        LineSeg {
            segment_width: full_hu,
            ..Default::default()
        },
        // 반올림 오차 범위(800HU 미만)의 전폭 근사도 흔적이 아니다
        LineSeg {
            segment_width: full_hu - 500,
            ..Default::default()
        },
    ];
    assert!(
        !paragraph_has_narrow_trace(&para, full_hu),
        "저장 전폭 lineseg 가 흔적으로 오판 — 조기 탈출이 죽어 타이핑 비용 회귀"
    );
    para.line_segs[0].segment_width = full_hu - 2000;
    assert!(
        paragraph_has_narrow_trace(&para, full_hu),
        "좁힘 sw 는 흔적"
    );
    para.line_segs[0].segment_width = full_hu;
    para.line_segs[0].column_start = 1200;
    assert!(
        paragraph_has_narrow_trace(&para, full_hu),
        "column_start 는 흔적"
    );
}

// ─── 핀 6: 그림 host 일반화 ──────────────────────────────────────────────

/// 1x1 투명 PNG (67바이트) — 그림 삽입 핀용 최소 바이너리.
const PNG_1X1: [u8; 67] = [
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x62, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
    0x42, 0x60, 0x82,
];

/// 4줄짜리 한 문단 + 빈 앵커 문단 (officex_square_move_reflow.rs 재사용).
fn doc_with_long_para() -> (HwpDocument, usize) {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    let long = "가나다라마바사 아자차카타파하 강물이 흐르고 산이 높다 바람이 분다 구름이 간다 하늘이 푸르다 나무가 자란다 새가 웃는다 경치가 아름답다 보리가 여무다 들판이 넓다 여기에 표를 놓으면 글이 어떻게 흐르는지 본다 뒤에 말을 더 붙여 여러 줄이 되도록 한다 그래야 어울림이 보인다";
    doc.insert_text(0, 0, 0, long).unwrap();
    let flen = long.chars().count();
    doc.split_paragraph_native(0, 0, flen).unwrap();
    let host = doc.get_paragraph_count(0).unwrap() as usize - 1;
    (doc, host)
}

/// 첫 페이지 렌더트리에서 Image 노드 bbox 를 찾는다.
fn image_bbox(doc: &HwpDocument) -> Option<(f64, f64, f64, f64)> {
    let tree = doc.build_page_render_tree(0).unwrap();
    fn walk(n: &RenderNode, out: &mut Option<(f64, f64, f64, f64)>) {
        if out.is_some() {
            return;
        }
        if matches!(n.node_type, RenderNodeType::Image(_)) {
            *out = Some((n.bbox.x, n.bbox.y, n.bbox.width, n.bbox.height));
            return;
        }
        for c in &n.children {
            walk(c, out);
        }
    }
    let mut out = None;
    walk(&tree.root, &mut out);
    out
}

#[test]
fn square_picture_host_narrows_neighbors() {
    let (mut doc, host) = doc_with_long_para();
    // insertPicture 기본값: treat_as_char=false / textWrap=Square / rel=Para/Para —
    // 빈 host 문단의 부분폭 어울림 그림.
    let r: serde_json::Value = serde_json::from_str(
        &doc.insert_picture(
            0,
            host as u32,
            0,
            "[]",
            &PNG_1X1,
            6000,
            6000,
            1,
            1,
            "png",
            "",
            None,
            None,
        )
        .unwrap(),
    )
    .unwrap();
    let ci = r["controlIdx"].as_u64().unwrap() as u32;
    let pi = r["paraIdx"].as_u64().unwrap() as u32;

    // 그림을 본문 문단 줄들 위로 이동 → 옆 문단 좁힘 발화.
    // (insertPicture 실측 기본값은 Paper/Paper 기준 — 종이 좌상단 원점 오프셋.
    //  본문 좌단 x≈113px=8504HU, 첫 줄 y≈132px → 둘째 줄 언저리 10500HU 로 겹친다.)
    doc.set_picture_properties(0, pi, ci, r#"{"vertOffset":10500,"horzOffset":8504}"#)
        .unwrap();
    let (ix, iy, iw, ih) = image_bbox(&doc).expect("그림 노드가 렌더트리에 없음");
    assert_eq!(
        body_overlap_with(&doc, ix, iy, iw, ih),
        0,
        "본문 줄이 어울림 그림을 뚫음 — 그림 host 일반화 미발화"
    );
    assert!(
        !narrowed_paras(&doc).is_empty(),
        "그림 옆 문단에 좁힘 cs 기록이 없음"
    );

    // 오프셋 이동 시 재좁힘 — setPictureProperties 경로가 훅에 걸리는지.
    doc.set_picture_properties(0, pi, ci, r#"{"vertOffset":11500,"horzOffset":12000}"#)
        .unwrap();
    let (ix, iy, iw, ih) = image_bbox(&doc).expect("이동 후 그림 노드가 렌더트리에 없음");
    assert_eq!(
        body_overlap_with(&doc, ix, iy, iw, ih),
        0,
        "그림 이동 후 본문 줄이 그림을 뚫음 — 재좁힘 미발화"
    );
}

// ─── 핀 7: 글상자 float 의 para_tops 오염 차단 ──────────────────────────

#[test]
fn textbox_shape_does_not_pollute_para_tops() {
    let mut doc = doc_with_five_paras();
    doc.split_paragraph_native(0, 2, 33).unwrap();
    let (pi, ci) = partial_square_table(&mut doc, 3);
    doc.move_table_offset(0, pi, ci, 6000, 3000).unwrap();
    let narrowed_before = narrowed_paras(&doc);
    assert!(!narrowed_before.is_empty(), "사전조건: 좁힘 문단 존재");

    // float 글상자를 첫 문단 앵커로 페이지 아래쪽에 배치 — 내부 문단 인덱스(0..)가
    // 본문 인덱스와 충돌하는 구성. walk 가 TextBox 하위로 내려가면 para_tops 가
    // 글상자 y 로 오염되어 엉뚱한 문단이 좁혀지거나 좁힘이 풀린다.
    doc.create_shape_control(
        r#"{"sectionIdx":0,"paraIdx":0,"charOffset":0,"shapeType":"textbox","treatAsChar":false,"width":12000,"height":6000,"vertOffset":30000,"horzOffset":0}"#,
    )
    .unwrap();
    // 편집 → 훅 1회.
    doc.insert_text(0, 4, 0, "가").unwrap();

    assert_eq!(
        overlap_count(&doc, pi, ci),
        0,
        "글상자 추가 후 본문이 표를 뚫음"
    );
    // 표 위쪽(밴드 밖) 문단들은 전폭 유지 — 스퓨리어스 좁힘 없음.
    for p in 0..3u32 {
        assert!(
            seg_cs(&doc, p).iter().all(|&c| c == 0),
            "밴드 밖 문단 {p} 가 글상자 오염으로 좁혀짐: {:?}",
            seg_cs(&doc, p)
        );
    }
}

// ─── 핀 9 (phase A): 텍스트 있는 그림 host 의 자기 줄 좁힘 ──────────────

#[test]
fn picture_text_host_self_wrap() {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    let long = "가나다라마바사 아자차카타파하 강물이 흐르고 산이 높다 바람이 분다 구름이 간다 하늘이 푸르다 나무가 자란다 새가 웃는다 경치가 아름답다 보리가 여무다 들판이 넓다 여기에 표를 놓으면 글이 어떻게 흐르는지 본다 뒤에 말을 더 붙여 여러 줄이 되도록 한다 그래야 어울림이 보인다";
    doc.insert_text(0, 0, 0, long).unwrap();

    // **텍스트가 있는** 문단 0 자신에 float Square 그림 삽입 — 표 host 와 달리
    // 공백뿐 필터를 통과해야 한다(phase A).
    let r: serde_json::Value = serde_json::from_str(
        &doc.insert_picture(
            0, 0, 0, "[]", &PNG_1X1, 6000, 6000, 1, 1, "png", "", None, None,
        )
        .unwrap(),
    )
    .unwrap();
    let ci = r["controlIdx"].as_u64().unwrap() as u32;
    let pi = r["paraIdx"].as_u64().unwrap() as u32;
    assert_eq!(pi, 0, "그림이 문단 0 에 앵커되어야 한다: {r}");

    // 그림을 자기 문단 줄들 위(본문 좌단, 둘째 줄 언저리)로 배치.
    doc.set_picture_properties(0, pi, ci, r#"{"vertOffset":10500,"horzOffset":8504}"#)
        .unwrap();
    let (ix, iy, iw, ih) = image_bbox(&doc).expect("그림 노드가 렌더트리에 없음");
    assert_eq!(
        body_overlap_with(&doc, ix, iy, iw, ih),
        0,
        "host 자신의 줄이 그림을 뚫음 — 자기 밴드 좁힘 미발화"
    );
    // 자기 줄에 좁힘 cs/sw 기록 — typeset 앵커 arming(cs/sw 소비)이 성립하는 전제.
    assert!(
        seg_cs(&doc, 0).iter().any(|&c| c > 0),
        "host 문단 자기 줄에 좁힘 cs 기록이 없음: {:?}",
        seg_cs(&doc, 0)
    );

    // host 문단에 타이핑 — 편집 훅이 자기 밴드 재줄바꿈을 유지하는지.
    doc.insert_text(0, 0, 0, "머리말 ").unwrap();
    let (ix, iy, iw, ih) = image_bbox(&doc).expect("타이핑 후 그림 노드가 렌더트리에 없음");
    assert_eq!(
        body_overlap_with(&doc, ix, iy, iw, ih),
        0,
        "타이핑 후 host 줄이 그림을 뚫음"
    );
}

// ─── 핀 8: host 삭제 → 다른 페이지 흔적 문단의 전폭 원복 ────────────────

#[test]
fn host_delete_restores_full_width() {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    let filler = "가나다라마바사 아자차카타파하 강물이 흐르고 산이 높다 바람이 분다";
    let flen = filler.chars().count();
    doc.insert_text(0, 0, 0, filler).unwrap();
    // 2쪽을 만들 만큼 문단을 쌓는다(문단당 1줄 × 50) — host 와 옆 문단이 2쪽에 놓인다.
    for i in 0..50u32 {
        doc.split_paragraph_native(0, i as usize, flen).unwrap();
        doc.insert_text(0, i + 1, 0, filler).unwrap();
    }
    let host = doc.get_paragraph_count(0).unwrap() as usize - 1;
    let (pi, ci) = partial_square_table(&mut doc, host);
    // 표를 위로 끌어 2쪽의 앞 문단들과 겹치게 — 첫 페이지 밖 좁힘 흔적 생성.
    doc.move_table_offset(0, pi, ci, 6000, -9000).unwrap();
    assert!(
        doc.page_count() >= 2,
        "사전조건: 2쪽 문서 (현재 {}쪽)",
        doc.page_count()
    );
    let bb: serde_json::Value =
        serde_json::from_str(&doc.get_table_bbox(0, pi, ci).unwrap()).unwrap();
    assert!(
        bb["pageIndex"].as_u64().unwrap_or(0) >= 1,
        "사전조건: 표가 첫 페이지 밖에 있어야 페이지 집합 축소를 검증한다: {bb}"
    );
    let narrowed = narrowed_paras(&doc);
    assert!(!narrowed.is_empty(), "사전조건: 좁힘 흔적 문단 존재");

    // host 문단 삭제 → 어울림 개체 소멸. 페이지 집합 축소가 흔적 페이지를 빼먹으면
    // 첫 페이지 밖의 흔적 문단이 좁힘 채 방치된다.
    doc.delete_paragraph(0, pi).unwrap();
    for p in narrowed {
        assert!(
            seg_cs(&doc, p).iter().all(|&c| c == 0),
            "host 삭제 후 문단 {p} 가 전폭으로 원복되지 않음: {:?}",
            seg_cs(&doc, p)
        );
    }
}
