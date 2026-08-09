//! 변경 내용 추적 v1 회귀 — 판정식 ①~④ (mydocs/eng/plans/track-changes.md)

use rhwp::wasm_api::HwpDocument;

fn text0(doc: &HwpDocument) -> String {
    doc.document().sections[0].paragraphs[0].text.clone()
}

fn changes(doc: &HwpDocument) -> Vec<(u32, String, String)> {
    // (id, kind, text)
    let json = doc.get_track_changes();
    let mut out = Vec::new();
    for chunk in json.split("{\"id\":").skip(1) {
        let id: u32 = chunk[..chunk.find(',').unwrap()].parse().unwrap();
        let kind = chunk.split("\"kind\":\"").nth(1).unwrap();
        let kind = kind[..kind.find('"').unwrap()].to_string();
        let text = chunk.split("\"text\":\"").nth(1).unwrap();
        let text = text[..text.find('"').unwrap()].to_string();
        out.push((id, kind, text));
    }
    out
}

#[test]
fn tracked_insert_and_delete() {
    let mut doc = HwpDocument::create_empty();
    doc.insert_text(0, 0, 0, "원본내용").unwrap();

    // ① ON 삽입 — 글자는 들어가고 Insert 변경 1건
    doc.set_track_changes(true, "검토자", "2026-07-30");
    doc.insert_text(0, 0, 2, "추가").unwrap();
    assert_eq!(text0(&doc), "원본추가내용", "삽입 자체는 정상");
    let ch = changes(&doc);
    assert_eq!(ch.len(), 1, "Insert 변경 1건: {ch:?}");
    assert_eq!(ch[0].1, "insert");
    assert_eq!(ch[0].2, "추가", "변경 본문");

    // 이어 치면 같은 변경으로 확장 (1글자 = 1변경 방지)
    doc.insert_text(0, 0, 4, "요").unwrap();
    let ch = changes(&doc);
    assert_eq!(ch.len(), 1, "인접 삽입은 확장: {ch:?}");
    assert_eq!(ch[0].2, "추가요");

    // ② ON 삭제 — 글자는 남고 Delete 마크
    doc.delete_text(0, 0, 0, 1).unwrap(); // '원' 삭제 시도
    assert_eq!(
        text0(&doc),
        "원본추가요내용",
        "추적 삭제는 글자를 지우지 않는다"
    );
    let ch = changes(&doc);
    assert_eq!(ch.len(), 2, "Delete 변경 추가: {ch:?}");
    assert!(ch.iter().any(|c| c.1 == "delete" && c.2 == "원"));

    // ②' 자기 삽입분을 지우면 실삭제 + 마크 축소
    doc.delete_text(0, 0, 4, 1).unwrap(); // '요' (자기 Insert 안)
    assert_eq!(text0(&doc), "원본추가내용", "자기 삽입분은 실삭제");
    let ch = changes(&doc);
    assert!(
        ch.iter().any(|c| c.1 == "insert" && c.2 == "추가"),
        "마크 축소: {ch:?}"
    );

    // ③ 검토 연산 4종
    let ins_id = ch.iter().find(|c| c.1 == "insert").unwrap().0;
    let del_id = ch.iter().find(|c| c.1 == "delete").unwrap().0;
    doc.accept_track_change(del_id).unwrap(); // 삭제 확정 → '원' 소멸
    assert_eq!(text0(&doc), "본추가내용", "accept(Del) = 실삭제");
    doc.reject_track_change(ins_id).unwrap(); // 삽입 취소 → '추가' 소멸
    assert_eq!(text0(&doc), "본내용", "reject(Ins) = 실삭제");
    assert_eq!(changes(&doc).len(), 0, "변경 목록 소진");

    // accept(Ins)/reject(Del) = 마크만 해제
    doc.insert_text(0, 0, 0, "머").unwrap();
    let id = changes(&doc)[0].0;
    doc.accept_track_change(id).unwrap();
    assert_eq!(text0(&doc), "머본내용", "accept(Ins) = 글자 유지");
    doc.delete_text(0, 0, 0, 1).unwrap();
    let id = changes(&doc)[0].0;
    doc.reject_track_change(id).unwrap();
    assert_eq!(text0(&doc), "머본내용", "reject(Del) = 글자 복원(그대로)");
    assert_eq!(changes(&doc).len(), 0);

    // OFF 격리 — 같은 조작이 실삭제로 돌아온다
    doc.set_track_changes(false, "", "");
    doc.delete_text(0, 0, 0, 1).unwrap();
    assert_eq!(text0(&doc), "본내용", "OFF 삭제는 실삭제");
}

#[test]
fn track_survives_hwpx_roundtrip() {
    let mut doc = HwpDocument::create_empty();
    doc.insert_text(0, 0, 0, "원본내용").unwrap();
    doc.set_track_changes(true, "검토자", "2026-07-30");
    doc.insert_text(0, 0, 2, "추가").unwrap();
    doc.delete_text(0, 0, 0, 1).unwrap();
    let before = changes_summary(&doc);
    assert_eq!(before.len(), 2, "사전 조건");

    // ④ HWPX 저장 → 재로드 후 변경·마크 잔존
    let bytes = doc.export_hwpx().unwrap();
    let re = HwpDocument::from_bytes(&bytes).unwrap();
    assert_eq!(text0(&re), "원본추가내용", "본문 유지(삭제 표시 글자 포함)");
    let after = changes_summary(&re);
    assert_eq!(after, before, "변경 목록이 왕복을 살아남아야 한다");
}

fn changes_summary(doc: &HwpDocument) -> Vec<(String, String, String)> {
    changes(doc)
        .into_iter()
        .map(|(_, k, t)| (k, t, "검토자".to_string()))
        .collect()
}

fn cell_text(doc: &HwpDocument, host: u32, ctrl_hint: u32, cei: usize) -> String {
    let _ = ctrl_hint;
    let host = &doc.document().sections[0].paragraphs[host as usize];
    for ctrl in &host.controls {
        if let rhwp::model::control::Control::Table(t) = ctrl {
            return t.cells[cei].paragraphs[0].text.clone();
        }
    }
    String::new()
}

#[test]
fn tracked_range_and_cell_edits() {
    let mut doc = HwpDocument::create_empty();
    doc.insert_text(0, 0, 0, "가나다라마").unwrap();
    doc.set_track_changes(true, "검토자", "2026-07-30");

    // 선택(범위) 삭제 — 지워지지 않고 Delete 마크
    doc.delete_range_native(0, 0, 1, 0, 3, None).unwrap();
    assert_eq!(
        text0(&doc),
        "가나다라마",
        "범위 삭제도 글자를 지우지 않는다"
    );
    let ch = changes(&doc);
    assert!(
        ch.iter().any(|c| c.1 == "delete" && c.2 == "나다"),
        "범위 마크: {ch:?}"
    );

    // 셀 편집 — 삽입·삭제 모두 추적
    doc.set_track_changes(false, "", "");
    let t: serde_like::Table = serde_like::create_table(&mut doc);
    doc.set_track_changes(true, "검토자", "2026-07-30");
    doc.insert_text_in_cell_native(0, t.para as usize, t.ctrl as usize, 0, 0, 0, "셀추가")
        .unwrap();
    assert_eq!(
        cell_text(&doc, t.para, t.ctrl, 0),
        "셀추가",
        "셀 삽입 자체는 정상"
    );
    let ch = changes(&doc);
    assert!(
        ch.iter().any(|c| c.1 == "insert" && c.2 == "셀추가"),
        "셀 Insert 마크: {ch:?}"
    );

    // 셀 삭제 (자기 삽입이 아닌 기존 글자) — 먼저 추적 끄고 글자 심기
    doc.set_track_changes(false, "", "");
    doc.insert_text_in_cell_native(0, t.para as usize, t.ctrl as usize, 1, 0, 0, "원본")
        .unwrap();
    doc.set_track_changes(true, "검토자", "2026-07-30");
    doc.delete_text_in_cell_native(0, t.para as usize, t.ctrl as usize, 1, 0, 0, 1)
        .unwrap();
    assert_eq!(
        cell_text(&doc, t.para, t.ctrl, 1),
        "원본",
        "셀 추적 삭제는 글자 유지"
    );
    let ch = changes(&doc);
    assert!(
        ch.iter().any(|c| c.1 == "delete" && c.2 == "원"),
        "셀 Delete 마크: {ch:?}"
    );

    let before: Vec<(String, String)> = changes(&doc).into_iter().map(|(_, k, t)| (k, t)).collect();
    // 사이드카 왕복 — 셀 마크 잔존.
    // (전체 HWPX export 는 합성 문서(create_empty+표)의 기존 ID 미등록 문제로 막힌다 —
    //  charPrIDRef 0 미등록. 본문 전체 왕복은 위 테스트가, 여기선 사이드카 논리를 검증)
    let sidecar = rhwp::serializer::track_sidecar::build_track_sidecar(doc.document())
        .expect("사이드카 생성");
    assert!(
        sidecar.contains("\"cei\":"),
        "셀 좌표가 실려야 함: {sidecar}"
    );
    {
        let d = doc.document_mut();
        let ids: Vec<u32> = d.track_changes.iter().map(|r| r.id).collect();
        d.track_changes.clear();
        d.next_track_id = 1;
        for sec in &mut d.sections {
            for host in &mut sec.paragraphs {
                for ctrl in &mut host.controls {
                    if let rhwp::model::control::Control::Table(tb) = ctrl {
                        for cell in &mut tb.cells {
                            for para in &mut cell.paragraphs {
                                para.track_marks.clear();
                            }
                        }
                    }
                }
                host.track_marks.clear();
            }
        }
        let _ = ids;
        rhwp::serializer::track_sidecar::restore_track_sidecar(d, &sidecar);
    }
    let after: Vec<(String, String)> = changes(&doc).into_iter().map(|(_, k, t)| (k, t)).collect();
    assert_eq!(after, before, "셀 마크 포함 사이드카 왕복 보존");

    // 모두 적용 — 셀 실삭제까지
    doc.resolve_all_track_changes_native(true).unwrap();
    assert_eq!(
        cell_text(&doc, t.para, t.ctrl, 1),
        "본",
        "accept → 셀 실삭제"
    );
    assert_eq!(changes(&doc).len(), 0);
}

/// 테스트 안 표 생성 헬퍼 — wasm createTable 반환(JSON)을 얇게 파싱
mod serde_like {
    use super::*;
    pub struct Table {
        pub para: u32,
        pub ctrl: u32,
    }
    pub fn create_table(doc: &mut HwpDocument) -> Table {
        let r = doc.create_table_native(0, 0, 0, 2, 2).unwrap();
        let get = |k: &str| -> u32 {
            let s = r.split(&format!("\"{}\":", k)).nth(1).unwrap();
            s[..s.find([',', '}']).unwrap()].parse().unwrap()
        };
        Table {
            para: get("paraIdx"),
            ctrl: get("controlIdx"),
        }
    }
}
