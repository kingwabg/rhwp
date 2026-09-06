//! [불변식 가드 2026-09-02] 표 셀 덤프 — check_corpus_invariants 가 가리킨 표를 들여다본다.
//! 사용: cargo run --release --example diag_table_dump <파일> <섹션> <문단> <컨트롤>
//! 최상위 문단의 컨트롤만 (중첩 표는 경로의 cell 인덱스를 따라가며 확장 예정).
use rhwp::model::control::Control;
use rhwp::wasm_api::HwpDocument;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let bytes = std::fs::read(&a[1]).expect("read");
    let doc = HwpDocument::from_bytes(&bytes).expect("parse");
    let (s, p, c): (usize, usize, usize) = (
        a[2].parse().unwrap(),
        a[3].parse().unwrap(),
        a[4].parse().unwrap(),
    );
    let para = &doc.document().sections[s].paragraphs[p];
    let Control::Table(t) = &para.controls[c] else {
        panic!("not a table")
    };
    println!(
        "rows={} cols={} cells={} common={}x{} row_sizes={:?}",
        t.row_count,
        t.col_count,
        t.cells.len(),
        t.common.width,
        t.common.height,
        t.row_sizes
    );
    println!("col_widths={:?}", t.get_column_widths());
    println!("row_heights={:?}", t.get_row_heights());
    for (i, cell) in t.cells.iter().enumerate() {
        let txt: String = cell
            .paragraphs
            .iter()
            .map(|p| p.text.as_str())
            .collect::<Vec<_>>()
            .join("|");
        println!(
            "#{i:3} ({:2},{:2}) span {}x{} size {}x{} {:?}",
            cell.row,
            cell.col,
            cell.row_span,
            cell.col_span,
            cell.width,
            cell.height,
            txt.chars().take(20).collect::<String>()
        );
    }
    if let Err(e) = t.check_invariants() {
        println!("INVARIANT: {e}");
    }
    for l in t.lint_invariants() {
        println!("LINT: {l}");
    }
}
