//! [oracle-pdf-mining-20260806 §2-B/§2-C] 글자취급 개체의 세로 배치 = **글리프 상자가
//! 기준선을 r:(1−r) 로 가름** (r = 그 줄의 baseline_distance / line_height).
//!
//! ```text
//! 잉크상단 = 줄상단 + bd − r·(표높이 + 바깥여백 상하) + 바깥여백상
//! ```
//!
//! 오라클은 한컴 인쇄 PDF 벡터 실측이다:
//! - `samples/21_언어_기출_편집가능본.pdf` 1쪽 — 한 줄(lh=3015 bd=1507, r=0.4998)에 있는
//!   바깥여백 566 짜리 성명 표(h=2449)와 바깥여백 0 인 수험번호 표(h=2448)의 **잉크 y 가
//!   완전히 동일**(둘 다 966.48…990.96pt). "잉크상단 = 줄상단+여백상"·"잉크바닥 =
//!   줄바닥−여백하" 모델은 둘을 2.83pt 어긋나게 하므로 기각되고, 글리프 상자 가름만
//!   남는다.
//! - `samples/복학원서.pdf` — 표 잉크바닥이 같은 줄 뒤 텍스트 기준선보다 30.57pt **아래**
//!   (예측 30.54pt, Δ0.03pt). 종전 식이 인코딩한 법칙 4("잉크바닥 = 기준선 + 여백하")의
//!   직접 반증.
//!
//! 종전 `paragraph_layout` 3곳은 `줄상단 + bd + 여백하 − 표높이` 를 썼고, 실측 표본에서는
//! 전부 `.max(줄상단)` 클램프로 접혀 오차(1.40~2.83pt)가 가려져 있었다.
use std::fs;
use std::path::Path;

use rhwp::renderer::render_tree::{RenderNode, RenderNodeType};

/// HWPUNIT → px (96dpi, 75HU = 1px).
fn px(hu: f64) -> f64 {
    hu / 75.0
}

/// 오라클 허용 오차 = ±0.5pt.
const TOL: f64 = 0.5 * 96.0 / 72.0;

fn render(rel: &str, page: u32) -> RenderNode {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    let bytes = fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let mut doc = rhwp::wasm_api::HwpDocument::from_bytes(&bytes)
        .unwrap_or_else(|e| panic!("parse {rel}: {e:?}"));
    let tree = doc
        .build_page_render_tree(page)
        .unwrap_or_else(|e| panic!("render {rel} p{page}: {e:?}"));
    tree.root
}

/// (표 bbox, 셀 bbox, 줄 노드, 텍스트 런) 을 술어로 골라 모으는 단일 순회.
fn collect<T>(
    node: &RenderNode,
    pick: &mut impl FnMut(&RenderNode) -> Option<T>,
    out: &mut Vec<T>,
) {
    if let Some(v) = pick(node) {
        out.push(v);
    }
    for c in &node.children {
        collect(c, pick, out);
    }
}

/// `21_언어_기출_편집가능본.hwp` s0#0/cell6#0 — 셀 안 인라인 TAC 표 2개(성명·수험번호).
///
/// 셀 padding 상 141HU, 저장 seg lh=3015 bd=1507(문단 세로정렬=가운데 → r=0.4998).
/// 성명 표 글리프높이 = 2449+566 = 3015 = lh → 잉크상단 = 줄상단 + 283HU.
/// 수험번호 표는 여백이 없지만 글리프높이 2448 이 r 만큼만 기준선을 가르므로
/// 잉크상단 = 줄상단 + 1507 − 0.4998×2448 = 줄상단 + 283.4HU — **같은 y**.
///
/// 수정 전: 둘 다 `(줄상단 + bd + 여백하 − 표높이)` 가 음수라 줄상단으로 클램프
/// (실측 셀상단+1.87px) → 오라클보다 3.77px(2.83pt) 위.
#[test]
fn eoneo_cell_tac_pair_ink_top_splits_baseline_by_glyph_box() {
    let root = render("samples/21_언어_기출_편집가능본.hwp", 0);
    // 성명(w=14174) · 수험번호(w=21384) 표와 그 호스트 셀(cell6).
    let mut tables = Vec::new();
    collect(
        &root,
        &mut |n: &RenderNode| match &n.node_type {
            RenderNodeType::Table(_) if n.bbox.width < px(22_000.0) => {
                Some((n.bbox.width, n.bbox.x, n.bbox.y))
            }
            _ => None,
        },
        &mut tables,
    );
    let find = |w_hu: f64| {
        tables
            .iter()
            .find(|(w, ..)| (*w - px(w_hu)).abs() <= 1.0)
            .unwrap_or_else(|| panic!("폭 {w_hu}HU 표를 찾지 못함: {tables:?}"))
    };
    let name = find(14_174.0);
    let exam_no = find(21_384.0);

    // 호스트 셀 = 두 표를 모두 품는 가장 작은 셀.
    let mut cells = Vec::new();
    collect(
        &root,
        &mut |n: &RenderNode| match &n.node_type {
            RenderNodeType::TableCell(_) => Some(n.bbox.clone()),
            _ => None,
        },
        &mut cells,
    );
    let host = cells
        .iter()
        .filter(|b| {
            b.x <= name.1 + 0.5
                && b.x + b.width >= exam_no.1 + 0.5
                && b.y <= name.2 + 0.5
                && b.y + b.height >= name.2 - 0.5
        })
        .min_by(|a, b| a.height.partial_cmp(&b.height).unwrap())
        .expect("성명·수험번호 표를 품는 셀");

    // 줄 상단 = 셀 상단 + 셀 padding 상(141HU). 오라클 잉크상단 = 줄상단 + 283HU.
    let expect = host.y + px(141.0) + px(283.0);
    for (label, t) in [("성명", name), ("수험번호", exam_no)] {
        assert!(
            (t.2 - expect).abs() <= TOL,
            "{label} 표 잉크상단 {:.3} 이 오라클 {expect:.3}(셀상단 {:.3} + 패딩 1.867 + \
             바깥여백상 3.773)과 {:.3}px 어긋났다",
            t.2,
            host.y,
            (t.2 - expect).abs()
        );
    }
    assert!(
        (name.2 - exam_no.2).abs() <= 0.1,
        "PDF 실측: 여백 566 표와 여백 0 표의 잉크 y 는 동일해야 한다 — 성명 {:.3} vs \
         수험번호 {:.3}",
        name.2,
        exam_no.2
    );
}

/// `samples/tac-case-001.hwp` s0#1 — 본문 인라인 TAC 표(앞뒤 텍스트 같은 줄).
/// 표 h=2568 + 바깥여백 566 = 글리프 3134 = 저장 lh, bd=2664(r=0.85)
/// → 잉크상단 = 줄상단 + 283HU(3.773px). 수정 전: 줄상단 + (2664+283−2568)=379HU
/// (5.053px) 로 1.28px 아래.
#[test]
fn taccase001_inline_table_ink_top_is_line_top_plus_outer_margin() {
    let root = render("samples/tac-case-001.hwp", 0);
    // 호스트 문단(pi=1)의 줄 상단 = 그 문단 텍스트 런의 최소 y (런 bbox y = 줄 상단).
    let mut runs = Vec::new();
    collect(
        &root,
        &mut |n: &RenderNode| match &n.node_type {
            RenderNodeType::TextRun(r) if r.para_index == Some(1) && !r.text.trim().is_empty() => {
                Some(n.bbox.y)
            }
            _ => None,
        },
        &mut runs,
    );
    let line_top = runs
        .iter()
        .copied()
        .fold(f64::INFINITY, f64::min)
        .min(f64::MAX);
    assert!(line_top.is_finite(), "pi=1 텍스트 런을 찾지 못함");

    let mut tables = Vec::new();
    collect(
        &root,
        &mut |n: &RenderNode| match &n.node_type {
            RenderNodeType::Table(t) if t.para_index == Some(1) => Some(n.bbox.y),
            _ => None,
        },
        &mut tables,
    );
    let table_y = *tables.first().expect("pi=1 TAC 표를 찾지 못함");
    let expect = line_top + px(283.0);
    assert!(
        (table_y - expect).abs() <= TOL,
        "인라인 TAC 표 잉크상단 {table_y:.3} 이 오라클 {expect:.3}(줄상단 {line_top:.3} + \
         바깥여백상 3.773)과 {:.3}px 어긋났다",
        (table_y - expect).abs()
    );
}

/// `samples/복학원서.hwp` s0#16 — 오라클 PDF 표본(표 h=21016, 바깥여백 각 변 140,
/// 저장 seg[1] lh=21296 bd=18102). 글리프높이 == lh → 잉크상단 = 줄상단 + 140HU.
///
/// 이 표는 블록 TAC 경로(`layout.rs` `is_tac` 표 아이템)로 그려져 이미 오라클과
/// 일치한다 — 인라인 3곳 정정이 이 경로를 흔들지 않는지 지키는 핀.
#[test]
fn bokhak_block_tac_table_ink_top_matches_oracle() {
    let root = render("samples/복학원서.hwp", 0);
    let mut lines = Vec::new();
    collect(
        &root,
        &mut |n: &RenderNode| match &n.node_type {
            RenderNodeType::TextLine(l) if l.para_index == Some(16) && l.line_index == Some(1) => {
                Some(n.bbox.y)
            }
            _ => None,
        },
        &mut lines,
    );
    let line_top = *lines.first().expect("s0#16 두 번째 줄 노드");
    let mut tables = Vec::new();
    collect(
        &root,
        &mut |n: &RenderNode| match &n.node_type {
            RenderNodeType::Table(t) if t.para_index == Some(16) => Some(n.bbox.y),
            _ => None,
        },
        &mut tables,
    );
    let table_y = *tables.first().expect("s0#16 표");
    let expect = line_top + px(140.0);
    assert!(
        (table_y - expect).abs() <= TOL,
        "복학원서 표 잉크상단 {table_y:.3} 이 오라클 {expect:.3}(줄상단 {line_top:.3} + \
         바깥여백상 1.867)과 {:.3}px 어긋났다",
        (table_y - expect).abs()
    );
}
