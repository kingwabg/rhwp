//! [parity] 픽스처 재열람 스모크 — 우리 writer 출력이 최소한 우리 reader 로 유효한지.
use rhwp::wasm_api::HwpDocument;
fn main() {
    for e in std::fs::read_dir("parity/fixtures").unwrap() {
        let p = e.unwrap().path();
        if p.extension().map(|x| x != "hwp").unwrap_or(true) {
            continue;
        }
        let bytes = std::fs::read(&p).unwrap();
        match HwpDocument::from_bytes(&bytes) {
            Ok(doc) => println!(
                "{:40} pages={} paras={:?}",
                p.file_name().unwrap().to_string_lossy(),
                doc.page_count(),
                doc.get_paragraph_count(0)
            ),
            Err(e) => println!(
                "{:40} LOAD FAIL {e:?}",
                p.file_name().unwrap().to_string_lossy()
            ),
        }
    }
}
