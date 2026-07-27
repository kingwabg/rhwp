//! [parity] 줄 폭 대조 — 한컴이 파일에 적어둔 LINE_SEG 와 우리 줄 상자가 같은가.
//!
//! 왜 픽셀 대조가 아니라 이것인가: 어울림 옆흐름 결함(그림 옆 줄이 단 전폭으로 조판됨)은
//! 그림이 글자 위에 그려져 **화면 대조로 안 잡힌다**(2026-07-27 실측 — 띠·가로 지표 모두 무력).
//! 반면 한컴은 저장 시 각 줄의 segment_width 를 남기므로, 그 값과 우리 줄 상자 폭을 직접
//! 비교하면 정확히 잡힌다. 대상은 **한컴이 쓴 그대로의 파일**뿐(우리가 재저장한 건 무의미).
//!
//! 사용: cargo run --release --example qa_lineseg_parity [파일...]  (없으면 samples/*.hwp)
use rhwp::renderer::render_tree::{RenderNode, RenderNodeType};
use rhwp::wasm_api::HwpDocument;

fn px(hu: i32) -> f64 {
    hu as f64 / 7200.0 * 96.0
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let files: Vec<String> = if args.is_empty() {
        let mut v: Vec<String> = std::fs::read_dir("samples")
            .unwrap()
            .filter_map(|e| {
                let p = e.ok()?.path();
                (p.extension()? == "hwp").then(|| p.to_string_lossy().into_owned())
            })
            .collect();
        v.sort();
        v
    } else {
        args
    };

    let (mut tot_ok, mut tot_all, mut bad_files) = (0usize, 0usize, 0usize);
    for f in &files {
        let Ok(bytes) = std::fs::read(f) else { continue };
        let Ok(doc) = HwpDocument::from_bytes(&bytes) else { continue };
        // 기대 폭: 문단별 line_segs (파서 모델). 실제: 렌더트리 TextLine bbox.
        let Ok(parsed) = rhwp::parser::parse_hwp(&bytes) else { continue };
        let Some(sec) = parsed.sections.first() else { continue };
        let mut expected: Vec<f64> = Vec::new();
        for p in &sec.paragraphs {
            for s in &p.line_segs {
                if s.segment_width > 0 {
                    expected.push(px(s.segment_width as i32));
                }
            }
        }
        let mut actual: Vec<f64> = Vec::new();
        for pg in 0..doc.page_count() {
            let Ok(tree) = doc.build_page_render_tree(pg) else { continue };
            collect(&tree.root, &mut actual);
        }
        // 순서 대응이 보장되진 않으므로 **분포**로 비교한다: 기대 폭 각각에 대해
        // 오차 2px 안의 실제 줄이 있는가(다중집합 매칭).
        let mut used = vec![false; actual.len()];
        let mut ok = 0usize;
        for e in &expected {
            if let Some(i) = actual
                .iter()
                .enumerate()
                .filter(|(i, _)| !used[*i])
                .find(|(_, a)| (**a - *e).abs() <= 2.0)
                .map(|(i, _)| i)
            {
                used[i] = true;
                ok += 1;
            }
        }
        let all = expected.len();
        if all == 0 {
            continue;
        }
        tot_ok += ok;
        tot_all += all;
        let pct = ok * 100 / all;
        if pct < 95 {
            bad_files += 1;
            println!("{pct:3}%  {ok}/{all}  {}", f.rsplit('/').next().unwrap());
        }
    }
    println!(
        "\n줄 폭 정합 {}% ({}/{}) · 95% 미만 파일 {}건 / 전체 {}건",
        if tot_all > 0 { tot_ok * 100 / tot_all } else { 0 },
        tot_ok,
        tot_all,
        bad_files,
        files.len()
    );
}

fn collect(node: &RenderNode, out: &mut Vec<f64>) {
    if matches!(node.node_type, RenderNodeType::TextLine(_)) {
        out.push(node.bbox.width);
    }
    for c in &node.children {
        collect(c, out);
    }
}
