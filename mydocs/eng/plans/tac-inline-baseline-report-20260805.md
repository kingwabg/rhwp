# 신고: 글자취급 표 혼합 줄 — 세로 기준선·빈 공간 클릭 (실측 재현 완비)

작성: sc- 세션 (2026-08-05). 03:04 pkg(= 3c97d8468 hit-test 수리 포함)로 실측.
재현: 빈 문단에 "왼쪽" 타이핑 → createTableEx(charOffset=2, 2×2, treatAsChar, colWidths [7087,7087]).

## 이미 고쳐진 것 (03:02/02:05 수리로 정상 확인)
- 수평 배치 정상: 텍스트 x113.4~139.3 → 표 x139.3~328.3 → 표 뒤 캐럿 x328.2 ✓
- 문단 논리 길이 3(텍스트2+표1) ✓

## 남은 결함 2

### ① 혼합 줄 세로 기준선 붕괴 (사용자 신고 "텍스트가 너무 위에/커서가 글자를 뒤덮음"의 원체)
- 표 bbox: y136.0 ~ 170.2 (h 34.2)
- 같은 줄 텍스트 캐럿(오프셋 0·1·3): y200.4, h13.3 — **표 바닥보다 30px 아래**
- 기대(한컴): tac 개체 바닥 ≈ 텍스트 기준선. 오라클 필요 — PrvImage 로
  "텍스트+글자취급 표" 한 줄 케이스의 y 관계 채취 권장.
- 파생 증상: 표 앞/뒤 캐럿 rect 가 h34.2(줄 전체)로 나와 "글자 하나를 덮는 블록 커서"로 보임.
  줄 메트릭이 서면 캐럿 높이도 따라 설 것.
- 파생 증상 2: getCursorRect(오프셋2) = {x139.3, y155.5, h12} vs position.cursorRect
  = {x139.3, y182, h34.2} — 두 경로가 서로 다른 y/h 를 답함.

### ② 표 오른쪽 "빈 공간" 클릭 → 표 앞 오프셋
- 클릭 지점: x = 표우측+10px, y = 표 세로 중앙(155)
- 결과: charOffset **2**(표 앞) → 이어서 타이핑하면 글자가 표 앞에 삽입
  (사용자 신고 "커서는 생기는데 입력하면 첫 번째 위치로 이동")
- 03:02 수리는 표 **위** 클릭의 앞/뒤 매핑으로 보임 — 표 오른쪽 **바깥** 빈 영역은
  여전히 앞으로 접힘. 기대: 줄 끝(오프셋 3).
- 진단 예제 diag_tac_click_type.rs 가 이 케이스를 다루는지 확인 요망.

## 근본 원인 규명 (rhwp 세션 2026-08-05, 엔진 diag 로 정밀 추적)

### 결함 ② — 이미 엔진에서 고쳐짐 (재확인 완료)
"왼쪽"+2×2 TAC 표(colWidths [7087,7087]) 정밀 재현으로 hit_test_native 직접 호출:
- 표 오른쪽 밖 클릭(right+10, right+40, y=표 세로중앙) → **charOffset 3** (표 뒤). 정상.
- 즉 3c97d8468(has_right_neighbor 밴드) 가 이 케이스를 이미 커버. 리포트의 "offset 2"는
  옛 pkg 측정치이거나, **결함 ①의 파생**(표가 30px 위에 떠서 클릭 y가 표 밴드 아래로
  빠지면 표 히트 실패 → 앞 오프셋). ①을 고치면 ②의 잔여 체감도 사라짐.

### 결함 ① — 정확한 원인 지목 (layout.rs vs paragraph_layout 배치 경쟁)
- 표는 **layout.rs:6578** 이 `table_y_start`(=문단 상단 y136)에 등록·렌더한다.
  이때 `inline_pos=None`(paragraph_layout 아직 미실행) → layout.rs 가 **먼저 선점**.
- 그 뒤 paragraph_layout emit_line_runs(5157) 는 올바른 기준선 y(=`y+baseline+om_bottom
  -table_h` ≈ 182, 텍스트 줄 상단)에 놓으려 하지만 `already_rendered=true`(layout.rs 가
  이미 등록)라서 **건너뛴다**. 결과: 표 y136(문단 상단), 텍스트 줄 y182 — 46px(한 줄) 차.
- 즉 인라인 TAC 표가 "문단 상단"에 붙고 텍스트 줄은 그 아래에 서서 세로로 어긋난다.
- 실측(diag, RHWP_DEBUG_PARA_TAC/직접 계측): 텍스트 줄 y=182.0 lh=34.2 baseline=29.1,
  paragraph_layout table_y=182(=max(176.9, 182)) vs layout.rs table_y_start=136.

### 안전한 수리 방향 (다음 세션 — 오라클 or 레이아웃 오너와)
두 경로가 **같은 기준선 y** 를 쓰게 해야 한다. 후보:
1. layout.rs 가 inline-in-para TAC(=`is_tac_table_inline_in_para` true, 같은 줄 텍스트 有)
   표를 **직접 배치하지 않고 paragraph_layout(5157)에 양보** — 단 y_offset 흐름 진행이
   layout.rs 표 블록에 깊게 얽혀(6590~, 자리차지·어울림·voff 분기 다수) 회귀 위험 큼.
2. layout.rs:6578 에서 inline TAC 의 `table_y_start` 를 **텍스트 줄 기준선**으로 보정.
   단 layout.rs 에 첫 줄 baseline 오프셋(182 vs 문단 상단 136, +46)이 바로 없어 line
   메트릭 재계산 필요.
- ⚠ TAC 테스트 다수(issue_1070/1071/1285 …)와 64쪽 핀 등 회귀 이력 밀집 구역 —
  블라인드 편집 금지. 한컴 오라클로 "텍스트+TAC표 한 줄"의 정확한 y 관계(표 바닥 =
  텍스트 기준선? 표 상단 = 줄 상단?) 확정 후 위 1/2 중 택해 surgical 수정.

## 한컴 오라클 채취 완료 (rhwp 세션 2026-08-05, 웹한글 실측)

재현: "왼쪽" 입력 → 글자취급 2×2 표(임의폭 30mm) 삽입 → 표 뒤에 "오른쪽" 입력.
웹한글 렌더 픽셀 실측(zoom):
- **표 뒤 텍스트("오른쪽")의 기준선 = 표 바닥에 정확히 정렬** ← 핵심 오라클.
  → 리포트 가설 "tac 개체 바닥 ≈ 텍스트 기준선" **확증**.
- **표 앞 텍스트("왼쪽")는 표 위 줄**에 있고, 표는 그 **아래 줄**에 놓인다.
  → 한컴은 이 TAC 표를 앞 텍스트 **옆(같은 x줄)이 아니라 다음 줄로 내린다**.
  (표는 자기 높이만큼 줄을 차지하는 "큰 글자" — 앞 텍스트는 표 상단 줄, 뒤 텍스트는
   표 하단 줄에 baseline 정렬.)

### rhwp 현재 동작과의 대조 (버그 확정)
- rhwp: "왼쪽" x113(좌)·표 x139(우)로 **가로로 나란히**, 그런데 표 y136(위)·텍스트
  y182(아래)로 세로 어긋남. = "옆에 붙이려다 만" 반쪽 인라인.
- 한컴: "왼쪽"은 표 **위 줄**, 표는 아래 줄. 가로로 안 붙임.
→ 즉 end-anchored(앞에만 텍스트) TAC 표는 **자기 줄로 내려가야** 한다.
  is_tac_table_inline_in_para 가 이 케이스에 true(is_tac_table_inline: width<0.9seg)를
  주는 게 오판. has_middle_anchor(양옆 텍스트)만 진짜 인라인, end-anchor 는 블록.

### 오라클 기반 수리 방향 (surgical, 다음 세션 greenlight 시)
- **height_measurer::is_tac_table_inline_in_para**: end-anchored(text 앞 有·뒤 無) TAC 표는
  false(블록·자기 줄) 반환하도록. middle-anchor(양옆 텍스트)만 inline 유지.
  → 그러면 layout.rs 블록 경로가 표를 앞 텍스트 **다음 줄**에 놓아 한컴과 일치, 세로
    어긋남(136 vs 182) 소멸. hit_test 밴드도 표가 제 줄에 서므로 자동 정합.
- ⚠ 회귀 검증 필수: is_tac_table_inline 이 true 를 주던 기존 인라인 케이스(양옆 텍스트,
  다중 TAC 표 등)와 issue_1070/1071/1285 핀, 64쪽 핀. 변경은 "end-anchor 판정" 한 줄
  추가로 최소화.

### ⛔ 막다른 길 (rhwp 세션 2026-08-05 실측 — 반복 금지)
is_tac_table_inline_in_para 를 end-anchor 에서 false 로 바꿔봤으나 **레이아웃 불변**
(표 y136·텍스트 y182 그대로). 이유: 표의 실제 배치는 **layout.rs:6578** 이 `table_y_start`
(=문단 상단 136)에 놓는 것이라, 게이트(인라인/블록 판정)만 바꿔선 y 가 안 움직인다.
게다가 블록으로 봐도 rhwp 는 표를 문단 **상단**(136)에, 앞 텍스트("왼쪽")를 그 **아래**
(182)에 두어 **한컴과 상하 반전**이다(한컴: 텍스트 위·표 아래).
→ 진짜 수리 지점: layout.rs 가 "문단 안에서 텍스트 **뒤**(charOffset 큰) 위치의 TAC 표"를
  앞 텍스트 줄 **다음**에 놓도록 `table_y_start` 를 그 텍스트 줄 끝(bottom)으로 잡아야 한다.
  현재는 para 상단 flow anchor 를 무조건 써서 순서를 무시한다. 이건 게이트가 아니라
  layout.rs 표 배치 y 계산의 문단내 순서 반영 문제 — 규모 있음.

## 완전한 인과 사슬 종결 (rhwp 세션 2026-08-05, composer 까지 추적)
"왼쪽"+2×2 TAC 표(작은 폭)의 line_seg 를 debug_line_seg_tags 로 확인: **1개** (텍스트와
표가 **한 줄**로 묶임). 사슬 전체:
1. **composer**(line_breaking.rs `inline_control_line_height_hwp`/`apply_inline_control_line_height`):
   TAC 표가 든 줄의 **높이만 키우고 같은 줄 유지** — 표를 자기 줄로 쪼개는 로직이 없다.
   작은 TAC 표(width<seg)는 항상 1 line_seg 로 인라인.
2. **typeset.rs:13410 pre_table_end_line**: total_lines==1 이라 0 → 표 앞 텍스트
   PartialParagraph 미생성, Table item 만 push.
3. **layout.rs:6578**: Table item 을 para 상단(y136)에 배치, 텍스트는 그 뒤 y182 로 밀림.
4. **한컴 오라클**: 2 줄(텍스트 위·표 아래)로 렌더.

→ **진짜 수리 지점 = composer**: end-anchored(앞 텍스트·뒤 없음) TAC 표를 자기 line_seg
  (2번째 줄)로 줄바꿈하도록 line_breaking 생성 로직 추가. total_lines 가 2가 되면 typeset
  pre_table_end_line 이 표 줄 인덱스(1)를 찾아 텍스트 PartialParagraph 를 먼저 push →
  layout 순서·y 자동 정합, hit_test 밴드도 표가 제 줄에 서므로 자동 정합.

## ⚠️ 리스크 평가 (다음 세션 착수 전 필독)
- 이 수리는 **line_seg 생성 변경 = core**. line_seg 는 HWP/HWPX 저장 왕복·쪽분할·전
  렌더의 근간이라, 잘못 건드리면 저장 왕복·페이지네이션이 조용히 깨진다(단위 테스트가
  전부 못 잡을 수 있음).
- TAC 회귀 이력 밀집(issue_1070/1071/1285, 64쪽 핀, 복학원서 PUA 필러, sample16 pi394…).
- **권장**: 별도 집중 세션에서 (a) end-anchor 판정 정밀화(뒤에 가시 텍스트 없음), (b)
  line_breaking 에서 그 표를 새 line_seg 로 분리, (c) 저장 왕복·pagination·핀 전수 검증,
  (d) 한컴 오라클로 여러 TAC 변종(start-anchor, 다중 표, 셀 안 TAC) 교차 확인. 긴 세션
  끝에 급히 칠 변경이 아님.

## 참고
- studio 쪽 재현 스크립트는 sc- 세션이 보유(요청 시 공유). DEV 훅(__inputHandler) 기반.
- 엔진 재현: "왼쪽" insert → createTableEx(charOffset=2, 2×2, treatAsChar, [7087,7087]).
  hit_test_native / build_page_render_tree 로 y·offset 직접 계측(임시 diag, 커밋 안 함).
