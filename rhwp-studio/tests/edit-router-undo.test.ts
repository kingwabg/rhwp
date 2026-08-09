import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const rootDir = dirname(dirname(fileURLToPath(import.meta.url)));

function source(path: string): string {
  return readFileSync(join(rootDir, path), 'utf8');
}

// 문서를 mutate 하는 커맨드/다이얼로그/드래그 경로는 반드시 편집 라우터
// (executeOperation)를 통과해 undo 스택에 기록되어야 한다 (#1320 계약).
// picture-props-undo.test.ts 와 동일한 소스 계약 검사 — 커버리지 확장분.

test('insert.ts 개체/삽입 커맨드는 스냅샷으로 undo 기록된다', () => {
  const insert = source('src/command/commands/insert.ts');
  const zOrderCount = insert.match(/operationType: 'objectZOrder'/g)?.length ?? 0;
  assert.equal(zOrderCount, 4, 'arrange-front/forward/backward/back 4곳 모두 스냅샷 기록 필요');
  for (const op of [
    'objectDelete', 'objectGroup', 'objectUngroup', 'objectRotate', 'objectFlip',
    'insertEquation', 'insertField', 'insertFootnote', 'insertEndnote',
  ]) {
    assert.match(insert, new RegExp(`operationType: '${op}'`), `insert.ts 에 '${op}' 스냅샷 기록 필요`);
  }
});

test('edit.ts 누름틀 고치기는 스냅샷으로 undo 기록된다', () => {
  assert.match(source('src/command/commands/edit.ts'), /operationType: 'updateField'/);
});

test('설정/삽입 다이얼로그 apply는 스냅샷으로 undo 기록된다', () => {
  const cases: Array<[string, string]> = [
    ['src/ui/new-number-dialog.ts', 'insertNewNumber'],
    ['src/ui/formula-dialog.ts', 'insertFormula'],
    ['src/ui/bookmark-dialog.ts', 'bookmark'],
    ['src/ui/page-setup-dialog.ts', 'pageSetup'],
    ['src/ui/section-settings-dialog.ts', 'sectionSettings'],
    ['src/ui/column-settings-dialog.ts', 'columnSettings'],
    ['src/ui/page-border-dialog.ts', 'pageBorder'],
    ['src/ui/endnote-shape-dialog.ts', 'endnoteShape'],
    ['src/ui/style-dialog.ts', 'styleDelete'],
    ['src/ui/style-edit-dialog.ts', 'styleEdit'],
  ];
  for (const [path, op] of cases) {
    assert.match(source(path), new RegExp(`operationType: '${op}'`), `${path} 에 '${op}' 스냅샷 기록 필요`);
  }
});

test('회전 드래그 종료는 before/after 각도를 record 로 기록한다', () => {
  const picture = source('src/engine/input-handler-picture.ts');
  const start = picture.indexOf('export function finishPictureRotateDrag');
  assert.notEqual(start, -1, 'finishPictureRotateDrag not found');
  const block = picture.slice(start, picture.indexOf('\nexport function ', start + 1));
  assert.match(block, /kind: 'record'/);
  assert.match(block, /ResizeObjectCommand/);
  assert.match(block, /rotationAngle/);
});

test('직선 끝점 드래그 종료는 시작/최종 끝점을 record 로 기록한다', () => {
  const mouse = source('src/engine/input-handler-mouse.ts');
  assert.match(mouse, /new MoveLineEndpointCommand\(/);
  assert.match(mouse, /origEndpoints/);
});

test('클릭 시 맨 앞 이동은 z-order 가 변한 경우에만 스냅샷 record 된다', () => {
  const mouse = source('src/engine/input-handler-mouse.ts');
  const start = mouse.indexOf('function bringShapeToFront');
  assert.notEqual(start, -1, 'bringShapeToFront not found');
  const block = mouse.slice(start, mouse.indexOf('\nfunction ', start + 1));
  assert.match(block, /new SnapshotCommand\('objectZOrder'/);
  assert.match(block, /discardSnapshot/, 'no-op 클릭은 스냅샷을 버려 빈 undo 엔트리를 만들지 않아야 함');
});
