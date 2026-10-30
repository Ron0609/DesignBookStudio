import { createServer } from 'node:http'
import { readFile } from 'node:fs/promises'
import { fileURLToPath } from 'node:url'
import { resolve, extname, sep } from 'node:path'
import assert from 'node:assert/strict'

// Isolated browser test with mocked Tauri commands. No Unity files or Git operations.
const { chromium } = await import(process.env.DESIGNBOOK_PLAYWRIGHT_MODULE || 'playwright')
const dist = fileURLToPath(new URL('../dist/', import.meta.url))
const server = createServer(async (request, response) => {
  const path = resolve(dist, '.' + (new URL(request.url, 'http://localhost').pathname === '/' ? '/index.html' : decodeURIComponent(new URL(request.url, 'http://localhost').pathname)))
  if (!path.startsWith(dist.endsWith(sep) ? dist : dist + sep)) { response.writeHead(403).end(); return }
  try {
    const mime = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.svg': 'image/svg+xml' }[extname(path)] || 'application/octet-stream'
    response.writeHead(200, { 'content-type': mime }).end(await readFile(path))
  } catch { response.writeHead(404).end() }
})
await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve))
let browser
try {
  browser = await chromium.launch({ channel: 'msedge', headless: true })
  const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } })
  const errors = []
  page.on('pageerror', (error) => errors.push(error.message))
  await page.addInitScript(() => {
    const path = 'DesignDocs/battle.md'
    const content = '# Battle\n\n## Attack\nnew attack content\n'
    const record = { id: 'before:after', before: 'a'.repeat(40), after: 'b'.repeat(40), timestamp: 1793372400, read: false, applied: true, files: [
      { path, previous_path: null, status: 'M', patch: '-old attack\n+new attack content', hunks: [{ line: 3, heading: 'Attack', removed: false, text: '@@ -1,3 +1,4 @@' }] },
      { path: 'DesignDocs/deleted.md', previous_path: null, status: 'D', patch: '-deleted content', hunks: [{ line: 0, heading: 'deleted', removed: true, text: '@@ -1 +0,0 @@' }] },
    ] }
    localStorage.setItem('designbook.lastUnityProject.v1', 'X:/isolated-unity')
    window.isTauri = true
    window.testCalls = []
    window.__TAURI_INTERNALS__ = {
      metadata: { currentWindow: { label: 'main' }, currentWebview: { label: 'main' } }, transformCallback: () => 1, unregisterCallback: () => {},
      invoke: async (command, args) => {
        window.testCalls.push({ command, args })
        const info = { unity_root: 'X:/isolated-unity', git_root: 'X:/isolated-unity_DesignBook', worktree_root: 'X:/isolated-unity_DesignBook', development_branch: 'feature/player', documents_path: 'X:/isolated-unity_DesignBook/DesignDocs', has_documents: true }
        switch (command) {
          case 'preview_workspace': return { existing: true, sparse_ready: true, sparse_scope: 'DesignDocs', blockers: [], document_changes: [], unpushed_documents: [] }
          case 'open_repository': case 'project_context': return info
          case 'list_documents': return [{ path, kind: 'markdown' }]
          case 'list_document_directories': return ['DesignDocs']
          case 'read_document': return content
          case 'git_status': return { branch: 'main', changed: [], changes: [], document_changed: [], other_changed: [], ahead: 0, behind: 0, clean: true, has_upstream: true, tracking_error: null }
          case 'check_image_references': return []
          case 'plugin:event|listen': return 1
          case 'plugin:event|unlisten': return null
          case 'sync_workspace': {
            const old = localStorage.getItem('fixture.updates')
            const records = old ? JSON.parse(old) : [record]
            localStorage.setItem('fixture.updates', JSON.stringify(records))
            return { message: args.unsaved ? '未保存の編集があるため自動同期を停止しました。' : '設計書は最新です。', records, new_id: old ? null : record.id }
          }
          case 'update_records': return JSON.parse(localStorage.getItem('fixture.updates') || '[]')
          case 'mark_updates_read': {
            const records = JSON.parse(localStorage.getItem('fixture.updates'))
            for (const item of records) if (!args.id || item.id === args.id) item.read = true
            localStorage.setItem('fixture.updates', JSON.stringify(records))
            return records
          }
          case 'locate_update': return content
          default: throw new Error(`Unexpected command: ${command}`)
        }
      },
    }
    window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} }
  })
  await page.goto(`http://127.0.0.1:${server.address().port}`)
  await page.getByRole('dialog', { name: '設計書が更新されました' }).waitFor()
  assert.match(await page.locator('.topbar .branch').innerText(), /XiuxianGame_DesignDocs \/ Docs: main/)
  await page.getByRole('button', { name: '後で見る', exact: true }).click()
  await page.getByRole('button', { name: '→ Attack', exact: false }).click()
  await page.waitForFunction(() => CSS.highlights.get('designbook-search-result')?.size === 1)
  assert.equal(await page.locator('.ProseMirror').getAttribute('contenteditable'), 'false')
  assert.match(await page.locator('.update-inline-diff').innerText(), /new attack content/)
  await page.getByRole('button', { name: '－削除内容の差分', exact: false }).click()
  await page.locator('.diff-view').waitFor()
  assert.equal(await page.locator('.diff-view').innerText(), '-deleted content')
  await page.locator('.modal > header button').click()
  await page.getByRole('button', { name: '編集', exact: true }).click()
  await page.locator('.ProseMirror p').first().click()
  await page.keyboard.press('End')
  await page.keyboard.type('UNSAVED-FIXTURE')
  await page.getByRole('button', { name: '更新を確認', exact: true }).click()
  await page.waitForFunction(() => window.testCalls.filter((call) => call.command === 'sync_workspace').at(-1)?.args.unsaved === true)
  assert.match(await page.locator('.ProseMirror').innerText(), /UNSAVED-FIXTURE/)
  await page.getByRole('button', { name: '→ Attack', exact: false }).click()
  await page.locator('.diff-view').waitFor()
  assert.match(await page.locator('.ProseMirror').innerText(), /UNSAVED-FIXTURE/)
  await page.locator('.modal > header button').click()
  // Reload intentionally models a new session after the isolated unsaved-buffer protection check.
  await page.reload()
  await page.getByRole('button', { name: '更新を確認', exact: true }).waitFor()
  await page.waitForFunction(() => document.querySelector('.github-updates [role=status]')?.textContent.includes('最新'))
  assert.equal(await page.getByRole('dialog', { name: '設計書が更新されました' }).count(), 0)
  assert.match(await page.getByRole('combobox', { name: '同期記録' }).innerText(), /既読/)
  assert.deepEqual(errors, [])
  assert.equal(await page.evaluate(() => window.testCalls.some((call) => call.command === 'create_workspace')), false)
  console.log('PASS: restored worktree, distinct development/docs branch display, update notification, rendered section navigation, inline diff, deleted read-only diff, unsaved protection, durable read state and no repeated popup')
} finally {
  await browser?.close()
  await new Promise((resolve) => server.close(resolve))
}
