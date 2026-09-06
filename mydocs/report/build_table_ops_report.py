#!/usr/bin/env python3
"""표 조작 되는/안 되는 상황 통합 보고서 생성 (실측 JSON + SVG 도식 + 화면 캡처)."""
import json
import os

BASE = os.path.dirname(os.path.abspath(__file__))
A = 'assets/table-ops-20260817'

# ── SVG 도식 ─────────────────────────────────────────────
W, H = 240, 132
X0, X1, X2, X3 = 16, 90, 164, 224
Y0, Y1, Y2, Y3 = 26, 52, 78, 104
GRAY, RED, GREEN, AMBER = '#8a929b', '#c0392b', '#1f7a4d', '#a15c00'


def cell(x, y, w, h, fill='#fff'):
    return f'<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="{fill}" stroke="{GRAY}" stroke-width="1.5"/>'


def grid3():
    s = ''
    for x, w in ((X0, X1 - X0), (X1, X2 - X1), (X2, X3 - X2)):
        for y, h in ((Y0, Y1 - Y0), (Y1, Y2 - Y1), (Y2, Y3 - Y2)):
            s += cell(x, y, w, h)
    return s


def line(x1, y1, x2, y2, color, wid=4):
    return f'<line x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" stroke="{color}" stroke-width="{wid}" stroke-linecap="round"/>'


def mark(cx, cy, kind='x'):
    color = {'x': RED, 'v': GREEN, '!': AMBER}[kind]
    s = f'<circle cx="{cx}" cy="{cy}" r="9" fill="#fff" stroke="{color}" stroke-width="2.4"/>'
    if kind == 'x':
        s += f'<line x1="{cx-4}" y1="{cy-4}" x2="{cx+4}" y2="{cy+4}" stroke="{color}" stroke-width="2.6" stroke-linecap="round"/>'
        s += f'<line x1="{cx+4}" y1="{cy-4}" x2="{cx-4}" y2="{cy+4}" stroke="{color}" stroke-width="2.6" stroke-linecap="round"/>'
    elif kind == 'v':
        s += f'<polyline points="{cx-4.5},{cy} {cx-1},{cy+3.5} {cx+4.5},{cy-3.5}" fill="none" stroke="{color}" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round"/>'
    else:
        s += f'<line x1="{cx}" y1="{cy-4.5}" x2="{cx}" y2="{cy+1}" stroke="{color}" stroke-width="2.6" stroke-linecap="round"/>'
        s += f'<circle cx="{cx}" cy="{cy+4}" r="1.4" fill="{color}"/>'
    return s


_aid = [0]


def arrow(x1, y1, x2, y2, color=RED, dash=''):
    _aid[0] += 1
    i = _aid[0]
    d = f' stroke-dasharray="{dash}"' if dash else ''
    return (f'<defs><marker id="m{i}" markerWidth="7" markerHeight="7" refX="6" refY="3.5" orient="auto">'
            f'<path d="M0,0 L7,3.5 L0,7 z" fill="{color}"/></marker></defs>'
            f'<line x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" stroke="{color}" stroke-width="2.2"{d} marker-end="url(#m{i})"/>')


def text(x, y, t, color='#5b6570', size=11, weight='normal'):
    return (f'<text x="{x}" y="{y}" font-size="{size}" fill="{color}" text-anchor="middle" font-weight="{weight}" '
            f'font-family="-apple-system,BlinkMacSystemFont,sans-serif">{t}</text>')


def svg(body):
    return f'<svg viewBox="0 0 {W} {H}" xmlns="http://www.w3.org/2000/svg">{body}</svg>'


def fig_stagger_ok():
    s = ''
    for y, h in ((Y0, Y1 - Y0 + 10), (Y1 + 10, Y2 - Y1 - 10), (Y2, Y3 - Y2)):
        s += cell(X0, y, X1 - X0, h)
    for x, w in ((X1, X2 - X1), (X2, X3 - X2)):
        for y, h in ((Y0, Y1 - Y0), (Y1, Y2 - Y1), (Y2, Y3 - Y2)):
            s += cell(x, y, w, h)
    s += line(X0, Y1 + 10, X1, Y1 + 10, GREEN) + mark((X0 + X1) / 2, Y1 + 10, 'v')
    s += arrow(X0 + 12, Y1 - 4, X0 + 12, Y1 + 6, GREEN)
    s += text(W / 2, 16, '한 칸 경계만 내려감', GREEN, 11.5, '600')
    s += text(W / 2, 126, '표 크기 불변', '#5b6570', 10.5)
    return svg(s)


def fig_outer():
    s = grid3()
    s += line(X0, Y3, X3, Y3, RED) + line(X3, Y0, X3, Y3, RED)
    s += mark((X0 + X3) / 2, Y3) + mark(X3, (Y0 + Y3) / 2)
    s += text(W / 2, 16, '바깥 테두리 = 대상 아님', RED, 11.5, '600')
    s += text(W / 2, 126, '잡으면 표 개체 선택', '#5b6570', 10.5)
    return svg(s)


def fig_merged_block():
    s = ''
    for y, h in ((Y0, Y1 - Y0), (Y1, Y2 - Y1), (Y2, Y3 - Y2)):
        s += cell(X0, y, X1 - X0, h)
    s += cell(X1, Y0, X2 - X1, Y2 - Y0, '#eef1f4') + cell(X1, Y2, X2 - X1, Y3 - Y2)
    for y, h in ((Y0, Y1 - Y0), (Y1, Y2 - Y1), (Y2, Y3 - Y2)):
        s += cell(X2, y, X3 - X2, h)
    s += line(X1, Y0, X1, Y2, RED) + mark(X1, (Y0 + Y2) / 2)
    s += text((X1 + X2) / 2, (Y0 + Y2) / 2 + 4, '병합', '#5b6570', 10.5)
    s += text(W / 2, 16, '마주 보는 칸 크기가 다름', RED, 11.5, '600')
    s += text(W / 2, 126, '양쪽이 같게 병합됐으면 가능', '#5b6570', 10.5)
    return svg(s)


def fig_floor():
    s = grid3()
    s += text((X0 + X1) / 2, Y1 + 18, '내용', '#16191d', 11)
    s += arrow(X0 + 14, Y1 + 6, X0 + 14, Y2 - 6)
    s += line(X0, Y2, X1, Y2, RED) + mark((X0 + X1) / 2, Y2)
    s += text(W / 2, 16, '글줄보다 얇게 못 만듦', RED, 11.5, '600')
    s += text(W / 2, 126, '빈 칸은 2.7px 까지 가능', '#5b6570', 10.5)
    return svg(s)


def fig_resize_col():
    s = ''
    MX = X1 + 26
    for y, h in ((Y0, Y1 - Y0), (Y1, Y2 - Y1), (Y2, Y3 - Y2)):
        s += cell(X0, y, MX - X0, h) + cell(MX, y, X2 - MX, h) + cell(X2, y, X3 - X2, h)
    s += line(MX, Y0, MX, Y3, GREEN)
    s += arrow(MX - 24, (Y0 + Y1) / 2, MX - 4, (Y0 + Y1) / 2, GREEN)
    s += text(W / 2, 16, '줄 전체가 함께 이동', GREEN, 11.5, '600')
    s += text(W / 2, 126, '표 크기 불변, 이웃이 보상', '#5b6570', 10.5)
    return svg(s)


def fig_resize_row_blocked():
    s = grid3()
    s += line(X0, Y1, X3, Y1, AMBER)
    s += arrow((X0 + X3) / 2 - 30, Y1 - 8, (X0 + X3) / 2 - 30, Y1 + 6, AMBER, '4 3')
    s += mark((X0 + X3) / 2 + 20, Y1, '!')
    s += text(W / 2, 16, '아래 행에 여유가 없으면 무동작', AMBER, 11, '600')
    s += text(W / 2, 126, '행은 글줄 밑으로 못 줄임', '#5b6570', 10.5)
    return svg(s)


def fig_table_handle():
    s = grid3()
    for cx, cy in ((X3, (Y0 + Y3) / 2), ((X0 + X3) / 2, Y3), (X3, Y3)):
        s += f'<rect x="{cx-4}" y="{cy-4}" width="8" height="8" fill="#2f6fd0" stroke="#fff" stroke-width="1.4"/>'
    for cx, cy in ((X0, Y0), ((X0 + X3) / 2, Y0), (X3, Y0), (X0, (Y0 + Y3) / 2), (X0, Y3)):
        s += f'<circle cx="{cx}" cy="{cy}" r="4.5" fill="#c9cdd2" stroke="#fff" stroke-width="1.2"/>'
    s += arrow(X3 + 2, Y3 + 2, X3 + 14, Y3 + 12, '#2f6fd0')
    s += text(W / 2, 16, '표 크기 조절 = 오른쪽·아래·모서리', '#2f6fd0', 10.5, '600')
    s += text(W / 2, 126, '왼쪽·위 핸들은 잠김(회색)', '#5b6570', 10.5)
    return svg(s)


def fig_cell_select():
    s = ''
    for x, w in ((X0, X1 - X0), (X1, X2 - X1), (X2, X3 - X2)):
        for y, h in ((Y0, Y1 - Y0), (Y1, Y2 - Y1), (Y2, Y3 - Y2)):
            fill = '#cfe3f5' if (x == X1 and y == Y1) else '#fff'
            s += cell(x, y, w, h, fill)
    s += arrow((X1 + X2) / 2, Y2 + 4, (X1 + X2) / 2, Y3 - 6, GREEN)
    s += text(W / 2, 16, 'F5 = 칸 선택, 화살표 = 이동', GREEN, 11.5, '600')
    s += text(W / 2, 126, 'F5 두 번 = 범위 확장', '#5b6570', 10.5)
    return svg(s)


def fig_tac():
    s = grid3()
    s += f'<rect x="{X0-6}" y="{Y0-6}" width="{X3-X0+12}" height="{Y3-Y0+12}" fill="none" stroke="{RED}" stroke-width="2" stroke-dasharray="5 4"/>'
    s += arrow((X0 + X3) / 2, (Y0 + Y3) / 2, (X0 + X3) / 2 + 34, (Y0 + Y3) / 2)
    s += mark((X0 + X3) / 2 + 40, (Y0 + Y3) / 2)
    s += text(W / 2, 16, '글자처럼 취급 = 이동 불가', RED, 11.5, '600')
    s += text(W / 2, 126, '해제해야 자유 배치', '#5b6570', 10.5)
    return svg(s)


def fig_nested():
    s = grid3()
    s += cell(X1 + 6, Y1 + 5, (X2 - X1) - 12, (Y2 - Y1) - 10, '#eef1f4')
    s += f'<line x1="{X1+6}" y1="{(Y1+Y2)/2}" x2="{X2-6}" y2="{(Y1+Y2)/2}" stroke="{GRAY}" stroke-width="1.2"/>'
    s += mark(X1 + 2, Y1 + 2)
    s += text(W / 2, 16, '표 안의 표(중첩)', RED, 11.5, '600')
    s += text(W / 2, 126, '구조 편집은 바깥 표에 적용됨', '#5b6570', 10.5)
    return svg(s)


FIGS = {
    'stagger-ok': fig_stagger_ok, 'outer': fig_outer, 'merged-block': fig_merged_block,
    'floor': fig_floor, 'resize-col': fig_resize_col, 'resize-row-blocked': fig_resize_row_blocked,
    'table-handle': fig_table_handle, 'cell-select': fig_cell_select, 'tac': fig_tac, 'nested': fig_nested,
}

# ── 카드 데이터 (실측 기반) ──────────────────────────────
CARDS = [
    # (섹션, 판정, 제목, 도식, 메시지들, 설명, 해법, 캡처, 캡션)
    ('op', 'yes', '어긋내기 — 한 칸 경계만 옮기기', 'stagger-ok',
     ['마우스: Shift + 경계선 드래그', '키보드: F5 → Shift + 화살표'],
     '셀 하나의 경계선만 옮겨 다른 줄과 어긋나게 만듭니다. 위·아래·좌·우 네 방향 모두 되고, '
     '표 전체 크기는 <b>절대 바뀌지 않습니다</b>(이웃 칸에서 공간을 주고받습니다).',
     '표가 커지거나 작아졌다면 어긋내기가 아니라 다른 조작을 한 것입니다.',
     't-stagger-row', '첫 칸의 아래 경계만 내려간 상태 — 표 크기는 그대로'),
    ('op', 'yes', '경계선 조절 — 줄 전체 옮기기', 'resize-col',
     ['마우스: 그냥 경계선 드래그(Shift 없이)', '키보드: F5 → Alt + 화살표(1mm씩)'],
     '그 경계선을 쓰는 <b>줄 전체</b>가 함께 움직이고, 반대편 이웃이 그만큼 양보해 '
     '<b>표 크기는 유지</b>됩니다. 실측: 열 폭 186/186/186 → 211/162/186px, 표 폭 559px 그대로.',
     '4px 안에 다른 경계선이 있으면 착 붙습니다(흡착). Alt를 누르면 흡착 해제.',
     't-col-resize', '가운데 세로 경계를 옮긴 결과 — 열 배분만 바뀜'),
    ('op', 'yes', '표 크기 조절 — 표 전체를 키우기/줄이기', 'table-handle',
     ['마우스: 표 선택 후 오른쪽·아래·오른쪽아래 핸들', '키보드: F5 → Ctrl + 화살표'],
     '표 자체가 커지거나 작아집니다. 실측: 559×59px → 599×89px(핸들), 559×59 → 567×67px(Ctrl).'
     ' 모든 열·행이 <b>비율대로</b> 함께 늘어납니다.',
     '왼쪽·위쪽 핸들 5개는 잠겨 있습니다(회색 금지 표시). 오른쪽·아래로만 조절합니다.',
     't-table-bigger', '표 전체가 커진 상태 — 모든 칸이 함께 늘어남'),
    ('op', 'yes', '표 이동 — 위치 옮기기', 'tac',
     ['Alt + 클릭으로 표 선택 후 드래그 또는 화살표(3mm씩)'],
     '자유 배치 표만 움직입니다. 실측: 표 위치 (113,132) → (125,144).'
     ' <b>글자처럼 취급</b>으로 놓인 표는 아무리 끌어도 움직이지 않습니다(오류도 안 뜹니다).',
     '안 움직이면 표 속성에서 「글자처럼 취급」을 끄세요. 용지 밖으로는 나가지 않습니다.',
     None, None),

    ('stagger', 'no', '바깥 테두리는 어긋낼 수 없음', 'outer',
     ['바깥 테두리는 어긋낼 수 없습니다'],
     '표의 맨 아래·맨 오른쪽 선은 뺏어올 이웃이 없습니다. 1열짜리 표의 세로선, 1행짜리 표의 가로선도 '
     '전부 바깥 테두리라 같은 이유로 거부됩니다.',
     '안쪽 경계선을 쓰거나, 열·행을 하나 더 추가해 내부 경계를 만드세요.',
     't-base', '기본 3×3 표 — 안쪽 선 4개만 어긋내기 대상'),
    ('stagger', 'no', '마주 보는 칸 크기가 다를 때(병합이 걸림)', 'merged-block',
     ['위아래 높이가 다른 칸과는 경계를 어긋낼 수 없습니다', '좌우 폭이 다른 칸과는 경계를 어긋낼 수 없습니다'],
     '세로 경계는 좌우 칸 <b>높이</b>가, 가로 경계는 위아래 칸 <b>폭</b>이 같아야 합니다. '
     '한쪽이 병합돼 크기가 다르면 격자가 성립하지 않아 거부됩니다. 대상이 병합됐든 이웃이 병합됐든 같습니다.',
     '<b>양쪽이 똑같이 병합된 경우는 정상 동작</b>합니다(실측 확인). 크기를 맞추면 풀립니다.',
     't-merged-vert', '가운데 열이 세로 병합된 표 — 이 경계는 어긋낼 수 없음'),
    ('stagger', 'no', '글자가 든 칸을 글줄보다 얇게', 'floor',
     ['이웃 칸에 남는 높이가 없습니다', '대상 칸에 남는 높이가 없습니다'],
     '칸은 자기 안의 글줄보다 얇아질 수 없습니다. 빈 칸은 2.7px까지 줄어들지만, 글자가 한 줄 있으면 '
     '그 줄 높이가 바닥입니다.',
     '반대 방향으로 옮기거나, 글자 크기를 줄이거나, 행을 먼저 키우세요.',
     None, None),
    ('stagger', 'no', '같은 방향으로 계속 밀어 이웃을 다 먹었을 때', 'floor',
     ['이웃 칸에 남는 폭이 없습니다'],
     '한 방향으로 반복하면 이웃이 최소 크기(2.7px)에 닿습니다. 실측: 3×3 새 표에서 오른쪽으로 '
     '한 칸씩 밀면 <b>26번째</b>에 거부. 그때까지 표 폭은 한 번도 변하지 않았습니다.',
     '반대 방향 되돌리기는 언제나 가능합니다. 열 폭을 먼저 넓히면 더 갈 수 있습니다.',
     None, None),
    ('stagger', 'warn', '한 번에 크게 끌면 — 거부가 아니라 한계까지만', 'floor',
     ['결과: 성공(갈 수 있는 데까지 이동)'],
     '이웃보다 큰 값을 줘도 거부되지 않고 <b>한계 지점까지</b> 갑니다. "많이 끌었는데 조금만 움직였다"로 '
     '보여 실패처럼 느껴집니다. 다른 열이 이미 어긋나 있으면 그 선에서 한 번 멈추고, 한 번 더 끌면 넘어갑니다.',
     '표 크기가 그대로면 정상입니다.',
     None, None),

    ('resize', 'no', '바깥 테두리는 크기 조절로도 안 잡힘', 'outer',
     ['드래그해도 아무 변화 없음'],
     '표 외곽 네 선은 리사이즈 대상에서 아예 제외돼 있습니다(마우스 커서도 안 바뀝니다). '
     '실측: 바깥 아래·오른쪽 테두리를 끌어도 표 크기 59.3px / 559.4px 그대로.',
     '표 전체 크기는 <b>표를 선택한 뒤 핸들</b>로 조절합니다.',
     None, None),
    ('resize', 'warn', '새 표에서 행 높이 조절이 무동작인 이유', 'resize-row-blocked',
     ['드래그해도 행 높이 그대로'],
     '갓 만든 표는 모든 행이 이미 <b>글줄 바닥</b>입니다. 한 행을 늘리려면 이웃 행이 그만큼 줄어야 하는데 '
     '이웃도 바닥이라 움직일 수 없습니다. 실측: 19.8/19.8/19.8px → 변화 없음.',
     '먼저 표 전체를 키운 뒤(핸들 또는 Ctrl+화살표) 행 배분을 조절하세요. 여유가 있으면 정상 동작합니다 '
     '— 실측: 19.8/59.8 → 43.7/35.9px.',
     't-row-resize', '아래 행에 여유를 준 표 — 이 상태에서는 행 조절이 됨'),
    ('resize', 'warn', '이미 어긋난 선은 Shift 없이 끌어도 어긋내기', 'stagger-ok',
     ['표 크기 유지(어긋내기로 처리)'],
     '한 칸만의 선(어긋난 선)을 Shift 없이 잡으면 자동으로 어긋내기로 처리됩니다. 원래 정렬 위치까지 '
     '끌어다 놓으면 <b>자동으로 원상 복구</b>됩니다.',
     '어긋남을 없애려면 그 선을 원래 자리로 끌면 됩니다.',
     None, None),
    ('resize', 'no', '최소 크기 아래로는 줄지 않음', 'floor',
     ['요청보다 덜 줄어듦(조용한 클램프)'],
     '엔진 최소 셀 크기는 2.7px, 화면 조작 최소는 열 18.9px · 행 17.0px입니다. 그 아래 값은 '
     '거부가 아니라 <b>조용히 잘려서</b> 적용됩니다.',
     '더 줄이려면 표 전체를 줄이거나 열/행을 삭제하세요.',
     None, None),

    ('select', 'yes', 'F5로 칸 선택하고 화살표로 이동', 'cell-select',
     ['0,0 → 1,0 → 2,0 → (끝에서 정지)'],
     'F5로 칸을 선택하면 화살표로 칸 사이를 옮겨 다닙니다. 표 끝에서는 더 가지 않고 멈춥니다. '
     'Escape로 해제합니다. 병합된 칸은 <b>한 칸으로</b> 취급되고, 어긋낸 표의 조각 칸도 정상 순회합니다.',
     'Tab / Shift+Tab으로도 칸을 옮길 수 있고, 마지막 칸에서 Tab을 누르면 행이 추가됩니다.',
     't-cell-select', 'F5로 가운데 칸을 선택한 상태'),
    ('select', 'yes', 'F5 두 번 = 범위 확장 모드', 'cell-select',
     ['1,1 → 1,1-1,2 → 1,1-2,2'],
     'F5를 한 번 더 누르면 화살표가 <b>이동</b>이 아니라 <b>범위 확장</b>이 됩니다. '
     '마우스로 칸 위를 드래그해도 범위가 선택됩니다(실측: 9칸 전부 선택).',
     '범위를 잡은 뒤 M을 누르면 병합, 삭제하면 삭제 확인 창이 뜹니다.',
     't-range-select', 'F5 두 번 후 화살표로 넓힌 범위 선택'),
    ('select', 'no', '잠긴 칸(셀 보호)에서는 편집이 막힘', 'floor',
     ['잠긴 셀이라 고칠 수 없어요'],
     '셀 보호가 켜진 칸에 캐럿이 있으면 병합·나누기·행열 삽입삭제를 포함한 편집이 차단됩니다.',
     '오른쪽 패널 「속성 → 셀 보호」를 끄면 풀립니다.',
     None, None),

    ('struct', 'no', '병합 — 범위가 병합 칸을 가로지르면 거부', 'merged-block',
     ['셀 (1,1) span (2,1)이 병합 범위를 벗어납니다', '병합 범위 (0,0)~(5,5)가 표 크기 3×3를 초과합니다'],
     '선택 범위가 기존 병합 칸이나 어긋내기 조각을 <b>반쪽만</b> 걸치면 거부됩니다. '
     '특히 어긋낸 표에서는 화면상 네모로 보여도 조각 경계가 범위 중간을 지나가 자주 막힙니다.',
     '선택을 넓혀 조각 전체를 포함시키거나, 어긋내기를 복원한 뒤 병합하세요. '
     '<b>주의: 병합 실패는 화면에 메시지가 안 뜹니다</b>(콘솔에만 기록).',
     't-merged-horz', '가로로 병합된 표'),
    ('struct', 'no', '나누기 — 병합 안 된 칸은 나눌 수 없음', 'merged-block',
     ['병합되지 않은 셀은 나눌 수 없습니다'],
     '「셀 나누기」의 기본 동작은 병합 해제입니다. 병합되지 않은 칸을 그냥 나누려 하면 거부됩니다.',
     '나누기 대화상자에서 줄/칸 수를 지정하면(1~256) 병합 여부와 무관하게 나눌 수 있습니다.',
     None, None),
    ('struct', 'no', '마지막 남은 행·열은 삭제 불가', 'outer',
     ['최소 1행은 유지해야 합니다', '최소 1열은 유지해야 합니다'],
     '표에는 최소 1행 1열이 남아야 합니다. 여러 행을 한꺼번에 지우다 이 한계에 닿으면 '
     '<b>앞서 지운 행은 그대로 남고 되돌리기도 안 됩니다</b>.',
     '표 자체를 없애려면 표 삭제를 쓰세요.',
     None, None),
    ('struct', 'yes', '행·열 삽입은 표 크기를 지킴', 'resize-col',
     ['행 삽입: 셀 9→12, 행 3→4', '열 삽입: 전 열을 비례 축소해 표 폭 유지'],
     '행을 넣으면 표가 그만큼 세로로 커지고, 열을 넣으면 <b>기존 열들이 비례해 좁아져</b> 표 폭이 유지됩니다'
     '(안 그러면 표가 용지를 넘어갑니다). 병합이 걸친 자리에 넣으면 병합 칸이 함께 늘어납니다.',
     '한 번에 1~63개까지 넣을 수 있습니다.',
     None, None),
    ('select', 'no', '글상자 안의 표에서는 F5 칸 선택이 안 됨', 'cell-select',
     ['본문 블록 선택으로 빠짐'],
     '글상자(텍스트 상자) 안에 든 표의 칸에서는 F5가 칸 선택으로 들어가지 않습니다. '
     '표 개체 선택(Escape)은 되지만 칸 선택만 구멍입니다.',
     '글상자 밖 본문 표에서는 정상입니다. 글상자 안 표는 마우스 드래그로 범위를 잡으세요.',
     None, None),
    ('select', 'warn', 'Shift+화살표는 선택 확장이 아니라 어긋내기', 'stagger-ok',
     ['격자가 바뀜(선택 범위는 그대로)'],
     '칸 선택 상태에서 Shift+화살표를 누르면 선택이 넓어지는 게 아니라 <b>경계선이 어긋납니다</b>. '
     '많이 헷갈리는 지점입니다.',
     '범위를 넓히려면 <b>F5를 한 번 더</b> 누른 뒤 화살표를 쓰세요.',
     None, None),
    ('select', 'warn', '표 끝에서 화살표는 멈추고, Tab은 빠져나감', 'cell-select',
     ['화살표: 마지막 칸에서 정지', 'Tab: 마지막 칸에서 행 추가'],
     '칸 선택 화살표는 표 밖으로 나가지 않고 멈춥니다. 반면 Tab은 마지막 칸에서 <b>새 행을 만들고</b>, '
     'Shift+Tab은 첫 칸에서 표 밖으로 나갑니다.',
     '표를 빠져나가려면 Escape(표 개체 선택) 또는 Shift+Tab을 쓰세요.',
     None, None),
    ('struct', 'no', '표 안의 표(중첩)에서는 구조 편집이 위험', 'nested',
     ['안쪽 표가 아니라 바깥 표가 편집됨'],
     '중첩된 안쪽 표에서 병합·나누기·행열 삽입삭제를 하면 좌표는 안쪽 기준인데 실제로는 <b>바깥 표</b>에 '
     '적용됩니다. 범위를 벗어나면 거부되고, 안 벗어나면 엉뚱한 칸이 바뀝니다.',
     '안쪽 표를 고쳐야 하면 따로 떼어내 편집하세요. 글자 입력·서식은 정상 동작합니다.',
     None, None),
    ('struct', 'warn', '여러 행을 한꺼번에 지우다 중단되면 되돌리기가 안 됨', 'outer',
     ['최소 1행은 유지해야 합니다'],
     '선택한 행을 하나씩 지우다 마지막 1행 한계에 닿으면, <b>앞서 지운 행은 그대로 남고 되돌리기 항목도 '
     '생기지 않습니다</b>.',
     '표 전체를 없앨 때는 행 삭제 대신 표 삭제를 쓰세요.',
     None, None),
]

SECTIONS = [
    ('op', '1. 네 가지 조작을 먼저 구분하세요', '표에서 "안 된다"고 느끼는 대부분은 다른 조작을 하고 있었던 경우입니다. 네 조작은 목적도 결과도 다릅니다.'),
    ('stagger', '2. 어긋내기 — 되는 상황 / 안 되는 상황', '한 칸 경계만 옮기는 조작입니다. 표 크기는 절대 바뀌지 않습니다.'),
    ('resize', '3. 경계선 크기조절 — 되는 상황 / 안 되는 상황', '줄 전체를 옮기는 조작입니다. 표 크기는 유지되고 배분만 바뀝니다.'),
    ('select', '4. 칸 선택과 이동', 'F5로 칸을 선택한 뒤의 이동·확장 규칙입니다.'),
    ('struct', '5. 병합·나누기·행열 삽입삭제', '표 구조를 바꾸는 조작의 제약입니다.'),
]


def esc(s):
    return s


def card_html(c):
    _, verdict, title, fig, msgs, why, fix, shot, cap = c
    cls = {'yes': 'yes', 'no': 'no', 'warn': 'warn'}[verdict]
    badge = {'yes': '가능', 'no': '안 됨', 'warn': '주의'}[verdict]
    msg_html = ''.join(f'<div class="msg">{m}</div>' for m in msgs)
    shot_html = ''
    if shot:
        shot_html = (f'<figure class="shot"><img src="{A}/{shot}.png" alt="{cap}">'
                     f'<figcaption>{cap}</figcaption></figure>')
    return f'''<div class="card {cls}">
  <div class="fig">{FIGS[fig]()}<div class="badge {cls}">{badge}</div></div>
  <div class="body">
    <h3>{title}</h3>
    {msg_html}
    <p class="why">{why}</p>
    <div class="fix">{fix}</div>
    {shot_html}
  </div>
</div>'''


def build():
    # 실측 데이터
    eng = [json.loads(l) for l in open('/tmp/ops-matrix.json') if l.startswith('{')]
    blocked = [json.loads(l) for l in open('/tmp/blocked-cases.json') if l.startswith('{')]
    ui = []
    for path in ('/private/tmp/claude-501/-Users-king-dev-rhwp/2f83f4da-180e-453f-838d-acd7101569cc/tasks/bqd12h3xu.output',
                 '/private/tmp/claude-501/-Users-king-dev-rhwp/2f83f4da-180e-453f-838d-acd7151569cc/tasks/bsr91b71n.output',
                 '/private/tmp/claude-501/-Users-king-dev-rhwp/2f83f4da-180e-453f-838d-acd7101569cc/tasks/bsr91b71n.output'):
        if os.path.exists(path):
            ui += [json.loads(l) for l in open(path) if l.startswith('{')]

    sec_html = []
    for key, title, lead in SECTIONS:
        cards = ''.join(card_html(c) for c in CARDS if c[0] == key)
        sec_html.append(f'<h2>{title}</h2><p class="lead">{lead}</p>{cards}')

    # 엔진 실측 표
    rows = []
    for d in eng:
        ok = d['result'] == 'OK'
        tag = '<span class="tag yes">가능</span>' if ok else '<span class="tag no">거부</span>'
        res = 'OK (한계까지 이동/적용)' if ok else d['result']
        keep = []
        if not d['widthKeep']:
            keep.append('표 폭 변함')
        if not d['heightKeep']:
            keep.append('표 높이 변함')
        rows.append(f'<tr><td>{tag}</td><td>{d["cat"]}</td><td>{d["label"]}</td><td>{res}</td>'
                    f'<td>{"·".join(keep) if keep else "표 크기 불변"}</td></tr>')
    eng_rows = '\n  '.join(rows)

    urows = []
    for d in ui:
        urows.append(f'<tr><td>{d["cat"]}</td><td>{d["label"]}</td><td>{d["result"]}</td><td>{d.get("note","")}</td></tr>')
    ui_rows = '\n  '.join(urows)

    html = TEMPLATE.replace('__SECTIONS__', '\n'.join(sec_html))
    html = html.replace('__ENG_ROWS__', eng_rows).replace('__UI_ROWS__', ui_rows)
    html = html.replace('__ENG_N__', str(len(eng) + len(blocked))).replace('__UI_N__', str(len(ui)))
    out = os.path.join(BASE, 'table_ops_20260817.html')
    open(out, 'w').write(html)
    print('built', out, len(html), 'bytes')


TEMPLATE = '''<!DOCTYPE html>
<html lang="ko">
<head>
<meta charset="utf-8">
<title>표 조작 — 되는 상황 / 안 되는 상황 (2026-08-17 실측)</title>
<style>
  :root { --ink:#16191d; --dim:#5b6570; --line:#dfe3e8; --bg:#f6f7f9;
    --no:#c0392b; --no-bg:#fdf2f1; --yes:#1f7a4d; --yes-bg:#f0f9f4; --warn:#a15c00; --warn-bg:#fdf6ea; }
  * { box-sizing:border-box; }
  body { margin:0; padding:40px 28px 80px; background:var(--bg); color:var(--ink);
    font:15px/1.75 -apple-system,BlinkMacSystemFont,"Apple SD Gothic Neo","Malgun Gothic",sans-serif; }
  .wrap { max-width:1120px; margin:0 auto; }
  h1 { font-size:29px; margin:0 0 10px; letter-spacing:-0.5px; }
  .sub { color:var(--dim); font-size:14px; margin:0 0 26px; }
  h2 { font-size:20px; margin:44px 0 6px; padding-bottom:8px; border-bottom:2px solid var(--ink); }
  .lead { color:var(--dim); margin:0 0 18px; font-size:14px; }
  .keybox { background:#fff; border:1px solid var(--line); border-radius:10px; padding:16px 18px; margin-bottom:8px; }
  .keybox table { width:100%; border-collapse:collapse; font-size:14px; }
  .keybox th, .keybox td { padding:8px 10px; border-bottom:1px solid var(--line); text-align:left; vertical-align:top; }
  .keybox th { background:#eef1f4; font-weight:600; }
  .keybox tr:last-child td { border-bottom:none; }
  kbd { background:#eef1f4; border:1px solid #cfd5db; border-bottom-width:2px; border-radius:4px;
        padding:1px 6px; font-size:12.5px; font-family:ui-monospace,Menlo,monospace; }
  .card { background:#fff; border:1px solid var(--line); border-radius:10px; padding:18px 20px;
    margin-bottom:14px; display:grid; grid-template-columns:250px 1fr; gap:20px; align-items:start; }
  .card.no { border-left:4px solid var(--no); } .card.yes { border-left:4px solid var(--yes); }
  .card.warn { border-left:4px solid var(--warn); }
  .fig { position:relative; background:#fff; border:1px solid var(--line); border-radius:6px; padding:6px; }
  .fig svg { display:block; width:100%; height:auto; }
  .badge { position:absolute; top:8px; right:8px; font-size:11.5px; padding:2px 8px; border-radius:20px; font-weight:600; }
  .badge.no { background:var(--no-bg); color:var(--no); } .badge.yes { background:var(--yes-bg); color:var(--yes); }
  .badge.warn { background:var(--warn-bg); color:var(--warn); }
  .body h3 { margin:0 0 9px; font-size:17px; }
  .msg { display:inline-block; font-family:ui-monospace,Menlo,monospace; font-size:12.5px;
    padding:5px 10px; border-radius:5px; margin:0 6px 8px 0; }
  .no .msg { background:var(--no-bg); color:var(--no); border:1px solid #f2c9c4; }
  .yes .msg { background:var(--yes-bg); color:var(--yes); border:1px solid #bfe3ce; }
  .warn .msg { background:var(--warn-bg); color:var(--warn); border:1px solid #ecd9b0; }
  .why { margin:2px 0 10px; }
  .fix { background:var(--bg); border-radius:6px; padding:10px 12px; font-size:14px; }
  .shot { margin:12px 0 0; }
  .shot img { max-width:100%; border:1px solid var(--line); border-radius:6px; display:block; }
  .shot figcaption { color:var(--dim); font-size:12.5px; margin-top:5px; }
  table.sum { width:100%; border-collapse:collapse; background:#fff; border:1px solid var(--line);
    border-radius:8px; overflow:hidden; font-size:13.5px; }
  table.sum th, table.sum td { padding:8px 11px; text-align:left; border-bottom:1px solid var(--line); vertical-align:top; }
  table.sum th { background:#eef1f4; font-weight:600; }
  table.sum tr:last-child td { border-bottom:none; }
  .tag { font-size:12px; padding:2px 8px; border-radius:20px; white-space:nowrap; }
  .tag.no { background:var(--no-bg); color:var(--no); } .tag.yes { background:var(--yes-bg); color:var(--yes); }
  footer { margin-top:44px; color:var(--dim); font-size:13px; border-top:1px solid var(--line); padding-top:16px; }
</style>
</head>
<body><div class="wrap">
<h1>표 조작 — 되는 상황 / 안 되는 상황</h1>
<p class="sub">2026-08-17 실측 · 엔진 프로브 __ENG_N__케이스 + 화면 조작 __UI_N__케이스를 실제로 실행해
엔진이 돌려준 메시지와 표 크기 변화를 그대로 옮겼습니다. 그림은 도식, 사진은 실제 편집기 화면입니다.</p>

<div class="keybox">
<table>
  <tr><th style="width:150px">조작</th><th style="width:290px">방법</th><th style="width:130px">표 크기</th><th>무엇이 바뀌나</th></tr>
  <tr><td><b>어긋내기</b></td><td><kbd>Shift</kbd> + 경계선 드래그 / <kbd>F5</kbd> → <kbd>Shift</kbd>+화살표</td>
      <td>불변</td><td>칸 <b>하나</b>의 경계만 이동 — 줄이 어긋남</td></tr>
  <tr><td><b>경계선 조절</b></td><td>경계선 그냥 드래그 / <kbd>F5</kbd> → <kbd>Alt</kbd>+화살표</td>
      <td>불변</td><td>그 경계를 쓰는 <b>줄 전체</b> 이동 — 배분만 바뀜</td></tr>
  <tr><td><b>표 크기 조절</b></td><td>표 선택 후 핸들(오른쪽·아래·모서리) / <kbd>F5</kbd> → <kbd>Ctrl</kbd>+화살표</td>
      <td><b>변함</b></td><td>표 전체가 비율대로 커지거나 작아짐</td></tr>
  <tr><td><b>표 이동</b></td><td><kbd>Alt</kbd>+클릭으로 표 선택 후 드래그/화살표</td>
      <td>불변</td><td>표의 <b>위치</b>만 이동(글자처럼 취급이면 불가)</td></tr>
</table>
</div>

__SECTIONS__

<h2>6. 엔진 실측 전체 결과</h2>
<p class="lead">각 케이스마다 새 표를 만들어 실제로 실행했습니다. "표 크기" 칸은 조작 후 표 폭·높이가 유지됐는지입니다.</p>
<table class="sum">
  <tr><th style="width:66px">판정</th><th style="width:100px">분류</th><th style="width:300px">상황</th><th>엔진 응답</th><th style="width:130px">표 크기</th></tr>
  __ENG_ROWS__
</table>

<h2>7. 화면 조작 실측 결과</h2>
<p class="lead">실제 편집기에서 마우스·키보드를 그대로 흉내내 측정했습니다.</p>
<table class="sum">
  <tr><th style="width:110px">분류</th><th style="width:290px">조작</th><th>결과</th><th style="width:250px">비고</th></tr>
  __UI_ROWS__
</table>

<footer>
최소 크기: 엔진 200 HWPUNIT ≈ 2.7px, 화면 조작은 열 18.9px · 행 17.0px.
어긋내기·경계선 조절·표 이동은 표 크기를 바꾸지 않습니다 — 표를 키우려면 표 크기 조절(핸들 또는 Ctrl+화살표)을 쓰세요.
</footer>
</div></body></html>'''

if __name__ == '__main__':
    build()
