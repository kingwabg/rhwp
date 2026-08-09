//! [parity] 줄 폭 대조 — 한컴이 파일에 적어둔 LINE_SEG 와 우리 줄 상자가 같은가.
//!
//! 왜 픽셀 대조가 아니라 이것인가: 어울림 옆흐름 결함(그림 옆 줄이 단 전폭으로 조판됨)은
//! 그림이 글자 위에 그려져 **화면 대조로 안 잡힌다**(2026-07-27 실측 — 띠·가로 지표 모두 무력).
//! 반면 한컴은 저장 시 각 줄의 segment_width 를 남기므로, 그 값과 우리 줄 상자 폭을 직접
//! 비교하면 정확히 잡힌다. 대상은 **한컴이 쓴 그대로의 파일**뿐(우리가 재저장한 건 무의미).
//!
//! ⚠ **작은 차이는 신호가 아니다**(2026-07-27 교차검증): 한컴 PrvImage 와 픽셀로 100%
//! 일치하는 문서(143E433F503322BD33)도 이 방식으론 30% 로 나온다 — `segment_width` 는
//! 줄의 **가용 띠 폭**이지 그린 줄 상자 폭이 아니어서, 표·단·들여쓰기 경로에서 정상적으로
//! 어긋난다. 따라서 **큰 어긋남(기본 50px 이상)만** 결함 후보로 센다. pic2-2018 의 실제
//! 결함은 218px 차이였다(전폭 566.9 vs 저장 348.5) — 이 문턱 위에 뚜렷이 뜬다.
//!
//! 사용: cargo run --release --example qa_lineseg_parity [파일...]  (없으면 samples/*.hwp)
//!       BIG=50 문턱 조절(px)
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

    let big: f64 = std::env::var("BIG")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(50.0);
    let (mut tot_big, mut tot_all, mut bad_files) = (0usize, 0usize, 0usize);
    for f in &files {
        let Ok(bytes) = std::fs::read(f) else {
            continue;
        };
        let Ok(doc) = HwpDocument::from_bytes(&bytes) else {
            continue;
        };
        // 기대 폭: 문단별 line_segs (파서 모델). 실제: 렌더트리 TextLine bbox.
        let Ok(parsed) = rhwp::parser::parse_hwp(&bytes) else {
            continue;
        };
        let Some(sec) = parsed.sections.first() else {
            continue;
        };
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
            let Ok(tree) = doc.build_page_render_tree(pg) else {
                continue;
            };
            collect(&tree.root, &mut actual);
        }
        // 순서 대응이 보장되진 않으므로 **분포**로 비교한다: 기대 폭 각각에 대해
        // 오차 2px 안의 실제 줄이 있는가(다중집합 매칭).
        let mut used = vec![false; actual.len()];
        let mut ok = 0usize;
        let mut misses: Vec<f64> = Vec::new();
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
            } else {
                // 가장 가까운 미사용 실제 줄과의 차이 — 부호가 증상을 가른다
                let near = actual
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| !used[*i])
                    .min_by(|a, b| (a.1 - e).abs().partial_cmp(&(b.1 - e).abs()).unwrap());
                if let Some((i, a)) = near {
                    used[i] = true;
                    misses.push(a - e);
                } else {
                    misses.push(f64::NAN);
                }
            }
        }
        let all = expected.len();
        if all == 0 {
            continue;
        }
        let _ = ok;
        let bigs: Vec<f64> = misses
            .iter()
            .filter(|d| !d.is_nan() && d.abs() >= big)
            .copied()
            .collect();
        tot_big += bigs.len();
        tot_all += all;
        let pct = 100 - (bigs.len() * 100 / all);
        if !bigs.is_empty() {
            bad_files += 1;
            let missing = misses.iter().filter(|d| d.is_nan()).count();
            let wider = bigs.iter().filter(|d| **d > 0.0).count();
            let narrower = bigs.iter().filter(|d| **d < 0.0).count();
            let mut mags: Vec<f64> = bigs.iter().map(|d| d.abs()).collect();
            mags.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let med = mags.get(mags.len() / 2).copied().unwrap_or(0.0);
            println!(
                "{pct:3}%  큰어긋남 {}/{all}  넓음{wider} 좁음{narrower} 줄없음{missing} 중앙차{med:.1}px  {}",
                bigs.len(),
                f.rsplit('/').next().unwrap()
            );
        }
    }
    println!(
        "\n큰 어긋남(>={big:.0}px) {} 줄 / 전체 {} 줄 · 해당 파일 {}건 / {}건",
        tot_big,
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
