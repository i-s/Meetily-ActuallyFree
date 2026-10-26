/** Real BlockNote browser regression against the development preview.
 * PLAYWRIGHT_MODULE may name a local playwright-core installation; use
 * CHROMIUM_PATH for an existing system Chromium. This does not test native IO. */
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const assert = require('node:assert/strict');
(async () => {
  const browser = await chromium.launch({ executablePath: process.env.CHROMIUM_PATH || undefined, args: ['--no-sandbox'] });
  try {
    const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
    const ipc = (command, args = {}) => page.evaluate(([command, args]) => window.__TAURI_INTERNALS__.invoke(command, args), [command, args]);
    const baseUrlFlag = process.argv.indexOf('--base-url');
    const baseUrl = baseUrlFlag >= 0 ? process.argv[baseUrlFlag + 1] : process.env.PREVIEW_URL || 'http://localhost:3118';
    await page.goto(baseUrl, { waitUntil: 'networkidle' });
    await page.getByRole('button', { name: 'Start recording', exact: true }).click();
    await page.getByRole('button', { name: 'Pause recording', exact: true }).click();
    const { meeting_id: meetingId } = await ipc('get_recording_state');
    const editor = page.locator('.af-notes-editor [contenteditable=true]').first();
    const note = 'Browser regression: сохранить Meeting notes без речи.';
    await editor.fill(note);
    await page.getByRole('tab', { name: 'Speakers', exact: false }).click();
    await page.getByRole('tab', { name: 'Ask AI', exact: true }).click();
    await page.locator('textarea').fill('What do my Meeting notes say?');
    await page.getByRole('button', { name: 'Send', exact: true }).click();
    await page.waitForFunction(id => window.__TAURI_INTERNALS__.invoke('get_meeting_chat', {meetingId:id}).then(turns => turns.length === 1), meetingId);
    const notes = await ipc('api_get_meeting_notes', {meetingId});
    assert(notes.markdown.includes(note), 'Immediate Ask AI must flush the real BlockNote edit after tab changes');
    assert(JSON.stringify(notes.json).includes(note), 'SQLite fixture must retain exact edited JSON');
    const tabs = page.getByRole('tab');
    const boxes = await tabs.evaluateAll(nodes => nodes.map(node => { const r = node.getBoundingClientRect(); return {name: node.textContent, left:r.left, right:r.right, width:r.width, scroll:node.scrollWidth, client:node.clientWidth}; }));
    for (const tab of boxes) assert(tab.scroll <= tab.client, `${tab.name} must fit its tab: ${JSON.stringify(tab)}`);
    for (let i = 1; i < boxes.length; i++) assert(boxes[i].left >= boxes[i-1].right, 'Live tabs must not overlap');
    if (process.env.PREVIEW_SCREENSHOT) await page.screenshot({path: process.env.PREVIEW_SCREENSHOT});
    await page.getByRole('button', { name: 'Stop recording', exact: true }).click();
    await page.waitForURL(/meeting-details/, {timeout: 20000});
    assert((await ipc('api_get_meeting_notes', {meetingId})).markdown.includes(note), 'Silent Stop must retain notes');
    console.log('PASS: real BlockNote edit → tab switches → immediate Ask AI flush → silent Stop; live tabs fit.');
  } finally { await browser.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
