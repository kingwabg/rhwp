//! [parity] 제품 축 기준 오라클 픽스처 생성기.
//!
//! 산출: parity/fixtures/F##_*.hwp — 한컴(한컴독스/한글)에서 열어 정답지(PDF/스크린샷)를
//! 뽑는 대상. 축 선정 근거 = sc- 제품 코드 역산(2026-07-27): 일지·공문 표는 전부
//! 글자처럼취급(tac), 서식 축 = fontSize·bold·textColor·align·colWidths·병합.
//! 규칙: 픽스처는 "축 하나당 파일 하나, 최소 내용" — 오라클 대조가 눈으로도 되게.
use rhwp::wasm_api::HwpDocument;

fn save(doc: &mut HwpDocument, name: &str) {
    let pages = doc.page_count();
    let bytes = doc.export_hwp().expect("save");
    let path = format!("parity/fixtures/{name}.hwp");
    std::fs::write(&path, &bytes).expect("write");
    println!("{path} ({} bytes, {pages}p)", bytes.len());
}

fn base() -> HwpDocument {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().expect("blank");
    doc
}

fn main() {
    // F01: tac 표 기본형 — 본문 사이 3×3 글자처럼취급 표 (제품 기본형)
    {
        let mut doc = base();
        doc.insert_text(
            0,
            0,
            0,
            "앞 본문입니다. 표는 글자처럼취급으로 이 뒤에 붙습니다.",
        )
        .unwrap();
        let len = doc.get_paragraph_length(0, 0).unwrap();
        let c: serde_json::Value = serde_json::from_str(&doc.create_table_ex(&format!(
            r#"{{"sectionIdx":0,"paraIdx":0,"charOffset":{len},"rowCount":3,"colCount":3,"treatAsChar":true}}"#
        )).unwrap()).unwrap();
        let (pi, ci) = (
            c["paraIdx"].as_u64().unwrap() as u32,
            c["controlIdx"].as_u64().unwrap() as u32,
        );
        for r in 0..3u32 {
            for col in 0..3u32 {
                doc.insert_text_in_cell(
                    0,
                    pi,
                    ci,
                    r * 3 + col,
                    0,
                    0,
                    &format!("{}{}", ["가", "나", "다"][r as usize], col + 1),
                )
                .unwrap();
            }
        }
        save(&mut doc, "F01_tac_table_basic");
    }
    // F02: tac 표 쪽 경계 — 본문으로 밀어 표가 쪽 끝에 걸치게 (원자성 축 §3.5)
    {
        let mut doc = base();
        // 명시적 문단 44개 — 자동 줄바꿈(폰트 메트릭 의존)에 기대지 않아 오라클 대조가 견고하다.
        // ⚠ insert_paragraph 는 지정 인덱스 "앞"에 빈 문단을 넣는다 — 위에서 아래로 역순 삽입.
        doc.insert_text(0, 0, 0, "채움 44").unwrap();
        for n in (1..44u32).rev() {
            doc.insert_paragraph(0, 0).unwrap();
            doc.insert_text(0, 0, 0, &format!("채움 {n}")).unwrap();
        }
        let last_len = doc.get_paragraph_length(0, 43).unwrap();
        let c: serde_json::Value = serde_json::from_str(&doc.create_table_ex(&format!(
            r#"{{"sectionIdx":0,"paraIdx":43,"charOffset":{last_len},"rowCount":6,"colCount":2,"treatAsChar":true}}"#
        )).unwrap()).unwrap();
        let (pi, ci) = (
            c["paraIdx"].as_u64().unwrap() as u32,
            c["controlIdx"].as_u64().unwrap() as u32,
        );
        for r in 0..6u32 {
            for col in 0..2u32 {
                doc.insert_text_in_cell(
                    0,
                    pi,
                    ci,
                    r * 2 + col,
                    0,
                    0,
                    &format!("행{}열{}", r + 1, col + 1),
                )
                .unwrap();
            }
        }
        // page_count 는 lazy — bbox 조회로 페이지네이션을 강제한 뒤 저장한다
        let _ = doc.get_table_cell_bboxes(0, pi, ci, None);
        save(&mut doc, "F02_tac_table_pagebreak");
    }
    // F03: 셀 서식 — fontSize·bold·textColor·정렬 (공문 축)
    {
        let mut doc = base();
        let c: serde_json::Value = serde_json::from_str(&doc.create_table_ex(
            r#"{"sectionIdx":0,"paraIdx":0,"charOffset":0,"rowCount":2,"colCount":2,"treatAsChar":true}"#
        ).unwrap()).unwrap();
        let (pi, ci) = (
            c["paraIdx"].as_u64().unwrap() as u32,
            c["controlIdx"].as_u64().unwrap() as u32,
        );
        doc.insert_text_in_cell(0, pi, ci, 0, 0, 0, "크게 20pt")
            .unwrap();
        doc.apply_char_format_in_cell(
            0,
            pi as usize,
            ci as usize,
            0,
            0,
            0,
            999,
            r#"{"fontSize":20}"#,
        )
        .unwrap();
        doc.insert_text_in_cell(0, pi, ci, 1, 0, 0, "굵게").unwrap();
        doc.apply_char_format_in_cell(
            0,
            pi as usize,
            ci as usize,
            1,
            0,
            0,
            999,
            r#"{"bold":true}"#,
        )
        .unwrap();
        doc.insert_text_in_cell(0, pi, ci, 2, 0, 0, "빨강").unwrap();
        doc.apply_char_format_in_cell(
            0,
            pi as usize,
            ci as usize,
            2,
            0,
            0,
            999,
            r##"{"textColor":"#FF0000"}"##,
        )
        .unwrap();
        doc.insert_text_in_cell(0, pi, ci, 3, 0, 0, "가운데")
            .unwrap();
        doc.apply_para_format_in_cell(
            0,
            pi as usize,
            ci as usize,
            3,
            0,
            r#"{"alignment":"center"}"#,
        )
        .unwrap();
        save(&mut doc, "F03_cell_format");
    }
    // F04: 열폭 + 병합 (일지 축) — 1행 전체 병합 헤더 + 2:1 열폭
    {
        let mut doc = base();
        let c: serde_json::Value = serde_json::from_str(&doc.create_table_ex(
            r#"{"sectionIdx":0,"paraIdx":0,"charOffset":0,"rowCount":3,"colCount":2,"treatAsChar":true,"colWidths":[28000,14000]}"#
        ).unwrap()).unwrap();
        let (pi, ci) = (
            c["paraIdx"].as_u64().unwrap() as u32,
            c["controlIdx"].as_u64().unwrap() as u32,
        );
        doc.merge_table_cells(0, pi, ci, 0, 0, 0, 1).unwrap();
        doc.insert_text_in_cell(0, pi, ci, 0, 0, 0, "병합 헤더")
            .unwrap();
        doc.insert_text_in_cell(0, pi, ci, 2, 0, 0, "넓은 열(2/3)")
            .unwrap();
        doc.insert_text_in_cell(0, pi, ci, 3, 0, 0, "좁은 열(1/3)")
            .unwrap();
        save(&mut doc, "F04_col_widths_merge");
    }
    // F05: 누름틀 필드 (일지 치환 축)
    {
        let mut doc = base();
        doc.insert_text(0, 0, 0, "이름: ").unwrap();
        let len = doc.get_paragraph_length(0, 0).unwrap();
        doc.insert_click_here_field_api(0, 0, len, "이름을 입력하세요", "", "name", true)
            .unwrap();
        doc.set_field_value_by_name("name", "홍길동").unwrap();
        save(&mut doc, "F05_field_clickhere");
    }
}
