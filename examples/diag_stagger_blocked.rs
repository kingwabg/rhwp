//! [진단 2026-08-17] 표 어긋내기가 **거부되는 상황** 전수 실측.
//! 각 케이스마다 신선한 3×3 표를 만들어 조작을 시도하고, 엔진이 돌려준
//! 실제 메시지(또는 성공)를 JSON 으로 찍는다. 보고서(HTML)의 근거 데이터.
use rhwp::wasm_api::HwpDocument;

fn make() -> (HwpDocument, usize, usize) {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    let c: serde_json::Value = serde_json::from_str(
        &doc.create_table_ex(
            r#"{"sectionIdx":0,"paraIdx":0,"charOffset":0,"rowCount":3,"colCount":3,"treatAsChar":false}"#,
        )
        .unwrap(),
    )
    .unwrap();
    (
        doc,
        c["paraIdx"].as_u64().unwrap() as usize,
        c["controlIdx"].as_u64().unwrap() as usize,
    )
}

fn table_of(doc: &HwpDocument) -> &rhwp::model::table::Table {
    for para in &doc.document().sections[0].paragraphs {
        for ctrl in &para.controls {
            if let rhwp::model::control::Control::Table(t) = ctrl {
                return t;
            }
        }
    }
    panic!("표 없음");
}

/// [11-b 비교용] 셀 벡터 [(row,col,row_span,col_span,width,height,본문 앞 8자)] — 실행 결과를 케이스별로 diff.
fn cells_vec(t: &rhwp::model::table::Table) -> String {
    let v: Vec<serde_json::Value> = t
        .cells
        .iter()
        .map(|c| {
            let text: String = c
                .paragraphs
                .iter()
                .map(|p| p.text.as_str())
                .collect::<Vec<_>>()
                .join("|");
            serde_json::json!([
                c.row,
                c.col,
                c.row_span,
                c.col_span,
                c.width,
                c.height,
                text.chars().take(8).collect::<String>()
            ])
        })
        .collect();
    serde_json::Value::Array(v).to_string()
}

fn idx_at(doc: &HwpDocument, row: u16, col: u16) -> usize {
    for para in &doc.document().sections[0].paragraphs {
        for ctrl in &para.controls {
            if let rhwp::model::control::Control::Table(t) = ctrl {
                return t
                    .cells
                    .iter()
                    .position(|c| c.row == row && c.col == col)
                    .unwrap_or(usize::MAX);
            }
        }
    }
    usize::MAX
}

fn grid(doc: &HwpDocument) -> String {
    for para in &doc.document().sections[0].paragraphs {
        for ctrl in &para.controls {
            if let rhwp::model::control::Control::Table(t) = ctrl {
                return t
                    .cells
                    .iter()
                    .map(|c| format!("({},{})s{}x{}", c.row, c.col, c.row_span, c.col_span))
                    .collect::<Vec<_>>()
                    .join(" ");
            }
        }
    }
    String::new()
}

fn out(id: &str, label: &str, r: Result<String, rhwp::error::HwpError>, doc: &HwpDocument) {
    let msg = match r {
        Ok(_) => "OK".to_string(),
        Err(e) => format!("{e:?}")
            .replace("RenderError(\"", "")
            .replace("\")", "")
            .replace("InvalidArgument(\"", ""),
    };
    println!(
        "{{\"id\":\"{id}\",\"label\":\"{label}\",\"result\":\"{msg}\",\"grid\":\"{}\"}}",
        grid(doc)
    );
    println!("cellsVec {id} {}", cells_vec(table_of(doc)));
}

fn main() {
    // 1. 바깥 아래 테두리
    {
        let (mut d, pi, ci) = make();
        let i = idx_at(&d, 2, 0);
        let r = d.offset_cell_boundary_native(0, pi, ci, i, false, 283);
        out("outer-bottom", "맨 아래 행의 아래(바깥) 테두리", r, &d);
    }
    // 2. 바깥 오른쪽 테두리
    {
        let (mut d, pi, ci) = make();
        let i = idx_at(&d, 0, 2);
        let r = d.offset_cell_boundary_native(0, pi, ci, i, true, 283);
        out("outer-right", "맨 오른쪽 열의 오른쪽(바깥) 테두리", r, &d);
    }
    // 3. 세로 병합 이웃 — 우변
    {
        let (mut d, pi, ci) = make();
        d.merge_table_cells_native(0, pi, ci, 0, 1, 1, 1).unwrap();
        let i = idx_at(&d, 0, 0);
        let r = d.offset_cell_boundary_native(0, pi, ci, i, true, 283);
        out(
            "merged-vert-neighbor",
            "오른쪽 이웃이 세로 병합(높이가 다름)",
            r,
            &d,
        );
    }
    // 4. 가로 병합 이웃 — 하변
    {
        let (mut d, pi, ci) = make();
        d.merge_table_cells_native(0, pi, ci, 1, 0, 1, 1).unwrap();
        let i = idx_at(&d, 0, 0);
        let r = d.offset_cell_boundary_native(0, pi, ci, i, false, 283);
        out(
            "merged-horz-neighbor",
            "아래 이웃이 가로 병합(폭이 다름)",
            r,
            &d,
        );
    }
    // 4b. 대상 자신이 병합 — 하변(폭 불일치)
    {
        let (mut d, pi, ci) = make();
        d.merge_table_cells_native(0, pi, ci, 0, 0, 0, 1).unwrap();
        let i = idx_at(&d, 0, 0);
        let r = d.offset_cell_boundary_native(0, pi, ci, i, false, 283);
        out(
            "merged-self-horz",
            "대상이 가로 병합된 칸(아래 이웃과 폭 다름)",
            r,
            &d,
        );
    }
    // 5. 이웃 폭 부족
    {
        let (mut d, pi, ci) = make();
        let i = idx_at(&d, 0, 0);
        let r = d.offset_cell_boundary_native(0, pi, ci, i, true, 100_000);
        out("neighbor-no-width", "오른쪽으로 이웃 폭보다 크게", r, &d);
    }
    // 6. 대상 폭 부족
    {
        let (mut d, pi, ci) = make();
        let i = idx_at(&d, 0, 0);
        let r = d.offset_cell_boundary_native(0, pi, ci, i, true, -100_000);
        out("target-no-width", "왼쪽으로 대상 폭보다 크게", r, &d);
    }
    // 7. 이웃 높이 부족
    {
        let (mut d, pi, ci) = make();
        let i = idx_at(&d, 0, 0);
        let r = d.offset_cell_boundary_native(0, pi, ci, i, false, 100_000);
        out("neighbor-no-height", "아래로 이웃 높이보다 크게", r, &d);
    }
    // 8. 대상 높이 부족 — 신선 표(글줄 바닥에 붙어 있음)
    {
        let (mut d, pi, ci) = make();
        let i = idx_at(&d, 0, 0);
        let r = d.offset_cell_boundary_native(0, pi, ci, i, false, -283);
        out(
            "target-no-height-fresh",
            "새 표에서 위로(대상이 글줄 바닥)",
            r,
            &d,
        );
    }
    // 9. 내용이 든 칸을 글줄 아래로 줄이기
    {
        let (mut d, pi, ci) = make();
        let i = idx_at(&d, 1, 0);
        d.insert_text_in_cell_native(0, pi, ci, i, 0, 0, "내용")
            .unwrap();
        let t = idx_at(&d, 0, 0);
        let r = d.offset_cell_boundary_native(0, pi, ci, t, false, 100_000);
        out("content-floor", "글자가 든 아래 칸을 글줄보다 얇게", r, &d);
    }
    // 10. 재이동 한계 — 같은 방향 반복
    {
        let (mut d, pi, ci) = make();
        let mut last = Ok(String::new());
        let mut n = 0;
        for k in 1..=40 {
            let i = idx_at(&d, 0, 0);
            last = d.offset_cell_boundary_native(0, pi, ci, i, true, 566);
            if last.is_err() {
                n = k;
                break;
            }
            n = k;
        }
        out(
            "shift-limit",
            &format!("오른쪽으로 계속 밀기({n}회째)"),
            last,
            &d,
        );
    }
    // 12. 1열 표 — 세로 경계가 아예 없음
    {
        let mut d = HwpDocument::create_empty();
        d.create_blank_document().unwrap();
        let c: serde_json::Value = serde_json::from_str(
            &d.create_table_ex(
                r#"{"sectionIdx":0,"paraIdx":0,"charOffset":0,"rowCount":3,"colCount":1,"treatAsChar":false}"#,
            )
            .unwrap(),
        )
        .unwrap();
        let (pi, ci) = (
            c["paraIdx"].as_u64().unwrap() as usize,
            c["controlIdx"].as_u64().unwrap() as usize,
        );
        let i = idx_at(&d, 0, 0);
        let r = d.offset_cell_boundary_native(0, pi, ci, i, true, 283);
        out("single-column", "1열짜리 표의 우변", r, &d);
    }
    // 13. 1행 표 — 가로 경계가 아예 없음
    {
        let mut d = HwpDocument::create_empty();
        d.create_blank_document().unwrap();
        let c: serde_json::Value = serde_json::from_str(
            &d.create_table_ex(
                r#"{"sectionIdx":0,"paraIdx":0,"charOffset":0,"rowCount":1,"colCount":3,"treatAsChar":false}"#,
            )
            .unwrap(),
        )
        .unwrap();
        let (pi, ci) = (
            c["paraIdx"].as_u64().unwrap() as usize,
            c["controlIdx"].as_u64().unwrap() as usize,
        );
        let i = idx_at(&d, 0, 0);
        let r = d.offset_cell_boundary_native(0, pi, ci, i, false, 283);
        out("single-row", "1행짜리 표의 하변", r, &d);
    }
    // 14. 대상이 세로 병합 — 자기 하변(아래 이웃과 폭 같음)
    {
        let (mut d, pi, ci) = make();
        d.merge_table_cells_native(0, pi, ci, 0, 0, 1, 0).unwrap();
        let i = idx_at(&d, 0, 0);
        let r = d.offset_cell_boundary_native(0, pi, ci, i, false, 283);
        out(
            "merged-self-vert-bottom",
            "대상이 세로 병합된 칸의 아래 경계",
            r,
            &d,
        );
    }
    // 15. 양쪽 다 같은 세로 병합 — 우변
    {
        let (mut d, pi, ci) = make();
        d.merge_table_cells_native(0, pi, ci, 0, 0, 1, 0).unwrap();
        d.merge_table_cells_native(0, pi, ci, 0, 1, 1, 1).unwrap();
        let i = idx_at(&d, 0, 0);
        let r = d.offset_cell_boundary_native(0, pi, ci, i, true, 283);
        out(
            "merged-both-same",
            "양쪽 모두 같은 높이로 병합된 칸 사이",
            r,
            &d,
        );
    }
    // 16. 글자 든 칸 자신을 글줄 아래로(대상 쪽 바닥)
    {
        let (mut d, pi, ci) = make();
        let t = idx_at(&d, 0, 0);
        d.insert_text_in_cell_native(0, pi, ci, t, 0, 0, "내용")
            .unwrap();
        let t2 = idx_at(&d, 0, 0);
        let r = d.offset_cell_boundary_native(0, pi, ci, t2, false, -100_000);
        out(
            "content-floor-self",
            "글자가 든 칸 자신을 글줄보다 얇게",
            r,
            &d,
        );
    }
    // 11. 성공 대조군
    {
        let (mut d, pi, ci) = make();
        let i = idx_at(&d, 0, 0);
        let r = d.offset_cell_boundary_native(0, pi, ci, i, false, 283);
        out("ok-baseline", "정상: 내부 경계 아래로 한 칸", r, &d);
    }
}
