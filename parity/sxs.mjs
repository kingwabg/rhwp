// [parity] 좌=한컴 정답지, 우=우리 렌더 — 한 장으로 합쳐 눈 대조 비용을 줄인다.
// 사용: node parity/sxs.mjs <출력디렉터리> <이름> [출력png]
import { createRequire } from "node:module";
import { readFileSync } from "node:fs";
const sharp = createRequire(process.env.RHWP_SHARP_FROM ?? "/Users/king/dev/sc-/package.json")("sharp");
const [dir, name, out = "/tmp/sxs.png"] = process.argv.slice(2);
const H = 900, W = 640;
async function svgFixed(p) {
  let svg = readFileSync(p, "utf8");
  for (const m of [...svg.matchAll(/data:image\/gif;base64,([A-Za-z0-9+/=]+)/g)]) {
    try {
      const png = await sharp(Buffer.from(m[1], "base64")).png().toBuffer();
      svg = svg.replace(m[0], `data:image/png;base64,${png.toString("base64")}`);
    } catch {}
  }
  return Buffer.from(svg);
}
const norm = (src) => sharp(src, { density: 200 }).flatten({ background: "#fff" })
  .resize({ width: W, height: H, fit: "contain", background: "#fff" }).png().toBuffer();
const lab = (t) => Buffer.from(`<svg width="${W}" height="26"><rect width="100%" height="100%" fill="#111"/><text x="8" y="19" font-family="sans-serif" font-size="14" fill="#fff">${t}</text></svg>`);
const h = await norm(`${dir}/${name}.hancom.png`);
const o = await norm(await svgFixed(`${dir}/${name}.ours.svg`));
await sharp({ create: { width: W * 2 + 8, height: H + 26, channels: 3, background: "#888888" } })
  .composite([
    { input: lab("한컴 정답지"), top: 0, left: 0 }, { input: h, top: 26, left: 0 },
    { input: lab("우리 엔진"), top: 0, left: W + 8 }, { input: o, top: 26, left: W + 8 },
  ]).png().toFile(out);
console.log(out);
