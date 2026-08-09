//! 표 앞뒤 세로 이동 대칭성 — 한컴 정합 (사용자 지시 2026-08-02).
//!
//! 문서: [문단1] [문단2] [3x3 표] [문단4]
//! 문단1에서 ↓ 를 계속 눌러 문단4까지 간 경로와, 문단4에서 ↑ 를 계속 눌러 문단1까지
//! 온 경로가 **서로 뒤집은 모양**이어야 한다. 어긋나면 표를 건너뛰거나 엉뚱한 칸에
//! 들어가는 것이다.
use rhwp::wasm_api::HwpDocument;

/// 커서 자리를 사람이 읽는 한 줄로. 본문은 `p{n}`, 셀 안은 `cell(r,c)`.
fn where_am_i(v: &serde_json::Value) -> String {
    let para = v["paragraphIndex"].as_u64().unwrap_or(u64::MAX);
    match v["cellIndex"].as_i64() {
        Some(c) if c >= 0 => format!("cell{}", c),
        _ => format!("p{}", para),
    }
}

struct Nav {
    doc: HwpDocument,
    sec: usize,
    para: usize,
    off: usize,
    cell: Option<(usize, usize, usize, usize)>, // (parent_para, control, cell, cell_para)
    px: f64,
}

impl Nav {
    fn step(&mut self, delta: i32) -> String {
        let (pp, ci, cel, cp) = match self.cell {
            Some(t) => (t.0 as u32, t.1 as u32, t.2 as u32, t.3 as u32),
            None => (u32::MAX, 0, 0, 0),
        };
        let json = self
            .doc
            .move_vertical(
                self.sec as u32,
                self.para as u32,
                self.off as u32,
                delta,
                self.px,
                pp,
                ci,
                cel,
                cp,
            )
            .expect("move_vertical");
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        if v.get("sectionIndex").is_none() {
            return "(끝)".to_string();
        }
        self.para = v["paragraphIndex"].as_u64().unwrap_or(self.para as u64) as usize;
        self.off = v["charOffset"].as_u64().unwrap_or(0) as usize;
        let cidx = v["cellIndex"].as_i64().unwrap_or(-1);
        if cidx >= 0 {
            self.cell = Some((
                v["parentParaIndex"].as_u64().unwrap_or(0) as usize,
                v["controlIndex"].as_u64().unwrap_or(0) as usize,
                cidx as usize,
                v["cellParaIndex"].as_u64().unwrap_or(0) as usize,
            ));
        } else {
            self.cell = None;
        }
        where_am_i(&v)
    }
}

fn build() -> (HwpDocument, usize) {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    // 문단1 · 문단2
    doc.insert_text(0, 0, 0, "첫째 문단").unwrap();
    doc.split_paragraph(0, 0, 5).unwrap();
    doc.insert_text(0, 1, 0, "둘째 문단").unwrap();
    // 문단3 자리에 3x3 표
    doc.split_paragraph(0, 1, 5).unwrap();
    doc.create_table(0, 2, 0, 3, 3).expect("create_table");
    // 문단4
    doc.split_paragraph(0, 2, 0).unwrap();
    doc.insert_text(0, 3, 0, "넷째 문단").unwrap();
    let n = doc.get_paragraph_count(0).unwrap() as usize;
    (doc, n)
}

/// 캐럿 x 를 바꿔가며 ↓/↑ 가 **같은 열**로 드나드는지 — 한컴은 캐럿이 걸친 열로 들어간다.
#[test]
fn table_entry_column_follows_caret_x() {
    // 표 첫 행 각 열의 캐럿 x 를 먼저 재서, 그 x 로 들어가면 그 열이 나와야 한다.
    let (doc, _) = build();
    let mut seen = Vec::new();
    for px in [120.0_f64, 300.0, 480.0] {
        let mut down = Nav {
            doc: {
                let (d, _) = build();
                d
            },
            sec: 0,
            para: 1,
            off: 0,
            cell: None,
            px,
        };
        let entered_down = down.step(1); // 문단2 → 표
                                         // 같은 x 로 아래에서 위로 들어가기
        let (d2, n) = build();
        let mut up = Nav {
            doc: d2,
            sec: 0,
            para: n - 2,
            off: 0,
            cell: None,
            px,
        };
        let entered_up = up.step(-1); // 문단4 → 표
        seen.push((px, entered_down.clone(), entered_up.clone()));
    }
    eprintln!("x별 진입: {seen:?}");
    let _ = doc;
    // 열이 x 를 따라 달라져야 한다(전부 같은 칸이면 x 를 안 보는 것)
    let downs: Vec<&String> = seen.iter().map(|(_, d, _)| d).collect();
    assert!(
        downs.iter().collect::<std::collections::HashSet<_>>().len() > 1,
        "캐럿 x 를 바꿔도 늘 같은 칸으로 들어간다 — x 를 안 본다: {seen:?}"
    );
    // ↓ 로 들어간 열과 ↑ 로 들어간 열이 같은 열이어야 한다(행만 다르다)
    for (px, d, u) in &seen {
        let dcol = d.trim_start_matches("cell").parse::<usize>().unwrap_or(99) % 3;
        let ucol = u.trim_start_matches("cell").parse::<usize>().unwrap_or(98) % 3;
        assert_eq!(dcol, ucol, "x={px} 에서 ↓ 는 {d}, ↑ 는 {u} — 열이 다르다");
    }
}

#[test]
fn down_and_up_paths_are_mirror_images() {
    let (doc, paras) = build();
    eprintln!("문단 수 = {paras}");

    // ↓ 로 문단1 → 문단4
    let mut down = Nav {
        doc,
        sec: 0,
        para: 0,
        off: 0,
        cell: None,
        px: 120.0,
    };
    let mut down_path = vec!["p0".to_string()];
    for _ in 0..12 {
        let w = down.step(1);
        if w == "(끝)" {
            break;
        }
        if down_path.last() == Some(&w) {
            break; // 더 못 감
        }
        down_path.push(w);
    }
    eprintln!("↓ 경로: {down_path:?}");

    // ↑ 로 마지막 자리 → 위로
    let (doc2, _) = build();
    let last_para = paras - 1;
    let mut up = Nav {
        doc: doc2,
        sec: 0,
        para: last_para,
        off: 0,
        cell: None,
        px: 120.0,
    };
    let mut up_path = vec![format!("p{last_para}")];
    for _ in 0..12 {
        let w = up.step(-1);
        if w == "(끝)" {
            break;
        }
        if up_path.last() == Some(&w) {
            break;
        }
        up_path.push(w);
    }
    eprintln!("↑ 경로: {up_path:?}");

    let mut up_rev = up_path.clone();
    up_rev.reverse();
    assert_eq!(
        down_path, up_rev,
        "표를 사이에 둔 ↓/↑ 경로가 서로 뒤집은 모양이 아니다\n  ↓ {down_path:?}\n  ↑(뒤집음) {up_rev:?}"
    );
}
