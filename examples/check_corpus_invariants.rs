//! [불변식 가드 2026-09-02] 말뭉치 표 불변식 검사.
//! samples/ 의 모든 문서를 열어 표(중첩·글상자·머리말·각주 포함)마다 `Table::check_invariants`
//! (구조, 위반이면 명령이 롤백된다)와 `lint_invariants`(경고)를 돌리고, 델타 불변식을
//! 절대 규칙으로 승격할 수 있는지 판단할 실물 통계(A1/A2, 행별 폭 차이, 조각 행)를 함께 찍는다.
//! 사용: cargo run --release --example check_corpus_invariants [루트=samples] > /tmp/corpus.txt
use rhwp::model::control::Control;
use rhwp::model::paragraph::Paragraph;
use rhwp::model::shape::ShapeObject;
use rhwp::model::table::Table;
use rhwp::wasm_api::HwpDocument;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect(&p, out);
        } else if matches!(
            p.extension().and_then(|s| s.to_str()).map(|s| s.to_ascii_lowercase()).as_deref(),
            Some("hwp") | Some("hwpx")
        ) {
            out.push(p);
        }
    }
}

fn walk_paragraphs<'a>(paras: &'a [Paragraph], path: &str, out: &mut Vec<(String, &'a Table)>) {
    for (pi, p) in paras.iter().enumerate() {
        for (ci, c) in p.controls.iter().enumerate() {
            let cp = format!("{path}/p{pi}/c{ci}");
            match c {
                Control::Table(t) => {
                    out.push((cp.clone(), t));
                    for (k, cell) in t.cells.iter().enumerate() {
                        walk_paragraphs(&cell.paragraphs, &format!("{cp}/cell{k}"), out);
                    }
                }
                Control::Shape(s) => walk_shape(s, &cp, out),
                Control::Header(h) => walk_paragraphs(&h.paragraphs, &format!("{cp}/header"), out),
                Control::Footer(f) => walk_paragraphs(&f.paragraphs, &format!("{cp}/footer"), out),
                Control::Footnote(f) => walk_paragraphs(&f.paragraphs, &format!("{cp}/footnote"), out),
                Control::Endnote(e) => walk_paragraphs(&e.paragraphs, &format!("{cp}/endnote"), out),
                Control::HiddenComment(h) => {
                    walk_paragraphs(&h.paragraphs, &format!("{cp}/hidden"), out)
                }
                _ => {}
            }
        }
    }
}

fn walk_shape<'a>(s: &'a ShapeObject, path: &str, out: &mut Vec<(String, &'a Table)>) {
    let tb = match s {
        ShapeObject::Line(x) => x.drawing.text_box.as_ref(),
        ShapeObject::Rectangle(x) => x.drawing.text_box.as_ref(),
        ShapeObject::Ellipse(x) => x.drawing.text_box.as_ref(),
        ShapeObject::Arc(x) => x.drawing.text_box.as_ref(),
        ShapeObject::Polygon(x) => x.drawing.text_box.as_ref(),
        ShapeObject::Curve(x) => x.drawing.text_box.as_ref(),
        ShapeObject::Chart(x) => x.drawing.text_box.as_ref(),
        ShapeObject::Ole(x) => x.drawing.text_box.as_ref(),
        ShapeObject::Group(g) => {
            for (i, ch) in g.children.iter().enumerate() {
                walk_shape(ch, &format!("{path}/g{i}"), out);
            }
            None
        }
        _ => None,
    };
    if let Some(tb) = tb {
        walk_paragraphs(&tb.paragraphs, &format!("{path}/textbox"), out);
    }
}

fn classify(reason: &str) -> &'static str {
    if reason.contains("row_sizes") {
        "S2"
    } else if reason.contains("순서") {
        "S3"
    } else if reason.contains("저장 치수") {
        "S4"
    } else if reason.contains("크기 이상") || reason.contains("셀 0개") {
        "S5"
    } else if reason.contains("표시 힌트") {
        "D6"
    } else {
        "S1"
    }
}

#[derive(Default)]
struct Stats {
    files: usize,
    parse_fail: usize,
    panics: usize,
    tables: usize,
    nested: usize,
    by_id: BTreeMap<&'static str, usize>,
    lint: BTreeMap<String, usize>,
    a1w_mismatch: usize,
    a1w_spacing_tables: usize,
    a1w_spacing_formula_ok: usize,
    a1h_mismatch: usize,
    a2_mismatch: usize,
    rowx_diff: usize,
    piece_rows: usize,
    fallback_tables: usize,
}

fn table_metrics(t: &Table, st: &mut Stats) {
    let cw = t.get_column_widths();
    let total_w: u64 = cw.iter().map(|&w| w as u64).sum();
    let cs = t.cell_spacing as i64;
    // A1 폭: 행별 앵커 셀 폭 합 vs common.width
    let mut any_row_mismatch = false;
    for r in 0..t.row_count {
        let sum: u64 = t.cells.iter().filter(|c| c.row == r).map(|c| c.width as u64).sum();
        if sum.abs_diff(t.common.width as u64) > 4 {
            any_row_mismatch = true;
        }
    }
    if any_row_mismatch {
        st.a1w_mismatch += 1;
    }
    if cs != 0 {
        st.a1w_spacing_tables += 1;
        let formula = total_w as i64 + (t.col_count as i64 + 1) * cs;
        if (formula - t.common.width as i64).abs() <= 4 {
            st.a1w_spacing_formula_ok += 1;
        }
    }
    // A1 높이: Σ실효 vs common.height
    let eff: u64 = t.effective_row_heights().iter().map(|&h| h as u64).sum();
    if eff.abs_diff(t.common.height as u64) > 4 {
        st.a1h_mismatch += 1;
    }
    // A2: 병합 셀 폭 == 걸친 열 합
    let rh = t.get_row_heights();
    let a2 = t.cells.iter().any(|c| {
        let (c0, c1) = (c.col as usize, (c.col + c.col_span) as usize);
        let (r0, r1) = (c.row as usize, (c.row + c.row_span) as usize);
        (c.col_span > 1
            && c1 <= cw.len()
            && (cw[c0..c1].iter().map(|&w| w as u64).sum::<u64>()).abs_diff(c.width as u64) > 4)
            || (c.row_span > 1
                && r1 <= rh.len()
                && (rh[r0..r1].iter().map(|&h| h as u64).sum::<u64>()).abs_diff(c.height as u64) > 4)
    });
    if a2 {
        st.a2_mismatch += 1;
    }
    // 행별 x선 차이: span1 셀 폭이 열 max 와 다른 행이 있는가
    if t.cells.iter().any(|c| {
        c.col_span == 1 && (c.col as usize) < cw.len() && c.width != cw[c.col as usize]
    }) {
        st.rowx_diff += 1;
    }
    // 조각 행 후보: row_span==1 셀이 하나도 없는 행
    if (0..t.row_count).any(|r| !t.cells.iter().any(|c| c.row == r && c.row_span == 1)) {
        st.piece_rows += 1;
    }
}

fn main() {
    let root = std::env::args().nth(1).unwrap_or_else(|| "samples".into());
    let mut files = Vec::new();
    collect(Path::new(&root), &mut files);
    files.sort();
    let mut st = Stats::default();
    let mut violations: Vec<String> = Vec::new();
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    for f in &files {
        st.files += 1;
        let Ok(bytes) = std::fs::read(f) else { st.parse_fail += 1; continue };
        let doc = match std::panic::catch_unwind(|| HwpDocument::from_bytes(&bytes)) {
            Ok(Ok(d)) => d,
            Ok(Err(_)) => { st.parse_fail += 1; continue }
            Err(_) => { st.panics += 1; continue }
        };
        let document = doc.document();
        let mut tables = Vec::new();
        for (si, sec) in document.sections.iter().enumerate() {
            walk_paragraphs(&sec.paragraphs, &format!("s{si}"), &mut tables);
        }
        for (path, t) in tables {
            st.tables += 1;
            if path.contains("/cell") || path.contains("/textbox") {
                st.nested += 1;
            }
            if let Err(why) = t.check_invariants() {
                let id = classify(&why);
                *st.by_id.entry(id).or_default() += 1;
                violations.push(format!("V\t{id}\t{}\t{path}\t{why}", f.display()));
            }
            let lints = t.lint_invariants();
            let mut had_fallback = false;
            for l in &lints {
                let key = l.split_whitespace().next().unwrap_or("?").to_string();
                if key == "S7" {
                    had_fallback = true;
                }
                *st.lint.entry(key).or_default() += 1;
            }
            if had_fallback {
                st.fallback_tables += 1;
            }
            table_metrics(t, &mut st);
        }
    }
    std::panic::set_hook(hook);
    println!(
        "files={} parse_fail={} panics={} tables={} nested={}",
        st.files, st.parse_fail, st.panics, st.tables, st.nested
    );
    println!("STRUCT violations by id: {:?}", st.by_id);
    println!("LINT counts (lines): {:?}  tables_with_S7_fallback={}", st.lint, st.fallback_tables);
    println!(
        "A1 width: rows-sum≠common {} tables; cell_spacing≠0 {} tables, formula Σ+(n+1)cs ok {}",
        st.a1w_mismatch, st.a1w_spacing_tables, st.a1w_spacing_formula_ok
    );
    println!("A1 height: Σeff≠common {} tables", st.a1h_mismatch);
    println!("A2 merged≠span-sum: {} tables", st.a2_mismatch);
    println!("row-x differs from column max: {} tables", st.rowx_diff);
    println!("span-only (piece-row candidate) rows: {} tables", st.piece_rows);
    violations.sort();
    for v in violations {
        println!("{v}");
    }
}
