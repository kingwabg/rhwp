# 변경 내용 추적 v1 (실행 설계)

## 전제(완료)
- 엔진에 추적 실체 없음(실측 2026-07-30): TRACKCHANGE 는 태그 상수·DocInfo ID 슬롯 이름뿐,
  HWPX 는 빈 `<hh:trackchageConfig flags="0"/>`.
- 글자 단위 범위의 편집-생존 패턴은 이미 있다: `CharShapeRef.start_pos`(utf16)를
  insert(paragraph.rs:568)·delete(:660)·split(:757)에서 이동 — track 마크는 이걸 미러.
- studio 오버레이 패턴: 맞춤법 물결선(selection-renderer 변형) — 조판을 건드리지 않고
  범위 위에 그린다.

## 무엇을 만드나 (v1 경계)
1. **모델**: `Paragraph.track_marks: Vec<TrackMark{start_pos,end_pos,tc_id}>`(utf16),
   `Document.track_changes: Vec<TrackChangeRec{id,kind(Ins|Del),author,date}>`.
   char_shapes 와 같은 세 지점에서 위치 이동(+merge 는 v1 미지원 명시).
2. **편집 훅** (본문 문단만 — 셀·머리말·각주는 v2):
   - 추적 ON 삽입: 정상 삽입 후 Insert 마크 추가(직전 마크와 인접·동일 작성자면 확장).
   - 추적 ON 삭제: 지우지 않고 Delete 마크. 단 자기 Insert 마크 안이면 실삭제+마크 축소.
3. **검토 연산**: accept/reject(id)·acceptAll/rejectAll.
   accept(Del)=실삭제, reject(Ins)=실삭제, accept(Ins)/reject(Del)=마크 해제.
4. **조회**: getTrackChanges() → [{id,kind,author,date,sec,para,start,end(논리),text}].
5. **저장**: HWPX zip 에 사이드카 `Contents/officexTrack.json`(changes+marks) 기록,
   파서는 있으면 복원. ⚠ 한컴 표준 태그(insertBegin/End)는 v2 — 한계 명시:
   한컴에서 열면 표시 없음(본문은 온전), 한컴에서 저장하면 추적 데이터 소실.
   HWP5 저장은 추적 데이터 미보존(동일 한계 명시).
6. **studio**: 추적 오버레이(삽입=색 밑줄, 삭제=색 취소선) + 검토 탭 배선
   (추적 켜기 토글·적용·취소·모두 적용/취소·다음/이전) + 삭제 시 커서 한 칸 이동 유지.
- **하지 말 것**: 조판(렌더러) 수정, char_shapes 의미 변경, 셀 경로 훅.

## 판정식
① ON 삽입 → getTrackChanges 에 Ins 1건(author·text 일치), 문서 텍스트에 글자 존재.
② ON 백스페이스 → 글자 **남고** Del 마크 1건. OFF 후 같은 조작 → 글자 사라짐(훅 격리).
③ reject(Ins)→글자 소멸 · accept(Del)→글자 소멸 · accept(Ins)/reject(Del)→마크만 소멸,
   네 경우 모두 track_changes 에서 해당 항목 제거.
④ HWPX 저장→재로드 → 변경·마크 잔존(kind·author·범위·본문 일치).
⑤ studio: 토글 ON 타이핑 → 오버레이 1개 표시, 적용 → 오버레이 소멸. Ctrl+Z 로 복원.

## 검증 사다리
cargo test --lib --tests(T1) → wasm 재빌드 → studio e2e(track-changes) 신설 → 게이트.

## 위험 핀·중단 규칙
- 기존 HWPX 왕복·insert/delete 계열 테스트가 깨지면 즉시 중단·되돌림.
- 추적 OFF 경로의 삽입/삭제 동작이 1바이트라도 달라지면 중단(훅은 ON 일 때만).

## 실행 결과
- 판정식 ①~④ = tests/track_changes.rs 2건 초록(삽입 기록·확장, 삭제 마크·자기삽입 실삭제,
  검토 4종, HWPX 왕복 잔존). ⑤ = studio e2e track-changes 초록(오버레이 표시·소멸,
  accept-all, Ctrl+Z 복원, 검토 리본 5명령). T1 전수 실패 0건.
- 실측으로 잡은 함정 2건: ①재파싱 utf16 오프셋이 컨트롤만큼 밀림(0→16) → 사이드카 좌표를
  텍스트 문자 인덱스로 ②논리 오프셋 역변환이 selection API 어법과 어긋남 → 수식 역산 대신
  logical_to_text_pos 역함수 탐색으로 정의.
- 미반영 한계(스펙 예고대로): 한컴 표준 태그(insertBegin/End)는 v2 — 한컴에서 열면 표시
  없음·한컴 저장 시 소실. HWP5 저장 미보존. 셀·머리말·각주 문단 미추적. 작성자 '검토자'
  고정(설정 연동 v2). 문단 병합 시 마크 이동 미지원.
