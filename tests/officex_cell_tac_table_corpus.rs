//! [트랙4 ⑤ / oracle-corpus-mining-20260805 (g)] 셀 안 TAC 표 — 코퍼스 실파일 핀.
//!
//! 코퍼스 확정(549건): 셀 문단도 본문과 **동일한 lineseg 물리**를 저장한다.
//! TAC 표는 셀 안에서도 "큰 글자" — 자기 줄 여부는 별도 시멘틱이 아니라 폭 기준
//! 줄바꿈의 귀결. 줄높이 = 표높이 + 外상하마진(횡단 법칙 #1).
//!
//! fixture: samples/2025 행정업무운영 편람(최종).hwp (10.7MB, 파싱 실측 ≈0.6s —
//! 파일 1개만 사용).
//! - s2#417/cell0#0: 자기 줄형 — seg 1개 lh=39331(=내부 표 h 39049 + 마진 282),
//!   sw=38096(셀 내폭).
//! - s2#413/cell5#0: 인라인형 — seg 1개 lh=1848(=내부 표 h, 마진 0; 표와 공백
//!   텍스트 같은 줄), sw=32168.
use std::fs;
use std::path::Path;

use rhwp::document_core::DocumentCore;
use rhwp::model::control::Control;
use rhwp::renderer::render_tree::{RenderNode, RenderNodeType};

const HU_PER_PX: f64 = 7200.0 / 96.0; // 75 HU = 1px

fn core() -> DocumentCore {
    let repo_root = env!("CARGO_MANIFEST_DIR");
    let path = Path::new(repo_root).join("samples/2025 행정업무운영 편람(최종).hwp");
    let bytes = fs::read(&path).unwrap_or_else(|e| panic!("read {}: {}", path.display(), e));
    DocumentCore::from_bytes(&bytes).expect("parse 2025 행정업무운영 편람(최종).hwp")
}

fn table_at<'a>(
    core: &'a DocumentCore,
    sec: usize,
    pi: usize,
    ci: usize,
) -> &'a rhwp::model::table::Table {
    match &core.document().sections[sec].paragraphs[pi].controls[ci] {
        Control::Table(t) => t,
        other => panic!("s{sec}#{pi} ctrl{ci} 가 표가 아니다: {other:?}"),
    }
}

fn inner_table(p: &rhwp::model::paragraph::Paragraph) -> &rhwp::model::table::Table {
    p.controls
        .iter()
        .find_map(|c| match c {
            Control::Table(t) => Some(t.as_ref()),
            _ => None,
        })
        .expect("셀 문단 안 표")
}

/// 저장 lineseg 가 코퍼스 채굴 수치와 일치 — 자기 줄형(s2#417)과 인라인형(s2#413).
#[test]
fn cell_tac_stored_linesegs_match_corpus_mining() {
    let core = core();

    // 자기 줄형: 1x1 래퍼 표 cell0#0 안의 대형 TAC 표.
    let outer = table_at(&core, 2, 417, 0);
    assert!(outer.common.treat_as_char, "s2#417 ctrl0 은 TAC 표");
    let p = &outer.cells[0].paragraphs[0];
    assert_eq!(p.line_segs.len(), 1, "자기 줄형: 셀 문단 seg 1개");
    let seg = &p.line_segs[0];
    assert_eq!(
        (
            seg.text_start,
            seg.vertical_pos,
            seg.line_height,
            seg.segment_width
        ),
        (0, 0, 39331, 38096),
        "s2#417/cell0#0 저장 lineseg = 코퍼스 채굴 수치"
    );
    let inner = inner_table(p);
    assert!(inner.common.treat_as_char, "내부 표도 TAC");
    assert_eq!(
        seg.line_height as i64,
        inner.common.height as i64
            + inner.outer_margin_top as i64
            + inner.outer_margin_bottom as i64,
        "횡단 법칙 #1: 줄높이 = 표높이 + 外상하마진"
    );

    // 인라인형: 15x7 표 cell5#0 — 소형 표가 공백 텍스트와 같은 줄(seg 1개).
    let host = table_at(&core, 2, 413, 4);
    assert!(host.common.treat_as_char, "s2#413 ctrl4 는 TAC 표");
    assert_eq!((host.row_count, host.col_count), (15, 7));
    let p = &host.cells[5].paragraphs[0];
    assert_eq!(
        p.line_segs.len(),
        1,
        "인라인형: 표+텍스트 같은 줄 — 자기 줄 시멘틱 없음(폭 기준)"
    );
    let seg = &p.line_segs[0];
    assert_eq!(
        (
            seg.text_start,
            seg.vertical_pos,
            seg.line_height,
            seg.segment_width
        ),
        (0, 1416, 1848, 32168),
        "s2#413/cell5#0 저장 lineseg = 코퍼스 채굴 수치"
    );
    let inner = inner_table(p);
    assert_eq!(
        seg.line_height as i64,
        inner.common.height as i64
            + inner.outer_margin_top as i64
            + inner.outer_margin_bottom as i64,
        "횡단 법칙 #1 (마진 0 → lh = h)"
    );
}

/// 렌더 트리: 셀 안 표가 저장 조판대로 놓인다.
/// - s2#413 host 표 높이 = 저장 seg lh(55136HU≈735.1px), 인라인 소형 표(1848HU≈24.6px)
///   는 host bbox 안.
/// - s2#417 표 top 은 s2#413 표 top(vpos=0) + 저장 vpos 11026HU≈147.0px (다음 쪽).
#[test]
fn cell_tac_render_tree_places_tables_per_stored_layout() {
    let core = core();
    let page_count = core.page_count();

    // (page, bbox) 수집 — 본문 표는 (section=2, pi) 로 식별.
    fn collect(n: &RenderNode, out: &mut Vec<(Option<usize>, Option<usize>, f64, f64, f64, f64)>) {
        if let RenderNodeType::Table(tn) = &n.node_type {
            out.push((
                tn.section_index,
                tn.para_index,
                n.bbox.x,
                n.bbox.y,
                n.bbox.width,
                n.bbox.height,
            ));
        }
        for c in &n.children {
            collect(c, out);
        }
    }

    let mut host413: Option<(u32, (f64, f64, f64, f64), Vec<(f64, f64, f64, f64)>)> = None;
    let mut t417: Option<(u32, (f64, f64, f64, f64))> = None;
    for page in 0..page_count {
        let tree = core.build_page_render_tree(page).expect("render tree");
        let mut tables = Vec::new();
        collect(&tree.root, &mut tables);
        if host413.is_none() {
            if let Some(&(_, _, x, y, w, h)) = tables
                .iter()
                .find(|(sec, pi, ..)| *sec == Some(2) && *pi == Some(413))
            {
                let nested: Vec<(f64, f64, f64, f64)> = tables
                    .iter()
                    .filter(|(_, pi, nx, ny, ..)| {
                        *pi != Some(413) && *nx >= x - 0.5 && *ny >= y - 0.5
                    })
                    .map(|&(_, _, nx, ny, nw, nh)| (nx, ny, nw, nh))
                    .collect();
                host413 = Some((page, (x, y, w, h), nested));
            }
        }
        if t417.is_none() {
            if let Some(&(_, _, x, y, w, h)) = tables
                .iter()
                .find(|(sec, pi, ..)| *sec == Some(2) && *pi == Some(417))
            {
                t417 = Some((page, (x, y, w, h)));
            }
        }
        if host413.is_some() && t417.is_some() {
            break;
        }
    }
    let (p413, host, nested) = host413.expect("s2#413 표를 렌더 트리에서 찾음");
    let (p417, t417) = t417.expect("s2#417 표를 렌더 트리에서 찾음");

    // host 표 높이 = 저장 seg lh 재현.
    let expect_h = 55136.0 / HU_PER_PX;
    assert!(
        (host.3 - expect_h).abs() <= 1.5,
        "s2#413 표 높이 {:.1} ≈ 저장 lh {expect_h:.1}px",
        host.3
    );
    // 인라인 소형 표(lh=1848HU≈24.6px)가 host bbox 안에 저장 조판대로.
    let small_h = 1848.0 / HU_PER_PX;
    let inline_small = nested.iter().find(|(nx, ny, nw, nh)| {
        (nh - small_h).abs() <= 1.0
            && *nx + *nw <= host.0 + host.2 + 0.5
            && *ny + *nh <= host.1 + host.3 + 0.5
    });
    assert!(
        inline_small.is_some(),
        "셀 안 인라인 소형 표(h≈{small_h:.1}px)가 host bbox 안에 있어야 한다: {nested:?}"
    );

    // s2#417 은 다음 쪽, top = 본문 top + 저장 vpos(11026HU).
    assert_eq!(p417, p413 + 1, "s2#417 은 s2#413 다음 쪽");
    let expect_dy = 11026.0 / HU_PER_PX;
    let dy = t417.1 - host.1;
    assert!(
        (dy - expect_dy).abs() <= 1.5,
        "s2#417 표 top 은 본문 top + 저장 vpos: 기대 {expect_dy:.1}px 실측 {dy:.2}px"
    );
}
