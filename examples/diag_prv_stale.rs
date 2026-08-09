//! [parity] PrvImage 신뢰성 — PrvText(한글이 저장한 본문 미리보기) 와 실제 본문을 대조하면
//! 미리보기가 낡았는지(우리 도구가 재저장했는지) 알 수 있다.
use rhwp::wasm_api::HwpDocument;
fn main() {
    for f in std::env::args().skip(1) {
        let bytes = std::fs::read(&f).unwrap();
        let doc = HwpDocument::from_bytes(&bytes).unwrap();
        let n = doc.get_paragraph_count(0).unwrap_or(0);
        let mut body = String::new();
        for pi in 0..n.min(6) {
            let len = doc.get_paragraph_length(0, pi).unwrap_or(0);
            if let Ok(t) = doc.get_text_range(0, pi, 0, len) {
                body.push_str(t.trim());
            }
        }
        println!(
            "{:44} paras={n} body={:?}",
            f.rsplit('/').next().unwrap(),
            body.chars().take(28).collect::<String>()
        );
    }
}
