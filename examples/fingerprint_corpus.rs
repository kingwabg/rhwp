//! [기준 고정 2026-09-02] 말뭉치 렌더 좌표 지문.
//! samples/ 아래 .hwp/.hwpx 를 전부 열어 페이지별 render tree JSON 을 해시하고
//! `경로\t해시` 를 정렬해 찍는다. 리팩터 전/후 출력을 diff 하면 렌더가 바뀐 파일만 남는다.
//! 파싱·렌더 실패는 `FAIL:<사유>`, 패닉은 `PANIC` 으로 기록해 파일 목록이 흔들리지 않게 한다.
//! 사용: cargo run --release --example fingerprint_corpus [루트=samples] > /tmp/fp.tsv
//! 결정성: 같은 빌드 2회 실행이 100% 일치해야 한다(RHWP_RENDER_PATH 미설정).
use rhwp::wasm_api::HwpDocument;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
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

fn fingerprint(path: &Path) -> String {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => return format!("FAIL:read:{e}"),
    };
    let doc = match HwpDocument::from_bytes(&bytes) {
        Ok(d) => d,
        Err(e) => return format!("FAIL:parse:{}", format!("{e:?}").split_whitespace().next().unwrap_or("?")),
    };
    let mut h = DefaultHasher::new();
    let n = doc.page_count();
    n.hash(&mut h);
    for p in 0..n {
        match doc.get_page_render_tree(p) {
            Ok(json) => json.hash(&mut h),
            Err(_) => return format!("FAIL:render:p{p}"),
        }
    }
    format!("{:016x}\tpages={n}", h.finish())
}

fn main() {
    let root = std::env::args().nth(1).unwrap_or_else(|| "samples".into());
    let mut files = Vec::new();
    collect(Path::new(&root), &mut files);
    files.sort();
    // 렌더 경로 안의 eprintln 잡음을 결과와 분리한다.
    let quiet = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    for f in &files {
        let fp = std::panic::catch_unwind(|| fingerprint(f)).unwrap_or_else(|_| "PANIC".into());
        println!("{}\t{fp}", f.display());
    }
    std::panic::set_hook(quiet);
    eprintln!("{} files", files.len());
}
