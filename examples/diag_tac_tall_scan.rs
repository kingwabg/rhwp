//! [TAC 분할 한컴 대조 스크래치] 코퍼스에서 "본문 높이보다 큰 글자취급(TAC) 표"를 찾는다.
//! 저장 lineseg 구조(host lh vs 표 높이·vpos 사슬)가 한컴의 배치 결정을 증언한다.
//! 사용: cargo run --release --example diag_tac_tall_scan -- samples

use rhwp::model::control::Control;

fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| "samples".into());
    let mut hits = 0;
    for entry in std::fs::read_dir(&dir).expect("dir") {
        let path = entry.expect("entry").path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if !(name.ends_with(".hwp") || name.ends_with(".hwpx")) {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else { continue };
        let Ok(doc) = rhwp::parse_document(&bytes) else { continue };
        for (si, sec) in doc.sections.iter().enumerate() {
            // 본문 높이 = 용지 − 위아래 여백·머리말꼬리말 (대략)
            let pd = &sec.section_def.page_def;
            let body_h = pd.height as i64
                - pd.margin_top as i64
                - pd.margin_bottom as i64
                - pd.margin_header as i64
                - pd.margin_footer as i64;
            if body_h <= 0 {
                continue;
            }
            for (pi, para) in sec.paragraphs.iter().enumerate() {
                for ctrl in &para.controls {
                    let Control::Table(t) = ctrl else { continue };
                    if !t.common.treat_as_char {
                        continue;
                    }
                    let h = t.common.height as i64;
                    if h > body_h {
                        hits += 1;
                        let segs: Vec<(i32, i32)> = para
                            .line_segs
                            .iter()
                            .map(|s| (s.line_height, s.vertical_pos))
                            .collect();
                        println!(
                            "{name} sec{si} pi{pi} rows={} tbl_h={h} body_h={body_h} segs(lh,vpos)={:?}",
                            t.row_count,
                            &segs[..segs.len().min(6)]
                        );
                    }
                }
            }
        }
    }
    println!("hits={hits}");
}
