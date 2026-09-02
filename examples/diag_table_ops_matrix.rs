//! [진단 2026-08-17] 표 조작 전 영역 되는/안 되는 상황 실측 매트릭스.
//! 카테고리: 어긋내기 / 크기조절 / 병합·나누기 / 행열 삽입삭제 / 어긋 후 조작.
//! 각 케이스마다 새 표를 만들어 실제로 실행하고 결과 + 표 불변식을 JSON 으로 찍는다.
use rhwp::wasm_api::HwpDocument;

fn make_n(rows: u32, cols: u32) -> (HwpDocument, usize, usize) {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    let json = format!(
        r#"{{"sectionIdx":0,"paraIdx":0,"charOffset":0,"rowCount":{rows},"colCount":{cols},"treatAsChar":false}}"#
    );
    let c: serde_json::Value = serde_json::from_str(&doc.create_table_ex(&json).unwrap()).unwrap();
    (
        doc,
        c["paraIdx"].as_u64().unwrap() as usize,
        c["controlIdx"].as_u64().unwrap() as usize,
    )
}
fn make() -> (HwpDocument, usize, usize) {
    make_n(3, 3)
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

fn idx_at(doc: &HwpDocument, row: u16, col: u16) -> usize {
    table_of(doc)
        .cells
        .iter()
        .position(|c| c.row == row && c.col == col)
        .unwrap_or(usize::MAX)
}

/// (셀 수, 행 수, 열 폭 합, 실효 높이 합)
fn state(doc: &HwpDocument) -> (usize, u16, u32, u32) {
    let t = table_of(doc);
    (
        t.cells.len(),
        t.row_count,
        t.get_column_widths().iter().sum(),
        t.effective_row_heights().iter().sum(),
    )
}

fn msg(r: &Result<String, rhwp::error::HwpError>) -> String {
    match r {
        Ok(_) => "OK".into(),
        Err(e) => format!("{e:?}")
            .replace("RenderError(\"", "")
            .replace("InvalidArgument(\"", "")
            .replace("\")", ""),
    }
}

struct Rec<'a> {
    cat: &'a str,
    id: &'a str,
    label: &'a str,
}

fn emit(
    rec: Rec,
    r: Result<String, rhwp::error::HwpError>,
    before: (usize, u16, u32, u32),
    doc: &HwpDocument,
) {
    let after = state(doc);
    let w_keep = before.2 == after.2;
    let h_keep = before.3.abs_diff(after.3) <= 4;
    println!(
        "{{\"cat\":\"{}\",\"id\":\"{}\",\"label\":\"{}\",\"result\":\"{}\",\"cells\":\"{}→{}\",\"rows\":\"{}→{}\",\"widthKeep\":{},\"heightKeep\":{}}}",
        rec.cat, rec.id, rec.label, msg(&r), before.0, after.0, before.1, after.1, w_keep, h_keep
    );
}

fn main() {
    // ══════════ A. 어긋내기(한 칸 경계) ══════════
    {
        let (mut d, pi, ci) = make();
        let b = state(&d);
        let i = idx_at(&d, 0, 0);
        let r = d.offset_cell_boundary_native(0, pi, ci, i, false, 283);
        emit(
            Rec {
                cat: "어긋내기",
                id: "a-down",
                label: "내부 가로 경계를 아래로",
            },
            r,
            b,
            &d,
        );
    }
    {
        let (mut d, pi, ci) = make();
        let b = state(&d);
        let i = idx_at(&d, 1, 0);
        let r = d.offset_cell_boundary_native(0, pi, ci, i, false, -283);
        emit(
            Rec {
                cat: "어긋내기",
                id: "a-up",
                label: "내부 가로 경계를 위로",
            },
            r,
            b,
            &d,
        );
    }
    {
        let (mut d, pi, ci) = make();
        let b = state(&d);
        let i = idx_at(&d, 0, 0);
        let r = d.offset_cell_boundary_native(0, pi, ci, i, true, 283);
        emit(
            Rec {
                cat: "어긋내기",
                id: "a-right",
                label: "내부 세로 경계를 오른쪽으로",
            },
            r,
            b,
            &d,
        );
    }
    {
        let (mut d, pi, ci) = make();
        let b = state(&d);
        let i = idx_at(&d, 0, 1);
        let r = d.offset_cell_boundary_native(0, pi, ci, i, true, -283);
        emit(
            Rec {
                cat: "어긋내기",
                id: "a-left",
                label: "내부 세로 경계를 왼쪽으로",
            },
            r,
            b,
            &d,
        );
    }
    {
        let (mut d, pi, ci) = make();
        let b = state(&d);
        let i = idx_at(&d, 2, 0);
        let r = d.offset_cell_boundary_native(0, pi, ci, i, false, 283);
        emit(
            Rec {
                cat: "어긋내기",
                id: "a-outer-bottom",
                label: "표의 맨 아래(바깥) 테두리",
            },
            r,
            b,
            &d,
        );
    }
    {
        let (mut d, pi, ci) = make();
        let b = state(&d);
        let i = idx_at(&d, 0, 2);
        let r = d.offset_cell_boundary_native(0, pi, ci, i, true, 283);
        emit(
            Rec {
                cat: "어긋내기",
                id: "a-outer-right",
                label: "표의 맨 오른쪽(바깥) 테두리",
            },
            r,
            b,
            &d,
        );
    }
    {
        let (mut d, pi, ci) = make_n(3, 1);
        let b = state(&d);
        let i = idx_at(&d, 0, 0);
        let r = d.offset_cell_boundary_native(0, pi, ci, i, true, 283);
        emit(
            Rec {
                cat: "어긋내기",
                id: "a-1col",
                label: "1열 표의 세로 경계",
            },
            r,
            b,
            &d,
        );
    }
    {
        let (mut d, pi, ci) = make_n(1, 3);
        let b = state(&d);
        let i = idx_at(&d, 0, 0);
        let r = d.offset_cell_boundary_native(0, pi, ci, i, false, 283);
        emit(
            Rec {
                cat: "어긋내기",
                id: "a-1row",
                label: "1행 표의 가로 경계",
            },
            r,
            b,
            &d,
        );
    }
    {
        let (mut d, pi, ci) = make();
        d.merge_table_cells_native(0, pi, ci, 0, 1, 1, 1).unwrap();
        let b = state(&d);
        let i = idx_at(&d, 0, 0);
        let r = d.offset_cell_boundary_native(0, pi, ci, i, true, 283);
        emit(
            Rec {
                cat: "어긋내기",
                id: "a-merge-vert-nb",
                label: "오른쪽 이웃이 세로 병합(높이 불일치)",
            },
            r,
            b,
            &d,
        );
    }
    {
        let (mut d, pi, ci) = make();
        d.merge_table_cells_native(0, pi, ci, 1, 0, 1, 1).unwrap();
        let b = state(&d);
        let i = idx_at(&d, 0, 0);
        let r = d.offset_cell_boundary_native(0, pi, ci, i, false, 283);
        emit(
            Rec {
                cat: "어긋내기",
                id: "a-merge-horz-nb",
                label: "아래 이웃이 가로 병합(폭 불일치)",
            },
            r,
            b,
            &d,
        );
    }
    {
        let (mut d, pi, ci) = make();
        d.merge_table_cells_native(0, pi, ci, 0, 0, 1, 0).unwrap();
        d.merge_table_cells_native(0, pi, ci, 0, 1, 1, 1).unwrap();
        let b = state(&d);
        let i = idx_at(&d, 0, 0);
        let r = d.offset_cell_boundary_native(0, pi, ci, i, true, 283);
        emit(
            Rec {
                cat: "어긋내기",
                id: "a-merge-both",
                label: "양쪽 모두 같은 높이로 병합",
            },
            r,
            b,
            &d,
        );
    }
    {
        let (mut d, pi, ci) = make();
        let i = idx_at(&d, 1, 0);
        d.insert_text_in_cell_native(0, pi, ci, i, 0, 0, "내용")
            .unwrap();
        let b = state(&d);
        let t = idx_at(&d, 0, 0);
        let r = d.offset_cell_boundary_native(0, pi, ci, t, false, 100_000);
        emit(
            Rec {
                cat: "어긋내기",
                id: "a-content-nb",
                label: "글자 든 아래 칸을 글줄보다 얇게",
            },
            r,
            b,
            &d,
        );
    }
    {
        let (mut d, pi, ci) = make();
        let b = state(&d);
        let i = idx_at(&d, 0, 0);
        let r = d.offset_cell_boundary_native(0, pi, ci, i, true, 100_000);
        emit(
            Rec {
                cat: "어긋내기",
                id: "a-big-delta",
                label: "한 번에 아주 크게(한계 클램프)",
            },
            r,
            b,
            &d,
        );
    }
    {
        let (mut d, pi, ci) = make();
        let b = state(&d);
        let mut last = Ok(String::new());
        for _ in 0..40 {
            let i = idx_at(&d, 0, 0);
            last = d.offset_cell_boundary_native(0, pi, ci, i, true, 566);
            if last.is_err() {
                break;
            }
        }
        emit(
            Rec {
                cat: "어긋내기",
                id: "a-repeat-limit",
                label: "같은 방향 반복(이웃 소진까지)",
            },
            last,
            b,
            &d,
        );
    }
    // 재이동·복원
    {
        let (mut d, pi, ci) = make();
        let i = idx_at(&d, 0, 0);
        d.offset_cell_boundary_native(0, pi, ci, i, false, 566)
            .unwrap();
        let b = state(&d);
        let i2 = idx_at(&d, 0, 0);
        let r = d.offset_cell_boundary_native(0, pi, ci, i2, false, 283);
        emit(
            Rec {
                cat: "어긋내기",
                id: "a-reshift-same",
                label: "이미 어긋난 경계를 같은 방향으로 더",
            },
            r,
            b,
            &d,
        );
    }
    {
        let (mut d, pi, ci) = make();
        let i = idx_at(&d, 0, 0);
        d.offset_cell_boundary_native(0, pi, ci, i, false, 566)
            .unwrap();
        let b = state(&d);
        let i2 = idx_at(&d, 0, 0);
        let r = d.offset_cell_boundary_native(0, pi, ci, i2, false, -283);
        emit(
            Rec {
                cat: "어긋내기",
                id: "a-reshift-back",
                label: "어긋난 경계를 반대 방향으로 되돌리기",
            },
            r,
            b,
            &d,
        );
    }
    {
        let (mut d, pi, ci) = make();
        let i = idx_at(&d, 0, 0);
        d.offset_cell_boundary_native(0, pi, ci, i, false, 566)
            .unwrap();
        let b = state(&d);
        let i2 = idx_at(&d, 0, 0);
        let r = d.restore_cell_boundary_native(0, pi, ci, i2, false);
        emit(
            Rec {
                cat: "어긋내기",
                id: "a-restore",
                label: "어긋낸 경계를 원래대로 복원",
            },
            r,
            b,
            &d,
        );
    }
    {
        let (mut d, pi, ci) = make();
        let i = idx_at(&d, 0, 0);
        d.offset_cell_boundary_native(0, pi, ci, i, false, 566)
            .unwrap();
        let b = state(&d);
        let i2 = idx_at(&d, 0, 1);
        let r = d.offset_cell_boundary_native(0, pi, ci, i2, false, 1132);
        emit(
            Rec {
                cat: "어긋내기",
                id: "a-cross-line",
                label: "이웃 열이 만든 기준선을 넘어 어긋내기",
            },
            r,
            b,
            &d,
        );
    }
    {
        let (mut d, pi, ci) = make();
        let i = idx_at(&d, 0, 0);
        d.offset_cell_boundary_native(0, pi, ci, i, true, 566)
            .unwrap();
        let b = state(&d);
        let i2 = idx_at(&d, 1, 0);
        let r = d.offset_cell_boundary_native(0, pi, ci, i2, true, 283);
        emit(
            Rec {
                cat: "어긋내기",
                id: "a-other-row",
                label: "한 행 어긋낸 뒤 다른 행의 같은 경계",
            },
            r,
            b,
            &d,
        );
    }

    // ══════════ B. 크기조절(경계선 드래그 = 행/열 전체) ══════════
    {
        let (mut d, pi, ci) = make();
        let b = state(&d);
        let r = d
            .resize_table_cells(
                0,
                pi as u32,
                ci as u32,
                r#"[{"cellIdx":0,"widthDelta":1000}]"#,
            )
            .map(|_| String::new())
            .map_err(|_| rhwp::error::HwpError::RenderError("실패".into()));
        emit(
            Rec {
                cat: "크기조절",
                id: "b-col-wider",
                label: "열 폭 넓히기",
            },
            r,
            b,
            &d,
        );
    }
    {
        let (mut d, pi, ci) = make();
        let b = state(&d);
        let r = d
            .resize_table_cells(
                0,
                pi as u32,
                ci as u32,
                r#"[{"cellIdx":0,"widthDelta":-1000}]"#,
            )
            .map(|_| String::new())
            .map_err(|_| rhwp::error::HwpError::RenderError("실패".into()));
        emit(
            Rec {
                cat: "크기조절",
                id: "b-col-narrow",
                label: "열 폭 좁히기",
            },
            r,
            b,
            &d,
        );
    }
    {
        let (mut d, pi, ci) = make();
        let b = state(&d);
        let r = d
            .resize_table_cells(
                0,
                pi as u32,
                ci as u32,
                r#"[{"cellIdx":0,"widthDelta":-1000000}]"#,
            )
            .map(|_| String::new())
            .map_err(|_| rhwp::error::HwpError::RenderError("실패".into()));
        emit(
            Rec {
                cat: "크기조절",
                id: "b-col-min",
                label: "열 폭을 0 이하로(최소 클램프)",
            },
            r,
            b,
            &d,
        );
    }
    {
        let (mut d, pi, ci) = make();
        let b = state(&d);
        let r = d
            .resize_table_cells(0, pi as u32, ci as u32, r#"[{"cellIdx":0,"heightDelta":2000},{"cellIdx":1,"heightDelta":2000},{"cellIdx":2,"heightDelta":2000}]"#)
            .map(|_| String::new())
            .map_err(|_| rhwp::error::HwpError::RenderError("실패".into()));
        emit(
            Rec {
                cat: "크기조절",
                id: "b-row-taller",
                label: "행 높이 키우기(행 전체)",
            },
            r,
            b,
            &d,
        );
    }
    {
        let (mut d, pi, ci) = make();
        let b = state(&d);
        let r = d
            .resize_table_cells(0, pi as u32, ci as u32, r#"[{"cellIdx":0,"heightDelta":-2000},{"cellIdx":1,"heightDelta":-2000},{"cellIdx":2,"heightDelta":-2000}]"#)
            .map(|_| String::new())
            .map_err(|_| rhwp::error::HwpError::RenderError("실패".into()));
        emit(
            Rec {
                cat: "크기조절",
                id: "b-row-shorter",
                label: "새 표에서 행 높이 줄이기(글줄 바닥)",
            },
            r,
            b,
            &d,
        );
    }
    {
        let (mut d, pi, ci) = make();
        let i = idx_at(&d, 0, 0);
        d.offset_cell_boundary_native(0, pi, ci, i, false, 566)
            .unwrap();
        let b = state(&d);
        let r = d
            .resize_table_cells(
                0,
                pi as u32,
                ci as u32,
                r#"[{"cellIdx":0,"widthDelta":800}]"#,
            )
            .map(|_| String::new())
            .map_err(|_| rhwp::error::HwpError::RenderError("실패".into()));
        emit(
            Rec {
                cat: "크기조절",
                id: "b-after-stagger",
                label: "어긋난 표에서 열 폭 조절",
            },
            r,
            b,
            &d,
        );
    }

    // ══════════ C. 병합 / 나누기 ══════════
    {
        let (mut d, pi, ci) = make();
        let b = state(&d);
        let r = d.merge_table_cells_native(0, pi, ci, 0, 0, 0, 1);
        emit(
            Rec {
                cat: "병합·나누기",
                id: "c-merge-horz",
                label: "가로로 두 칸 병합",
            },
            r,
            b,
            &d,
        );
    }
    {
        let (mut d, pi, ci) = make();
        let b = state(&d);
        let r = d.merge_table_cells_native(0, pi, ci, 0, 0, 1, 1);
        emit(
            Rec {
                cat: "병합·나누기",
                id: "c-merge-block",
                label: "2×2 블록 병합",
            },
            r,
            b,
            &d,
        );
    }
    {
        let (mut d, pi, ci) = make();
        let b = state(&d);
        let r = d.merge_table_cells_native(0, pi, ci, 0, 0, 0, 0);
        emit(
            Rec {
                cat: "병합·나누기",
                id: "c-merge-single",
                label: "한 칸만 지정해 병합",
            },
            r,
            b,
            &d,
        );
    }
    {
        let (mut d, pi, ci) = make();
        let b = state(&d);
        let r = d.merge_table_cells_native(0, pi, ci, 0, 0, 5, 5);
        emit(
            Rec {
                cat: "병합·나누기",
                id: "c-merge-oob",
                label: "표 밖 범위까지 병합",
            },
            r,
            b,
            &d,
        );
    }
    {
        let (mut d, pi, ci) = make();
        d.merge_table_cells_native(0, pi, ci, 0, 0, 0, 1).unwrap();
        let b = state(&d);
        let r = d.split_table_cell_native(0, pi, ci, 0, 0);
        emit(
            Rec {
                cat: "병합·나누기",
                id: "c-split-merged",
                label: "병합된 칸 나누기",
            },
            r,
            b,
            &d,
        );
    }
    {
        let (mut d, pi, ci) = make();
        let b = state(&d);
        let r = d.split_table_cell_native(0, pi, ci, 0, 0);
        emit(
            Rec {
                cat: "병합·나누기",
                id: "c-split-plain",
                label: "병합 안 된 칸 나누기",
            },
            r,
            b,
            &d,
        );
    }
    {
        let (mut d, pi, ci) = make();
        let i = idx_at(&d, 0, 0);
        d.offset_cell_boundary_native(0, pi, ci, i, false, 566)
            .unwrap();
        let b = state(&d);
        let r = d.merge_table_cells_native(0, pi, ci, 0, 0, 1, 1);
        emit(
            Rec {
                cat: "병합·나누기",
                id: "c-merge-after-stagger",
                label: "어긋낸 표에서 블록 병합",
            },
            r,
            b,
            &d,
        );
    }

    // ══════════ D. 행·열 삽입 / 삭제 ══════════
    {
        let (mut d, pi, ci) = make();
        let b = state(&d);
        let r = d.insert_table_row_native(0, pi, ci, 1, true);
        emit(
            Rec {
                cat: "행·열",
                id: "d-row-insert",
                label: "행 아래에 삽입",
            },
            r,
            b,
            &d,
        );
    }
    {
        let (mut d, pi, ci) = make();
        let b = state(&d);
        let r = d.insert_table_row_native(0, pi, ci, 0, false);
        emit(
            Rec {
                cat: "행·열",
                id: "d-row-insert-above",
                label: "행 위에 삽입",
            },
            r,
            b,
            &d,
        );
    }
    {
        let (mut d, pi, ci) = make();
        let b = state(&d);
        let r = d.delete_table_row_native(0, pi, ci, 1);
        emit(
            Rec {
                cat: "행·열",
                id: "d-row-delete",
                label: "가운데 행 삭제",
            },
            r,
            b,
            &d,
        );
    }
    {
        let (mut d, pi, ci) = make_n(1, 3);
        let b = state(&d);
        let r = d.delete_table_row_native(0, pi, ci, 0);
        emit(
            Rec {
                cat: "행·열",
                id: "d-row-delete-last",
                label: "마지막 남은 행 삭제",
            },
            r,
            b,
            &d,
        );
    }
    {
        let (mut d, pi, ci) = make();
        d.merge_table_cells_native(0, pi, ci, 0, 0, 1, 0).unwrap();
        let b = state(&d);
        let r = d.delete_table_row_native(0, pi, ci, 0);
        emit(
            Rec {
                cat: "행·열",
                id: "d-row-delete-merged",
                label: "세로 병합이 걸친 행 삭제",
            },
            r,
            b,
            &d,
        );
    }
    {
        let (mut d, pi, ci) = make();
        let i = idx_at(&d, 0, 0);
        d.offset_cell_boundary_native(0, pi, ci, i, false, 566)
            .unwrap();
        let b = state(&d);
        let r = d.insert_table_row_native(0, pi, ci, 0, true);
        emit(
            Rec {
                cat: "행·열",
                id: "d-row-insert-stagger",
                label: "어긋낸 표에 행 삽입",
            },
            r,
            b,
            &d,
        );
    }
    {
        let (mut d, pi, ci) = make();
        let i = idx_at(&d, 0, 0);
        d.offset_cell_boundary_native(0, pi, ci, i, false, 566)
            .unwrap();
        let b = state(&d);
        let r = d.delete_table_row_native(0, pi, ci, 2);
        emit(
            Rec {
                cat: "행·열",
                id: "d-row-delete-stagger",
                label: "어긋낸 표에서 행 삭제",
            },
            r,
            b,
            &d,
        );
    }
}
