//! [parity] 어울림 옆흐름의 원천 확인 — 파일에 저장된 LINE_SEG(줄 폭)를 그대로 본다.
//! 한컴은 저장 시 각 줄의 column_start/segment_width 를 기록한다. 그림 옆에서 좁아진
//! 줄이면 segment_width 가 단 폭보다 작아야 한다.
use rhwp::parser::parse_hwp;
fn main() {
    let path = std::env::args().nth(1).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let doc = parse_hwp(&bytes).unwrap();
    let si: usize = std::env::var("SEC").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
    let sec = &doc.sections[si];
    for (pi, p) in sec.paragraphs.iter().enumerate().skip(std::env::var("SKIP").ok().and_then(|v| v.parse().ok()).unwrap_or(0)).take(6) {
        let segs: Vec<String> = p.line_segs.iter().take(8)
            .map(|s| format!("{}+{}", s.column_start, s.segment_width)).collect();
        let text: String = p.text.chars().take(12).collect();
        let ctrls: Vec<String> = p.controls.iter().map(|c| match c {
            rhwp::model::control::Control::Picture(pic) => format!("Picture(wrap={:?},mr={},voff={})", pic.common.text_wrap, pic.common.margin.right, pic.common.vertical_offset),
            rhwp::model::control::Control::Shape(s) => format!("Shape({})", match s.as_ref() {
                rhwp::model::shape::ShapeObject::Picture(pic) => format!("Picture wrap={:?} mr={}", pic.common.text_wrap, pic.common.margin.right),
                other => format!("{:?}", std::mem::discriminant(other)),
            }),
            rhwp::model::control::Control::Table(t) => format!("Table(wrap={:?})", t.common.text_wrap),
            other => format!("{:?}", std::mem::discriminant(other)),
        }).collect();
        { println!("para{pi} {:?} 줄{} [{}] {}", text, p.line_segs.len(), segs.join(" "), ctrls.join(" ")); }
    }
}
