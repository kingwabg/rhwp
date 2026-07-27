# parity — 한컴 정합 오라클 코퍼스

## 왜 (2026-07-27 방향 전환)
조판 의미론(어울림·표 나눔·쪽 경계…)은 공개 포맷 스펙에 **없다** — 한글 실물에만 있다.
그래서 qa:rhwp(자기일관성 검증)만으로는 한컴 정합에 도달할 수 없다는 것이 실측 결론.
정답지를 갖고 싸운 싸움(1725 PDF 242쪽, TAC 쪽수 정합 66/27/35)만 빨리 끝났다.
→ **오라클-선행 원칙**: 정답지 없는 조판 수리는 하지 않는다. 없으면 "미확정 — 한컴 확인 대기".

## 축 선정 근거
sc- 제품 코드 역산(일지·공문·평가 HWP 생성 경로): 표는 전부 **글자처럼취급(tac)**,
서식 축 = fontSize·bold·textColor·align·colWidths·병합·누름틀. 이 축이 1순위 —
"한글 전체의 100%"가 아니라 "제품 문서의 100%"가 목표다.

## 구조
```
parity/
├── fixtures/   F##_축이름.hwp — 축 하나당 파일 하나, 최소 내용 (생성: examples/gen_parity_fixtures.rs)
└── oracle/     F##_축이름.pdf|png — 한컴(한컴독스/한글)이 렌더한 정답지 (사람 손 1회)
```

## 절차
1. `cargo run --release --example gen_parity_fixtures` — fixtures/ 생성
2. `cargo run --release --example smoke_parity_fixtures` — 우리 reader 재열람 스모크
3. 사람: fixtures/*.hwp 를 한컴독스(hancomdocs.com)에 업로드해 열고,
   PDF 저장(가능하면) 또는 전체 페이지 스크린샷 → oracle/ 에 같은 이름으로
4. (자동화 예정) qa:parity — 쪽수·줄바꿈·좌표를 oracle 과 대조, 래칫 게이트

## 픽스처 대장
| 파일 | 축 | 우리 엔진 판정 | 한컴 판정 |
|------|----|----------------|-----------|
| F01_tac_table_basic | tac 표 기본형(본문+3×3) | 1쪽 | 미확보 |
| F02_tac_table_pagebreak | tac 표 쪽 경계(채움 44문단+6×2) | **3쪽** | 미확보 |
| F03_cell_format | 셀 서식(20pt·굵게·빨강·가운데) | 1쪽 | 미확보 |
| F04_col_widths_merge | 열폭 2:1 + 1행 병합 | 1쪽 | 미확보 |
| F05_field_clickhere | 누름틀(name=홍길동) | 1쪽 | 미확보 |

"한컴 판정" 열은 oracle/ 확보 후 채운다 — 이 표가 곧 정합 스코어보드의 씨앗이다.
