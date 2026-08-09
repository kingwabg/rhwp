# 구역 나누기 (Alt+Shift+Enter) — 실행 설계

## 전제(완료)
- 다중 구역 **읽기 경로는 전부 있다**: 파서(HWP5/HWPX)·컴포저·페이지네이션·직렬화가
  `document.sections: Vec<Section>` 을 일반적으로 순회한다
  (serializer/hwpx/mod.rs:72, serializer/body_text.rs — 구역별 BodyText 스트림).
- 문단 분리 원형 = `insert_column_break_native` (document_core/commands/text_editing.rs:1901)
  — split_at → 리플로우 → recompose → paginate 순서가 굳어 있다.
- 직렬화기는 문단0에 SectionDef 컨트롤이 없으면 `section.section_def` 로 보강한다
  (body_text.rs:40 Issue #1915). 단 **컨트롤이 있으면 컨트롤이 이긴다**(바탕쪽 함정과 동일)
  — 새 구역 문단0에는 컨트롤을 명시적으로 심는 편이 안전.

## 무엇을 만드나
1. **엔진** `insert_section_break_native(sec, para, offset)` (text_editing.rs, 컬럼브레이크 아래):
   - 문단을 offset 에서 split → 뒷부분부터 끝까지를 새 `Section` 으로 이동.
   - 새 구역 `section_def` = 원 구역 clone, `page_num=0`(이어서), `raw_stream=None` (양쪽).
   - 새 구역 문단0 controls 앞에 `Control::SectionDef(clone)` 삽입
     (기존 문단0에서 온 SectionDef/ColumnDef 컨트롤이 딸려 왔다면 중복 방지로 먼저 제거).
   - 두 구역 recompose + paginate + page tree 캐시 무효화.
   - 반환 `{ok, sectionIdx: sec+1, paraIdx: 0, charOffset: 0}`.
2. **wasm_api** `insertSectionBreak` — insertColumnBreak 와 동일 서명 패턴.
3. **studio**: bridge 메서드 + `page:section-break` 명령(스냅샷 undo, 커서를 새 구역 머리로)
   + 단축키 `Alt+Shift+Enter` + 레이아웃 리본 '구역 나누기' 버튼(구역 설정 옆).
- **하지 말 것**: SectionDef 의미 변경, 기존 다중 구역 렌더 경로 수정, HWP3 경로 배려(대상 아님).

## 판정식
① 1구역 2문단 문서에서 2문단 머리에 실행 → `sections.len() 1→2`, 새 구역 문단 수 1,
   원 구역 문단 수 1, 본문 텍스트 총량 불변.
② 나누기 후 새 구역만 `setPageDef(landscape:true)` → `getPageDef(0).landscape=false`,
   `getPageDef(1).landscape=true` (구역 독립성 = 이 기능의 존재 이유).
③ 저장(HWP5)→재로드 후에도 ①②가 유지된다 (구역 수·가로/세로 독립).
④ studio: Alt+Shift+Enter → 커서가 (구역+1, 문단0, 0) 으로 이동, Ctrl+Z 로 1구역 복귀.

## 검증 사다리
cargo test --lib --tests(T1) → wasm 재빌드 → studio e2e 신설(section-break) →
스윕은 배포 사이클에서.

## 위험 핀·중단 규칙
- 기존 저장 왕복 테스트(HWP5/HWPX roundtrip 계열)가 깨지면 즉시 중단·되돌림.
- 페이지네이션이 구역 경계에서 새 쪽을 시작하지 않으면(한컴 정본: 구역은 새 쪽부터)
  그 동작 확인 후 조정 — 오라클: 한컴에서 구역 나누기 한 문서의 PrvImage.

## 실행 결과
- 판정식 ①②③ = tests/section_break.rs 초록 (구역 1→2 · 텍스트 총량 불변 · landscape 독립 ·
  HWP5 왕복 유지). ④ = studio e2e section-break 초록 (커서 (1,0,0) · Ctrl+Z 복귀 · 리본 버튼).
- T1 전수 초록(exit 0). 설계 변경 1건: 문단 머리에서 나누면 원 구역 끝에 빈 문단이 남는다 —
  split_at 의 Enter 계열 의미론(쪽/단 나누기와 동일)을 따르기로 확정, 판정식 ① 기대값 수정.
- 미반영 한계: 구역 **병합**(나누기 취소를 문서 편집으로 하기) 없음 — Ctrl+Z 만 지원.
  HWPX 왕복은 일반 경로 검증에 의존(전용 판정 없음). 오라클(PrvImage) 대조는 미실시.
