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

---

## 실행 결과 (2026-07-26 — 배선 완료, 설계 3곳 변경)

실측(diag_wrap3 + TAC_CURSOR)이 계획의 전제 하나를 뒤집었다: **createTable 은 문단을
분할**해 표를 빈 host 문단(pi=1)에 앵커한다. 판정식의 "앞 글자"는 앞 문단(pi=0) —
계획 1번의 "para_start_y 확정 시점(해당 문단 도달 시) 등록"으로는 이미 배치된 앞
문단을 밀 수 없다(역방향). 이에 따른 변경:

1. **컬럼 시작 선등록**: PendingFloatBand 에 `flow_top`(단 흐름 좌표) 추가,
   build_single_column 이 단 시작 시점에 `col_anchor_y + flow_top − start_height` 로
   절대 y 를 추정해 등록. 같은 값으로 `para_start_y` 를 **사전 시드** — 표가 밀린
   텍스트를 따라 내려가는 순환을 끊는다(한컴의 동결 semantics, 계약 2로 핀).
2. **양수 오프셋 생산 제거**: 2/3 이 실은 양수 밴드는 소비자 0 인 추측 코드였고,
   양수는 self-registration(드로잉 진실)이 전담한다. 생산은 **음수(위로)만** —
   visible 분기 + 빈 host 단독 float(final else, 판정식 경로) 2곳.
3. **줄 단위 교체는 인라인**: stack_lines_through_bands 결과 일괄 대입 대신 줄 루프
   안에서 skip_float_bands 1회분(같은 수식)을 적용. 밴드 운반은 LayoutEngine
   `current_flow_bands` 인테리어 셀(계획대로 인자 추가 없음). 재생(vpos replay)
   문단·셀 내부는 제외(계획대로).

typeset 예산 짝맞춤은 full-fit 경로에 `band_stack_extra`(같은 계산부, #1789/#1510
프로브 규약)로 반영 — **분할 경로와 역방향(밴드보다 앞에 배치된 문단) 예산은 미반영**
(한계: 역방향 미스는 실측 ≈ 밴드 정렬 낭비 1줄 이내, 분할 경로는 페이지 재조판이 흡수).

검증: 판정식 통과(표[144.4..195.7] 동결 + 앞글자 132.3→199.5) · 중간 교차(−10mm)에서
줄 단위 회피 실증(앞줄 부동·교차 줄만 하강) · 위험 핀 7타겟 20건 green ·
계약 핀 신설 `tests/officex_wrap3_upward_float.rs`.
