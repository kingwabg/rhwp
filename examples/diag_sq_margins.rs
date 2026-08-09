//! Square 그림들의 오른쪽 여백(margin.right) 분포 — 등록 시 뺄 값의 규약 확인용.
use rhwp::model::control::Control;
use rhwp::model::shape::{ShapeObject, TextWrap};
fn main() {
    for f in std::env::args().skip(1) {
        let bytes = std::fs::read(&f).unwrap();
        let doc = rhwp::parser::parse_hwp(&bytes).unwrap();
        let mut mrs: Vec<(usize, usize, i32, i32)> = Vec::new();
        for (si, sec) in doc.sections.iter().enumerate() {
            for (pi, p) in sec.paragraphs.iter().enumerate() {
                for c in &p.controls {
                    let cm = match c {
                        Control::Picture(p) => Some(&p.common),
                        Control::Shape(s) => match s.as_ref() {
                            ShapeObject::Picture(p) => Some(&p.common),
                            _ => None,
                        },
                        _ => None,
                    };
                    if let Some(cm) = cm {
                        if !cm.treat_as_char && matches!(cm.text_wrap, TextWrap::Square) {
                            mrs.push((si, pi, cm.margin.left as i32, cm.width as i32));
                        }
                    }
                }
            }
        }
        println!(
            "{}: Square 그림 {}개",
            f.rsplit('/').next().unwrap(),
            mrs.len()
        );
        for (si, pi, mr, w) in mrs.iter().take(8) {
            println!(
                "   sec{si} para{pi} margin_left={mr}HU({:.2}px) w={w}",
                *mr as f64 / 7200.0 * 96.0
            );
        }
    }
}
