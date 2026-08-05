/**
 * E2E: 밑줄형 캐럿 — 캐럿은 세로 바가 아니라 줄 바닥의 가로 바(2px)이고
 * 색은 파랑↔하늘색 중간(#4367F5)이다. (caret-renderer.ts)
 */
import {
  runTest, createNewDocument, clickEditArea, typeText, screenshot, assert,
} from './helpers.mjs';

runTest('밑줄형 캐럿 (모양·색)', async ({ page }) => {
  await createNewDocument(page);
  await clickEditArea(page);
  await typeText(page, '캐럿확인');
  await page.evaluate(() => new Promise(r => setTimeout(r, 600)));
  const c = await page.evaluate(() => {
    const el = document.querySelector('.caret');
    if (!el) return null;
    el.style.opacity = '1';
    const r = el.getBoundingClientRect();
    return { w: r.width, h: r.height, bg: el.style.background || el.style.backgroundColor, display: el.style.display };
  });
  assert(c, '캐럿 엘리먼트 존재');
  assert(c.display === 'block', `캐럿 표시됨: ${c.display}`);
  assert(c.h === 2, `밑줄 두께 2px: h=${c.h}`);
  assert(c.w > c.h, `가로 바 형태(w>h): w=${c.w} h=${c.h}`);
  assert(c.bg.includes('67, 103, 245'), `색 #4367F5: ${c.bg}`);
  await screenshot(page, 'caret-underline');
});
