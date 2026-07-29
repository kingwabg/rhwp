# 한컴독스 클립보드 상호운용 관문 — Stage 0 정찰 결과 (2026-07-30 실측)

목표: studio에서 만든 텍스트·표·그림·도형·글상자·차트를 한컴독스(webhwp.hancomdocs.com)에
복사-붙여넣기 했을 때 동일하게 나오는 것. 이 문서는 실측 정찰의 정본 기록.

## 실측 방법
Chrome 확장으로 한컴독스 웹 한글 편집기(새 한글(7).hwpx)에 서식 텍스트·2×2 표·직사각형
도형을 만들고 ①복사 페이로드 덤프(브라우저 API + macOS 네이티브 클립보드)
②우리식 HTML 붙여넣기 ③image/png 붙여넣기를 수행. 원본 덤프:
sc- 세션 스크래치 `hancom-copy.html`(22,606B) · `hancom-hwpjson.json`(17,900B 정형화).

## 발견 1 — 채널: 한컴도 text/plain + text/html 2종뿐
- `navigator.clipboard.read()` 기준 커스텀 웹 포맷 없음. macOS 네이티브 클립보드에도
  «class HTML»·utf8/16 텍스트뿐 — **전용 네이티브 포맷 없음**.
- 우리(studio)와 동일 구조: text/plain + text/html (+그림일 때 image/png).

## 발견 2 — 진짜 페이로드는 HTML 주석 속 hwpjson (핵심)
복사 HTML 구조:
```
<div id='hwpEditorBoardContent' contenteditable='true'
     data-hjsonver='2.0' data-jsonlen='18158'><!--[data-hwpjson]{...전체 문서모델 JSON...}-->
  <p>…겉보기용 일반 HTML(P/SPAN/TABLE, 인라인 CSS)…</p>
```
- **`<!--[data-hwpjson]{…}-->` 주석에 문서모델 전문**이 실린다. 표·도형은 이 JSON에만
  존재(도형은 HTML 태그로 안 나감 — img/svg 0개).
- `navigator.clipboard.read()`는 주석을 정제(sanitize)해서 지우므로 브라우저 API로는 안
  보인다. **copy 이벤트 `e.clipboardData.setData()` 경로는 비정제** — 우리 onCopy도 이
  경로라서 같은 주석을 실을 수 있다.
- JSON 스키마: HWPX의 압축 키 판. 최상위 `documentPr·dh·ro(문단 runs)·sl(셀 문단)·
  cs(글자모양)·bf(테두리채움)·pp(문단모양)·st·bi(바이너리)…`. 컨트롤은 fourcc로 참조 —
  `tbl `(표)·`gso `(그리기개체)·`secd`(구역). 그림 `bi`는 base64가 아니라 **한컴 서버
  리소스 파일명 참조**(`00…B6.png`) — 외부로는 그림이 JSON에 실려 나가지 않는다.

## 발견 3 — 외부 HTML 붙여넣기 수용성 (우리 clipboard.rs 출력 형태로 실측)
| 콘텐츠 | 결과 |
|---|---|
| 서식 스팬(굵게·색·크기) | ✅ 굵은 빨강 유지. 폰트는 없는 글꼴이면 함초롬돋움 폴백 |
| `<table>`+인라인 테두리 CSS | ✅ 실제 표로 삽입 |
| `<img src="data:…base64">` | ❌ 깨진 자리표시(외부/데이터 URI 로드 거부) |
| image/png 클립보드 항목 단독 | ✅ 그림 정상 삽입(서버 업로드됨) |

## 결론 — 관문 전략
1. ~~1단계(텍스트+표)는 현행 HTML로 이미 대부분 통과~~ **정정(2026-07-30 실복사 실측)**:
   위 판단은 손제작 샘플 HTML 기준이었고, **실제 전체선택 복사는 337B — 첫 문단만 실린다.**
   `export_selection_html`이 범위 내 표 컨트롤·후속 문단을 떨군다(문단 0~2 요청 → `<p>` 1개,
   table 0개, 217B). `table_to_html`은 개체 단독선택 경로에서만 불림. **P1 = 범위 복사에
   컨트롤 직렬화 편입.** 한컴의 수용성 자체는 실측 검증됨(우리식 HTML 표 붙여넣기 ✅).
   부수 발견: 한컴→우리 붙여넣기는 표·굵게·색까지 이미 수신 성공(우리 HTML 파서 우수) ·
   더블클릭 단어선택+Cmd+B 미적용 결함 발견(별도 수리). 비교 리포트 아티팩트로 박제.
2. **그림은 HTML에 실으면 안 되고 image/png 항목으로**: 혼합 콘텐츠(텍스트+그림) 복사
   시 그림이 유실되는 것이 브라우저 클립보드의 구조적 한계(ClipboardItem은 문서 흐름 내
   위치를 못 실음). 완전 해법은 3의 hwpjson.
3. **도형·글상자·완전 충실도의 유일한 통로 = hwpjson 방언 출력**: 우리 copy에서
   `data-hjsonver='2.0'` 래퍼 + `[data-hwpjson]` 주석을 함께 실으면 한컴이 전량 소비.
   역방향(한컴→우리)도 같은 주석 파싱으로 표·도형까지 수신 가능. 스키마 역공학이 본작업.
4. 그림의 hwpjson 왕복은 `bi` 서버 리소스 참조 때문에 외부에선 불성립 — 그림만은
   image/png 채널 병행이 정답.

## 미규명 (재확인 필요)
- 한컴 자기 복사본을 합성 cmd+V로 재붙여넣기하면 표가 4칸·글리프 깨짐으로 관측됨(2회).
  합성 키 입력 아티팩트인지 hwpjson 소비 실패인지 미규명 — 실기기 수동 재현으로 판정할 것.
- `data-jsonlen`(18158) vs 주석 실측(17900) 차이 — 인코딩 단위(utf8 바이트 vs utf16) 추정.

## 실행 결과 — 1단계 수리 (2026-07-30, 같은 날 완료)
실조작 재실측(우리 에디터 실복사)에서 결함 4종 발견·수리·검증:
1. **범위 복사 표 유실** — `export_selection_html_native`에 범위 안 컨트롤 직렬화 편입
   (경계 문단은 앵커 문자 위치로 포함 판정). 회귀: `test_export_selection_html_includes_table_in_range`.
2. **표 무테두리 복사** — `border_fill_id` 1-기반을 0-기반 배열에 그대로 인덱싱(off-by-one).
   렌더러(layout.rs)와 동일한 -1 보정. 같은 테스트에 `border-top:` 단언.
3. **더블클릭 단어선택 부재**(studio) — `CursorState.selectWordAtCursor` 신설(본문·셀),
   onDblClick 최종 폴백 배선. 이게 없어 더블클릭+Ctrl+B가 대기 서식으로 빠졌다.
4. **Ctrl+A 옛 anchor 승계**(studio) — `selectWholeDoc`이 `clearSelection` 선행.
   선택이 남은 상태의 전체선택-복사에서 문서 앞부분이 유실됐다.
판정: 실복사 337B→1,672B(본문·굵게·표·테두리4셀), 한컴 붙여넣기에 표+균일 테두리+굵게 도달
(스크린샷 아티팩트 박제). e2e `dblclick-word-select.test.mjs` 5단언 headless PASS.
메뉴 전수 대조 갭 지도 = 아티팩트 "기능 갭 지도" · 남은 부재: 차트·하이퍼링크(자리)·주석(자리)·
문단띠(자리)·웹동영상·PDF내보내기·빠른교정·변경내용 목록 패널(v2b).

## 다음 단계(사용자 합의된 단계별 관문)
1. 텍스트+표 HTML 목측 대조(사실상 완료 수준) → 2. 그림 image/png 경로 정비 →
3. 도형·글상자 = hwpjson 역공학(gso 스키마) → 4. 차트(생성 UI 자체가 미구현) →
최종. 5종 문서 전체 복사-붙여넣기 시각 대조.
