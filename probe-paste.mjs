// pasteHtml 표 배치 회귀 진단 — 엔진 pkg 경로를 인자로 받아 표가 어디에 놓이는지 본다.
// 사용: node probe-paste.mjs <pkgDir>
import { readFile } from "node:fs/promises";
import { pathToFileURL } from "node:url";
import { join } from "node:path";

const pkgDir = process.argv[2];
const core = await import(pathToFileURL(join(pkgDir, "rhwp.js")).href);
await core.default({ module_or_path: await readFile(join(pkgDir, "rhwp_bg.wasm")) });

const html = `<table>
<tr><th colspan="6" style="text-align:center;font-weight:bold">제목</th></tr>
<tr><th style="width:14%">라벨</th><td colspan="5" style="width:86%">값</td></tr>
</table>`;

const doc = core.HwpDocument.createEmpty();
doc.createBlankDocument();
const paste = JSON.parse(doc.pasteHtml(0, 0, 0, html));
console.log("pasteHtml 응답:", JSON.stringify(paste).slice(0, 200));
console.log("문단 수(sec0):", doc.getParagraphCount(0));

// 각 문단의 컨트롤 0을 표로 읽어본다 — 어디에 표가 있는지 스캔
for (let p = 0; p < doc.getParagraphCount(0); p++) {
  for (let c = 0; c < 3; c++) {
    try {
      const props = JSON.parse(doc.getTableProperties(0, p, c));
      console.log(`  표 발견 @ (0,${p},${c}): rows×cols=${props.rowCount}×${props.colCount} tableWidth=${props.tableWidth} treatAsChar=${props.treatAsChar} vertRelTo=${props.vertRelTo}`);
    } catch (e) {
      // 표 아님 — 조용히
    }
  }
}

// 앱이 가정하는 (0,0,0)이 표인가?
try {
  const p000 = JSON.parse(doc.getTableProperties(0, 0, 0));
  console.log("(0,0,0) 표 OK:", p000.rowCount, "×", p000.colCount);
} catch (e) {
  console.log("(0,0,0) 실패:", String(e).slice(0, 80));
}
doc.free();
