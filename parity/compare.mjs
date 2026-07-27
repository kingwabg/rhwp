// [parity] 한컴 정답지 vs 우리 렌더 — 줄 띠(ink band) 대조 스코어러.
//
// 왜 픽셀 동일성이 아니라 줄 띠인가: 폰트 힌팅·안티에일리어싱은 절대 같을 수 없다.
// 조판 정합의 본질은 "글줄이 어디서 시작·끝나고 몇 줄인가" 이므로, 각 행의 잉크량을
// 세로 프로파일로 만들어 **띠(줄) 위치**를 비교한다. 좌우 여백은 가로 프로파일로.
//
// 사용: node parity/compare.mjs <출력디렉터리>
// sharp 는 이 저장소 의존성이 아니다 — 이미 설치된 sc- 의 것을 빌려 쓴다(새 의존성 0).
// 경로는 RHWP_SHARP_FROM 으로 덮어쓸 수 있다.
import { createRequire } from "node:module";
const req = createRequire(process.env.RHWP_SHARP_FROM ?? "/Users/king/dev/sc-/package.json");
const sharp = req("sharp");
import { readdirSync } from "node:fs";
import { join } from "node:path";

const dir = process.argv[2] ?? "parity/out";
const H = 1024; // 정규화 높이 — 한컴 미리보기 기본값

async function profile(path) {
  const img = sharp(path, { density: 200 }).flatten({ background: "#fff" }).resize({ height: H, fit: "contain", background: "#fff" }).greyscale();
  const { data, info } = await img.raw().toBuffer({ resolveWithObject: true });
  const rows = new Float64Array(info.height);
  const cols = new Float64Array(info.width);
  for (let y = 0; y < info.height; y++) {
    for (let x = 0; x < info.width; x++) {
      const ink = 255 - data[y * info.width + x];
      if (ink > 40) { rows[y] += ink; cols[x] += ink; }
    }
  }
  // 각 행의 잉크 좌·우 끝 — 세로 띠만 보면 "폭이 틀린" 결함(어울림 옆흐름 등)을 놓친다.
  const left = new Int32Array(info.height).fill(-1);
  const right = new Int32Array(info.height).fill(-1);
  for (let y = 0; y < info.height; y++) {
    for (let x = 0; x < info.width; x++) {
      if (255 - data[y * info.width + x] > 40) { if (left[y] < 0) left[y] = x; right[y] = x; }
    }
  }
  let mass = 0;
  for (let i = 0; i < rows.length; i++) mass += rows[i];
  return { rows, cols, left, right, w: info.width, h: info.height, mass };
}

// 잉크 있는 행 → 띠(연속 구간)로 묶는다
function bands(arr, thresh) {
  const out = [];
  let start = -1;
  for (let i = 0; i < arr.length; i++) {
    const on = arr[i] > thresh;
    if (on && start < 0) start = i;
    if (!on && start >= 0) { if (i - start >= 2) out.push([start, i]); start = -1; }
  }
  if (start >= 0) out.push([start, arr.length]);
  return out;
}

// 띠 안 행들의 오른쪽 끝 중앙값 — 줄이 어디서 끝나는가(폭 정합)
function bandRight(prof, [s, e]) {
  const v = [];
  for (let y = s; y < e; y++) if (prof.right[y] >= 0) v.push(prof.right[y]);
  if (!v.length) return -1;
  v.sort((p, q) => p - q);
  return v[Math.floor(v.length / 2)];
}

function score(a, b) {
  // 띠 중심을 그리디 매칭 — 5px 이내면 일치
  const ca = a.map(([s, e]) => (s + e) / 2);
  const cb = b.map(([s, e]) => (s + e) / 2);
  let hit = 0;
  const used = new Set();
  const pairs = [];
  ca.forEach((x, ai) => {
    let best = -1, bd = 6;
    cb.forEach((y, i) => { const d = Math.abs(x - y); if (!used.has(i) && d < bd) { bd = d; best = i; } });
    if (best >= 0) { used.add(best); hit++; pairs.push([ai, best]); }
  });
  return { hit, ours: ca.length, hancom: cb.length, pairs };
}

const names = [...new Set(readdirSync(dir).filter(f => f.endsWith(".hancom.png")).map(f => f.replace(".hancom.png", "")))];
const results = [];
for (const n of names) {
  try {
    const [o, h] = await Promise.all([profile(join(dir, `${n}.ours.svg`)), profile(join(dir, `${n}.hancom.png`))]);
    const maxRow = Math.max(...h.rows);
    const ob = bands(o.rows, maxRow * 0.02), hb = bands(h.rows, maxRow * 0.02);
    const s = score(ob, hb);
    const pct = s.hancom ? Math.round((s.hit / s.hancom) * 100) : 0;
    // 짝지어진 띠끼리 줄 끝 위치를 비교 — 8px 넘게 어긋나면 폭이 틀린 것이다.
    let wOk = 0, wTot = 0;
    for (const [ai, bi] of s.pairs) {
      const ro = bandRight(o, ob[ai]), rh = bandRight(h, hb[bi]);
      if (ro < 0 || rh < 0) continue;
      wTot++;
      if (Math.abs(ro - rh) <= 8) wOk++;
    }
    const wpct = wTot ? Math.round((wOk / wTot) * 100) : -1;
    // 잉크량이 극단으로 어긋나면 오라클 자체를 의심한다 — PrvImage 는 한글이 **마지막에
    // 저장할 때** 만든 그림이라, 다른 도구가 재저장한 파일에선 낡은 그림이 남는다(실측:
    // 253E164F57A1BC6934-empty 는 본문이 비었는데 미리보기엔 포스터가 있다).
    const ratio = h.mass > 0 ? o.mass / h.mass : 0;
    const suspect = ratio < 0.2 || ratio > 5;
    results.push({ n, pct, wpct, ...s, suspect, ratio });
  } catch (e) { results.push({ n, pct: -1, err: String(e).slice(0, 40) }); }
}
results.sort((a, b) => a.pct - b.pct);
const ok = results.filter(r => r.pct >= 0 && !r.suspect);
const bad = results.filter(r => r.pct < 0 || r.suspect);
for (const r of ok) console.log(`${String(r.pct).padStart(4)}%  폭 ${String(r.wpct).padStart(4)}%  줄 ${String(r.ours).padStart(3)}/${String(r.hancom).padStart(3)}  ${r.n}`);
if (bad.length) {
  console.log(`\n[제외 ${bad.length}건 — 오라클 의심/오류]`);
  for (const r of bad) console.log(`  ${r.n}  ${r.err ?? `잉크비 ${r.ratio?.toFixed(2)}`}`);
}
const avg = ok.length ? Math.round(ok.reduce((a, r) => a + r.pct, 0) / ok.length) : 0;
const perfect = ok.filter(r => r.pct === 100).length;
const wok = ok.filter(r => r.wpct >= 0);
const wavg = wok.length ? Math.round(wok.reduce((a, r) => a + r.wpct, 0) / wok.length) : 0;
console.log(`\n세로(줄 위치) ${avg}% · 완전일치 ${perfect}/${ok.length} | 가로(줄 끝) ${wavg}% (${wok.length}건) | 제외 ${bad.length}건`);
