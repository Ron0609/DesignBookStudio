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
    const path = 'DesignDocs/reading.md'
    const content = '# 検証用文書\n\n修炼属性 の概要です。\n\n## 資料対応表\n\n| 資料 | 内容 |\n| --- | --- |\n| `docs/02_World_Bible.md` | 寿命、種族、修炼属性、職業 |\n'
    localStorage.setItem('designbook.lastUnityProject.v1', 'X:/isolated-fixture')
    window.isTauri = true
    window.__TAURI_INTERNALS__ = {
      metadata: { currentWindow: { label: 'main' }, currentWebview: { label: 'main' } },
      transformCallback: () => 1,
      unregisterCallback: () => {},
      invoke: async (command, args) => {
        switch (command) {
          case 'open_repository': return { unity_root: 'X:/isolated-fixture', git_root: 'X:/isolated-fixture', documents_path: 'X:/isolated-fixture/DesignDocs', has_documents: true }
          case 'list_documents': return [{ path, kind: 'markdown' }]
          case 'list_document_directories': return ['DesignDocs']
          case 'read_document': return content
          case 'git_status': return { branch: 'fixture', changed: [], changes: [], document_changed: [], other_changed: [], ahead: 0, behind: 0, clean: true, has_upstream: false, tracking_error: null }
          case 'check_image_references': return []
          case 'plugin:event|listen': return 1
          case 'plugin:event|unlisten': return null
          case 'search_documents': {
            const query = args.query.trim()
            const matches = []
            content.split('\n').forEach((line, index) => {
              let from = 0
              while (query && (from = line.indexOf(query, from)) >= 0) {
                matches.push({ path, kind: 'markdown', excerpt: line, line: index + 1, column: from + 1, length: query.length, excerpt_offset: from, query })
                from += query.length
              }
            })
            return { matches, truncated: false, skipped: [] }
          }
          default: throw new Error(`Unexpected command in isolated test: ${command}`)
        }
      },
    }
    window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} }
  })
  await page.goto(`http://127.0.0.1:${server.address().port}`)
  const editor = page.locator('.milkdown-host .ProseMirror')
  await editor.waitFor()
  assert.equal(await editor.getAttribute('contenteditable'), 'false')
  const originalText = await editor.textContent()
  await editor.locator('p').first().click()
  await page.keyboard.type('ACCIDENTAL-EDIT')
  assert.equal(await editor.textContent(), originalText, 'Clicking/typing in reading mode must not alter the document')
  await page.getByRole('button', { name: '編集', exact: true }).click()
  await page.waitForFunction(() => document.querySelector('.ProseMirror')?.getAttribute('contenteditable') === 'true')
  await page.getByRole('button', { name: '閲覧に戻る', exact: true }).click()
  await page.getByRole('textbox', { name: '文書名・本文を検索' }).fill('修炼属性')
  await page.waitForFunction(() => document.querySelectorAll('.search-result-hit').length === 2)
  await page.locator('.search-result-hit').last().click()
  try { await page.waitForFunction(() => CSS.highlights.get('designbook-search-result')?.size === 1, null, { timeout: 10000 }) }
  catch (error) {
    console.log(await page.evaluate(() => ({ notice: document.querySelector('.toast')?.textContent, editors: [...document.querySelectorAll('.ProseMirror')].map((node) => node.textContent), tables: [...document.querySelectorAll('.milkdown-host table')].map((node) => ({ parent: node.parentElement.className, insideDocument: !!node.closest('.ProseMirror'), text: node.textContent })), highlights: [...CSS.highlights.keys()] })))
    console.log(errors)
    throw error
  }
  assert.equal(await page.locator('.search-source-lines').count(), 0, 'Search must default to rendered reading mode')
  assert.equal(await editor.getAttribute('contenteditable'), 'false')
  assert.equal(await page.locator('.milkdown-host .ProseMirror table:visible').count(), 1, 'Markdown table must remain rendered in the document; hidden drag previews do not count')
  const target = await page.evaluate(() => {
    const range = [...CSS.highlights.get('designbook-search-result')][0]
    return { text: range.toString(), cell: range.startContainer.parentElement.closest('td')?.textContent }
  })
  assert.equal(target.text, '修炼属性')
  assert.match(target.cell, /寿命、種族、修炼属性、職業/)
  await page.getByRole('button', { name: '原文を表示', exact: true }).click()
  await page.locator('.search-source-lines').waitFor()
  await page.getByRole('button', { name: '文書表示へ戻る', exact: true }).click()
  await page.waitForFunction(() => CSS.highlights.get('designbook-search-result')?.size === 1)
  assert.equal(await page.locator('.search-source-lines').count(), 0)
  assert.equal(await editor.getAttribute('contenteditable'), 'false')
  await page.getByRole('button', { name: '編集', exact: true }).click()
  await editor.locator('p').first().click()
  await page.keyboard.press('End')
  await page.keyboard.type('INTENTIONAL-EDIT')
  await page.getByRole('button', { name: '閲覧に戻る', exact: true }).click()
  assert.match(await editor.textContent(), /INTENTIONAL-EDIT/)
  assert.equal(await page.locator('.save-button').isEnabled(), true, 'Leaving edit mode must retain the latest keystrokes before the debounced callback')
  assert.deepEqual(errors, [])
  console.log('PASS: explicit edit mode, read-only search navigation, rendered table, exact later occurrence, no page errors')
} finally {
  await browser?.close()
  await new Promise((resolve) => server.close(resolve))
}
