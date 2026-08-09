// P0-1 재현: NaN 좌표 hitTest가 panic하고 문서 핸들이 영구 잠기는지 확인.
// 갓 빌드한 pkg/를 직접 로드한다(sc- QA 하네스와 같은 방식).
import { readFile } from "node:fs/promises";

const core = await import("./pkg/rhwp.js");
await core.default({ module_or_path: await readFile("./pkg/rhwp_bg.wasm") });

const doc = core.HwpDocument.createEmpty();
doc.createBlankDocument();
JSON.parse(
  doc.createTableEx(
    JSON.stringify({
      sectionIdx: 0,
      paraIdx: 0,
      charOffset: 0,
      rowCount: 2,
      colCount: 2,
      treatAsChar: true,
    }),
  ),
);

let hitVerdict, freeVerdict;
try {
  const r = doc.hitTest(0, Number.NaN, Number.NaN);
  hitVerdict = "RETURNED " + String(r).slice(0, 120);
} catch (e) {
  hitVerdict = "THREW " + String(e).slice(0, 160);
}

// panic 후 문서 핸들이 살아있는지: 정상 좌표 hitTest + free()
let aliveVerdict;
try {
  const r2 = doc.hitTest(0, 10, 10);
  aliveVerdict = "ALIVE " + String(r2).slice(0, 80);
} catch (e) {
  aliveVerdict = "DEAD " + String(e).slice(0, 120);
}
try {
  doc.free();
  freeVerdict = "OK";
} catch (e) {
  freeVerdict = "THREW " + String(e).slice(0, 120);
}

console.log("hitTest(NaN,NaN):", hitVerdict);
console.log("hitTest(10,10) after:", aliveVerdict);
console.log("free():", freeVerdict);
