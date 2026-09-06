---
kind: reference
status: active
canonical: mydocs/tech/hwp_form_object_api.md
last_verified: 2026-07-17
---

# HWP 양식 개체 API 기술 자료

## 개요

HWP 문서에 포함된 양식 개체(Form Object)의 값을 JavaScript에서 조회하고 설정할 수 있는 WASM API를 제공한다. 5종 양식 개체(명령 단추, 선택 상자, 콤보 상자, 라디오 단추, 편집 상자)를 지원한다.

## 양식 개체 타입

| 타입 | formType 문자열 | HWP 타입ID | 주요 속성 |
|------|----------------|-----------|-----------|
| 명령 단추 | `PushButton` | `tbp+` | caption |
| 선택 상자 | `CheckBox` | `tbc+` | value (0/1), caption |
| 콤보 상자 | `ComboBox` | `boc+` | text (선택값), items (항목 목록) |
| 라디오 단추 | `RadioButton` | `tbr+` | value (0/1), caption, RadioGroupName |
| 편집 상자 | `Edit` | `tde+` | text (입력값) |

## API 레퍼런스

### getFormObjectAt(pageNum, x, y)

페이지 좌표에서 양식 개체를 찾는다.

**파라미터:**
- `pageNum` (number) — 페이지 번호 (0부터 시작)
- `x` (number) — 페이지 내 X 좌표 (px, 96dpi 기준)
- `y` (number) — 페이지 내 Y 좌표 (px, 96dpi 기준)

**반환값:**
```json
{
  "found": true,
  "sec": 0,
  "para": 4,
  "ci": 0,
  "formType": "ComboBox",
  "name": "ComboBox",
  "value": 0,
  "caption": "",
  "text": "계절 선택",
  "bbox": { "x": 42.5, "y": 111.5, "w": 80.8, "h": 19.3 }
}
```

양식 개체가 없으면 `{"found": false}` 반환.

### getFormValue(sec, para, ci)

양식 개체의 현재 값을 조회한다.

**파라미터:**
- `sec` (number) — 구역 인덱스 (0부터 시작)
- `para` (number) — 문단 인덱스
- `ci` (number) — 컨트롤 인덱스

**반환값:**
```json
{
  "ok": true,
  "formType": "CheckBox",
  "name": "CheckBox",
  "value": 1,
  "text": "",
  "caption": "선택 상자",
  "enabled": true
}
```

**타입별 값 해석:**

| 타입 | value 의미 | text 의미 |
|------|-----------|-----------|
| CheckBox | 0=해제, 1=선택 | — |
| RadioButton | 0=해제, 1=선택 | — |
| ComboBox | — | 현재 선택된 항목 텍스트 |
| Edit | — | 입력된 텍스트 |
| PushButton | — | — |

### setFormValue(sec, para, ci, valueJson)

양식 개체의 값을 설정한다. 설정 후 자동으로 재조판 및 렌더 캐시가 무효화된다.

**파라미터:**
- `sec` (number) — 구역 인덱스
- `para` (number) — 문단 인덱스
- `ci` (number) — 컨트롤 인덱스
- `valueJson` (string) — JSON 문자열

**valueJson 형식:**

```javascript
// CheckBox / RadioButton: value 설정
'{"value": 1}'          // 선택
'{"value": 0}'          // 해제

// ComboBox / Edit: text 설정
'{"text": "여름"}'      // 텍스트 설정

// 복합 설정 (value + text 동시)
'{"value": 1, "text": "새값"}'

// caption 변경
'{"caption": "새 캡션"}'
```

**반환값:**
```json
{ "ok": true }
```

설정 후 `renderPageToCanvas()`를 다시 호출하면 변경된 값이 화면에 반영된다.

### getFormObjectInfo(sec, para, ci)

양식 개체의 상세 정보를 반환한다. 파서가 추출한 전체 속성과 ComboBox 항목 목록을 포함한다.

**파라미터:**
- `sec` (number) — 구역 인덱스
- `para` (number) — 문단 인덱스
- `ci` (number) — 컨트롤 인덱스

**반환값:**
```json
{
  "ok": true,
  "formType": "ComboBox",
  "name": "ComboBox",
  "value": 0,
  "text": "계절 선택",
  "caption": "",
  "enabled": true,
  "width": 6058,
  "height": 1450,
  "foreColor": 0,
  "backColor": 15790320,
  "properties": {
    "TabOrder": "3",
    "ListBoxRows": "4",
    "EditEnable": "1",
    "GroupName": "",
    "BorderType": "5"
  },
  "items": ["봄", "여름", "가을", "겨울"]
}
```

**참고:**
- `width`, `height`는 HWPUNIT 단위 (1인치 = 7200 HWPUNIT)
- `foreColor`, `backColor`는 BGR 24비트 정수 (0x00BBGGRR)
- `items`는 ComboBox 전용 — HWP 스크립트의 `InsertString()` 호출에서 추출
- `properties`는 HWP 속성 문자열에서 파싱된 기타 속성 원본

## 사용 예시

### JavaScript (브라우저 콘솔)

```javascript
// WasmBridge 인스턴스 (rhwp-studio에서는 wasm 변수)

// 1. 모든 양식 개체 값 조회
for (let para = 0; para < 10; para++) {
  const result = wasm.getFormValue(0, para, 0);
  if (result.ok) {
    console.log(`${result.formType}[${result.name}]: value=${result.value}, text="${result.text}"`);
  }
}

// 2. 체크박스 토글
const cb = wasm.getFormValue(0, 2, 0);  // sec=0, para=2, ci=0
if (cb.ok && cb.formType === 'CheckBox') {
  const newVal = cb.value === 0 ? 1 : 0;
  wasm.setFormValue(0, 2, 0, JSON.stringify({ value: newVal }));
}

// 3. 콤보박스 항목 목록 조회 및 선택
const info = wasm.getFormObjectInfo(0, 4, 0);
if (info.ok && info.items) {
  console.log('항목:', info.items);  // ["봄", "여름", "가을", "겨울"]
  wasm.setFormValue(0, 4, 0, JSON.stringify({ text: info.items[2] }));  // "가을" 선택
}

// 4. 페이지 좌표로 양식 개체 찾기
const hit = wasm.getFormObjectAt(0, 100, 120);
if (hit.found) {
  console.log(`${hit.formType} "${hit.name}" at (${hit.bbox.x}, ${hit.bbox.y})`);
}
```

## ComboBox 항목 추출 원리

HWP 문서의 ComboBox 항목은 파일 바이너리에 직접 저장되지 않는다. 대신 `Scripts/DefaultJScript` OLE 스트림에 포함된 스크립트 코드에서 `InsertString()` 호출을 통해 런타임에 추가된다.

**파일 구조:**
```
HWP OLE Compound File
├── FileHeader
├── DocInfo
├── BodyText/Section0    ← 양식 개체 정의 (타입, 크기, 속성)
├── Scripts/
│   ├── DefaultJScript   ← zlib 압축 + UTF-16LE 스크립트
│   └── JScriptVersion
└── ...
```

**스크립트 예시:**
```javascript
ComboBox.ResetContent();
ComboBox.Text = "계절 선택";
ComboBox.InsertString("봄", 0);
ComboBox.InsertString("여름", 1);
ComboBox.InsertString("가을", 2);
ComboBox.InsertString("겨울", 3);
```

**추출 방식:**
1. `Scripts/DefaultJScript` 스트림을 zlib 해제 (raw deflate)
2. UTF-16LE → String 디코딩
3. `컨트롤이름.InsertString("항목", 인덱스)` 패턴을 정규식 매칭
4. 인덱스 순 정렬하여 항목 목록 구성

**제한사항:**
- 스크립트가 없는 문서에서는 항목 목록을 추출할 수 없음
- 조건부 항목 추가(`if`문 내부) 등 복잡한 스크립트 로직은 처리하지 않음
- `InsertString` 패턴 매칭만 지원 (완전한 스크립트 엔진이 아님)

## 배치 — 시각 오프셋 (2026-08-15)

양식 개체는 **앵커가 글자 사이(인라인)** 에 있고, 그 앵커로부터의 **시각 델타**로 자유
이동한다. `setFormObjectProps` 의 두 키가 정본이다.

| 키 | 단위 | 비고 |
|----|------|------|
| `horzOffset` | HWPUNIT | 음수 허용(왼쪽/위로) |
| `vertOffset` | HWPUNIT | 1 page px = 75 HWPUNIT |

저장소는 HWPX 정본 키 `PosHorzOffset` / `PosVertOffset`(`properties`)이다. 새 키를 만들지
않는다 — HWPX 파서(`hp:pos`)와 직렬화기가 이미 이 키를 왕복시키므로 새 키는 두 번째
진실이 된다.

**조판 불변이 계약이다.** 델타는 렌더 bbox에만 들어가고 폭 예약(`x += tac_w`)·줄 높이·
캐럿 칸·쪽수를 건드리지 않는다. 그래서 개체를 옆으로 옮겨도 원래 자리에 빈칸이 남는다 —
버그가 아니라 이 설계의 정의다. 이걸 없애려면 줄 폭·재래핑·쪽수가 함께 움직이므로 별건.
`Control::Form(_) => true`(인라인 술어, composer.rs·layout.rs·helpers.rs)를 열지 말 것.

**포맷 수용력과 한컴 실물은 다르다.** HWPX `<hp:pos>` 는 양식이든 그림이든 속성 집합이
같고(11속성 동일) HWP5 양식 CTRL_HEADER 도 46바이트 개체 공통 속성 그 자체라 포맷은 부동
배치를 담을 수 있다. 그러나 **한컴 실물은 전수 인라인**이다(샘플 실측: HWPX 190/190,
HWP 214/214 가 `treatAsChar=1` + 오프셋 0). 즉 비0 오프셋은 한컴 미채취 영역이고,
한컴에서 열면 오프셋을 무시해 원래 자리로 보일 수 있다.

**저장 매체 한계.** HWPX 는 왕복 보존된다. HWP5 직렬화기는 아직 오프셋을 쓰지 않으므로
`.hwp` 로 저장하면 위치가 0으로 돌아간다(신고 시 조각 C 로 처리).

**표 셀 안 양식은 제외**다. 히트 결과의 `(para, ci)` 쌍이 호스트 문단 기준이라 그대로
setter 에 넣으면 엉뚱한 컨트롤을 건드린다. 스튜디오가 `inCell` 을 보고 자유 이동을 막고
기존 글자 사이 이동만 허용한다.

핀: `tests/officex_form_free_move.rs` — 양·음 이동 / 조판 불변 / 오프셋 0 렌더 동일.

## 파일 내 위치 참조

| 구분 | 파일 경로 |
|------|-----------|
| 모델 | `src/model/control.rs` — FormType, FormObject |
| 파서 | `src/parser/control.rs` — parse_form_control, parse_form_properties |
| WASM API | `src/wasm_api.rs` — getFormObjectAt, getFormValue, setFormValue, getFormObjectInfo |
| 네이티브 구현 | `src/document_core/queries/form_query.rs` |
| 렌더 트리 | `src/renderer/render_tree.rs` — FormObjectNode |
| 레이아웃 | `src/renderer/layout/paragraph_layout.rs` — 인라인 배치, `form_visual_delta_px` |
| SVG 렌더링 | `src/renderer/svg.rs` — render_form_object |
| Canvas 렌더링 | `src/renderer/web_canvas.rs` — render_form_object |
| TS 인터페이스 | `rhwp-studio/src/core/types.ts` — FormObjectHitResult, FormValueResult, FormObjectInfoResult |
| TS Bridge | `rhwp-studio/src/core/wasm-bridge.ts` — getFormObjectAt 등 래퍼 |
| 클릭 처리 | `rhwp-studio/src/engine/input-handler.ts` — handleFormObjectClick, `commitFormFreeMove` |
| CSS | `rhwp-studio/src/styles/form-overlay.css` |
