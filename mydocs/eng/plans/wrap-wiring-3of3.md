# 어울림 배선 3/3 — 소비 (실행 설계)

전제(완료): 계산부 stack_lines_through_bands(float_placement.rs, 테스트 4건) ·
운반로 ColumnContent.topbottom_bands · 생산(typeset flush 2곳 드레인, 0b0db81).
이 문서만 보고 시작할 수 있게 쓴다.

## 무엇을 소비하나
1. **layout 예약 등록**: build_single_column 컬럼 루프에서 visible_float_exclusions 를
   지역 Vec 으로 쓰는 지점(layout.rs:4349 부근 init, 소비 :5057, 자체 등록 :6564).
   ColumnContent.topbottom_bands 를 **para_start_y 확정 시점에**(해당 문단 도달 시)
   절대좌표로 resolve 해 같은 Vec 에 push. 자체 등록(:6564)과 구간이 겹치면 중복은
   무해(skip 은 max-bottom)지만, 같은 (para,ci) 는 하나만 남기는 편이 깔끔.
2. **줄 단위 교체(본편)**: paragraph_layout.rs 줄 루프(:2761 부근)의 누적 y 를
   stack_lines_through_bands 결과로. 인자 19개 함수에 인자 추가 금지 —
   LayoutEngine 인테리어 셀(current_paper_height 패턴, layout.rs:1320대)로 밴드 전달.
   vpos 재생 분기(endnote_line_vpos_base / para_topbottom_line_vpos_base)가 잡힌
   문단은 재생 우선(교체하지 않음).
3. **typeset 예산 짝맞춤**: 문단 fit 높이를 같은 함수로 —
   apply_visible_float_exclusions 호출부(:11334)의 문단에 대해 fmt.line_heights ×
   line_spacings 로 (ink, spacing) 배열을 만들어 end_y − start_y 를 fit 높이로.
   **layout 과 같은 커밋**이어야 한다(한쪽만 바꾸면 페이지 바닥 붕괴 — S3·S4 병력).

## 좌표 규약
band.top(절대) = para_start_y + offset_from_para_top. 프로브는 잉크 높이만(#1789).
owner 스킵: 자기 문단 제목은 안 밀림(#1549). HWPX typeset 프로브는 lh+ls 유지(issue_1510).

## 검증 사다리 (수정마다)
cargo test --no-fail-fast 전량(선재 실패 = issue_258 한 타겟뿐이어야) →
svg_snapshot 골든(issue_157·form-002 조기경보) → sc- npm test 665 → qa:rhwp diff 읽기.
위험 핀: issue_1853(52쪽)·1921(42쪽)·2322·2097대·1510·1789·1772(±1px).

## 판정식 (사용자 체감)
sc- scratchpad probe-flow.mjs: 표를 위로 −25mm 올린 뒤 **앞 글자 y=132 가
표 아래로 내려가면** 한컴 동작 달성. (현재: 표[144..196]가 132 위 잉크를 5px 덮음)

## 중단 규칙
새 실패가 2타겟을 넘거나 골든이 깨지고 원인이 한 사이클 안에 안 좁혀지면
그 커밋을 되돌리고 실패 목록만 남긴다. 반쯤 넣은 상태로 세션을 끝내지 않는다.
