#!/usr/bin/env bash
# [parity] 한컴 정답지 자동 수집 — 로그인·수작업 불필요.
#
# 원리: 한글이 저장한 .hwp 안에는 한글이 **직접 렌더한 1쪽 미리보기**(PrvImage)가 들어 있다.
# 즉 실물 코퍼스 자체가 오라클이다(실측 2026-07-27: samples/*.hwp 268개 중 250개 보유).
# 이 스크립트는 (한컴 1쪽 | 우리 1쪽) 짝을 out/ 에 만든다. 대조는 눈 또는 후속 스코어러.
#
# 사용: parity/oracle-sweep.sh [파일수] [출력디렉터리]
set -uo pipefail
cd "$(dirname "$0")/.."
LIMIT="${1:-20}"
OUT="${2:-parity/out}"
BIN=./target/release/rhwp
mkdir -p "$OUT"
[ -x "$BIN" ] || cargo build --release --bin rhwp

n=0; paired=0
for f in samples/*.hwp; do
  [ "$n" -ge "$LIMIT" ] && break
  base="$(basename "$f" .hwp)"
  # ① 한컴 정답지 — PrvImage 추출 후 PNG 로 정규화(원본은 GIF 인 경우도 있다)
  $BIN thumbnail "$f" -o "$OUT/${base}.hancom.raw" >/dev/null 2>&1 || continue
  sips -s format png "$OUT/${base}.hancom.raw" --out "$OUT/${base}.hancom.png" >/dev/null 2>&1 || continue
  rm -f "$OUT/${base}.hancom.raw"
  n=$((n+1))
  # ② 우리 렌더 — SVG 1쪽(래스터화는 compare.mjs 가 sharp 로: 종횡비 보존)
  rm -rf "$OUT/.svg-$base"
  $BIN export-svg "$f" -o "$OUT/.svg-$base" >/dev/null 2>&1 || continue
  # ⚠ 다쪽 문서는 base_001.svg, 1쪽 문서는 base.svg 로 저장된다(엔진 규약)
  svg="$OUT/.svg-$base/${base}_001.svg"
  [ -f "$svg" ] || svg="$OUT/.svg-$base/${base}.svg"
  [ -f "$svg" ] || continue
  cp "$svg" "$OUT/${base}.ours.svg"
  paired=$((paired+1))
  rm -rf "$OUT/.svg-$base"
done
echo "정답지 $n건 · 짝 완성 $paired건 → $OUT"
