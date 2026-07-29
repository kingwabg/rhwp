// html_table_import panic 5건 재현 — pkg 경로를 인자로 받아 각 케이스가 panic하는지 본다.
import { readFile } from "node:fs/promises";
import { pathToFileURL } from "node:url";
import { join } from "node:path";

const pkgDir = process.argv[2] || "./pkg";
const core = await import(pathToFileURL(join(pkgDir, "rhwp.js")).href);
await core.default({ module_or_path: await readFile(join(pkgDir, "rhwp_bg.wasm")) });

const cases = [
  ["중첩 표", `<table><tr><td>바깥<table><tr><td>안쪽</td></tr></table></td></tr></table>`],
  ["안 닫힌 td", `<table><tr><td>셀1<td>셀2</tr></table>`],
  ["안 닫힌 table", `<table><tr><td>셀</td></tr>`],
  ["표+문단 3개↑", `<p>앞</p><table><tr><td>표</td></tr></table><p>뒤1</p><p>뒤2</p>`],
];

for (const [name, html] of cases) {
  const d = core.HwpDocument.createEmpty();
  d.createBlankDocument();
  let verdict;
  try {
    d.pasteHtml(0, 0, 0, html);
    verdict = "OK (paste 성공)";
  } catch (e) {
    verdict = /unreachable|recursive use|borrowed/.test(String(e))
      ? "PANIC " + String(e).slice(0, 50)
      : "graceful: " + String(e).slice(0, 60);
  }
  // panic 후 문서가 살았는지 — exportHwp 시도
  let alive;
  try { d.exportHwp(); alive = "문서 생존"; }
  catch (e) { alive = "문서 죽음 " + String(e).slice(0, 40); }
  try { d.free(); } catch {}
  console.log(`[${name}] ${verdict} | ${alive}`);
}
