//! [3단계 2026-09-02] HWPX 로드 표(raw_ctrl_data 비어 있음)의 편집 후 common 치수 갱신 확인.
//! 사용: cargo run --release --example diag_hwpx_dims <파일.hwpx> [섹션 문단 컨트롤 = 첫 표]
use rhwp::model::control::Control;
use rhwp::wasm_api::HwpDocument;

fn dims(doc: &HwpDocument, s: usize, p: usize, c: usize, tag: &str) {
    let Control::Table(t) = &doc.document().sections[s].paragraphs[p].controls[c] else { panic!() };
    let sw: u32 = t.get_column_widths().iter().sum();
    let sh: u32 = t.effective_row_heights().iter().sum();
    println!(
        "{tag}: common={}x{} Σcol_widths={sw} Σeff_rows={sh} raw_len={} cells={} inv={}",
        t.common.width, t.common.height, t.raw_ctrl_data.len(), t.cells.len(),
        t.check_invariants().err().unwrap_or_else(|| "ok".into())
    );
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let bytes = std::fs::read(&a[1]).expect("read");
    let mut doc = HwpDocument::from_bytes(&bytes).expect("parse");
    let (s, p, c) = if a.len() >= 5 {
        (a[2].parse().unwrap(), a[3].parse().unwrap(), a[4].parse().unwrap())
    } else {
        let mut found = None;
        'o: for (si, sec) in doc.document().sections.iter().enumerate() {
            for (pi, para) in sec.paragraphs.iter().enumerate() {
                for (ci, ctl) in para.controls.iter().enumerate() {
                    if matches!(ctl, Control::Table(_)) { found = Some((si, pi, ci)); break 'o; }
                }
            }
        }
        found.expect("no table")
    };
    dims(&doc, s, p, c, "load");
    // 첫 셀 오른쪽 경계를 +283HU(1mm) 어긋내기 → 표 폭 불변, common 은 Σ 로 재계산돼야 한다
    let r = doc.offset_cell_boundary_native(s, p, c, 0, true, 283);
    println!("offset → {:?}", r.err());
    dims(&doc, s, p, c, "after offset");
    let r = doc.restore_cell_boundary_native(s, p, c, 0, true);
    println!("restore → {:?}", r.err());
    dims(&doc, s, p, c, "after restore");
    // 열 폭을 각 +1000HU 로 늘려 Σ 를 바꾼다 → common 이 새 Σ 를 따라와야 한다(종전엔 raw 없으면 stale)
    let widths: Vec<u32> = {
        let Control::Table(t) = &doc.document().sections[s].paragraphs[p].controls[c] else { panic!() };
        t.get_column_widths().iter().map(|w| w + 1000).collect()
    };
    let r = doc.set_table_column_widths_native(s, p, c, widths);
    println!("set_column_widths(+1000 each) → {:?}", r.err());
    dims(&doc, s, p, c, "after widths");
}
