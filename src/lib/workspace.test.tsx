import { describe, expect, it, vi } from 'vitest'
import { renderToStaticMarkup } from 'react-dom/server'
import { prepareWorkspace, updateCounts, updateDocumentPath, updateHit, type WorkspacePreview, type UpdateRecord } from './workspace'
import { selectionForHit } from './documentSearch'
import { UpdatePanel } from '../UpdatePanel'

const preview: WorkspacePreview = { unity_root: 'X:/Unity', git_root: 'X:/Unity', worktree: 'X:/Unity_DesignBook', development_branch: 'feature/player', head: 'abc', upstream: null, existing: false, blockers: [], source_changes: [' M Assets/file'], staged: [], document_changes: [], unpushed_documents: [], worktree_changes: [], sparse_ready: true, sparse_scope: "DesignDocs" }
const record: UpdateRecord = { id: 'a:b', before: 'a', after: 'b', timestamp: 100, read: false, applied: true, files: [
  { path: 'DesignDocs/page.md', previous_path: null, status: 'M', hunks: [{ line: 3, heading: 'Attack', removed: false, text: '@@ -1 +1 @@' }], patch: '-old\n+new' },
  { path: 'DesignDocs/old.md', previous_path: null, status: 'D', hunks: [{ line: 0, heading: 'removed', removed: true, text: '@@ -1 +0,0 @@' }], patch: '-deleted' },
] }

describe('independent workspace and update navigation', () => {
  it('does not write when creation is declined', async () => {
    const invoke = vi.fn(async () => preview)
    expect(await prepareWorkspace('X:/Unity', invoke as never, async () => false)).toBe(false)
    expect(invoke.mock.calls).toHaveLength(1)
  })
  it('creates only after confirmation and restores existing worktrees without creation', async () => {
    const invoke = vi.fn(async () => preview)
    const confirm = vi.fn(async () => true)
    expect(await prepareWorkspace('X:/Unity', invoke as never, confirm)).toBe(true)
    expect(invoke).toHaveBeenLastCalledWith('create_workspace', { path: 'X:/Unity', confirmed: true })
    expect(confirm.mock.calls[0]?.length).toBe(1)
    const reuse = vi.fn(async () => ({ ...preview, existing: true }))
    await prepareWorkspace('X:/Unity', reuse as never, confirm)
    expect(reuse.mock.calls).toHaveLength(1)
  })
  it('requires a separate confirmation to convert an existing full worktree', async () => {
    const invoke = vi.fn(async () => ({ ...preview, existing: true, sparse_ready: false }))
    const confirm = vi.fn(async (_message: string) => false)
    expect(await prepareWorkspace('X:/Unity', invoke as never, confirm)).toBe(false)
    expect(confirm.mock.calls[0]?.[0]).toContain('Sparse Checkout に変換')
    expect(invoke.mock.calls).toHaveLength(1)
    expect(await prepareWorkspace('X:/Unity', invoke as never, async () => true)).toBe(true)
    expect(invoke).toHaveBeenLastCalledWith('create_workspace', { path: 'X:/Unity', confirmed: true })
  })
  it('reports occupied main, original document modifications and unpublished commits without mutation', async () => {
    const invoke = vi.fn(async () => ({ ...preview, blockers: ['main occupied'], document_changes: ['M DesignDocs/important.md'], unpushed_documents: ['123 docs commit'] }))
    await expect(prepareWorkspace('X:/Unity', invoke as never, async () => true)).rejects.toThrow('important.md')
    expect(invoke.mock.calls).toHaveLength(1)
  })
  it('maps actual changed lines to reading view and refuses deleted or invalid locations', () => {
    const content = '# Battle\r\n\r\n## 攻撃判定\r\nnew\r\n'
    const hit = updateHit(content, 'DesignDocs/page.md', record.files[0].hunks[0])!
    expect(hit.query).toBe('攻撃判定')
    expect(selectionForHit(content, hit)).not.toBeNull()
    expect(updateHit(content, 'DesignDocs/page.md', record.files[1].hunks[0])).toBeNull()
    expect(updateHit(content, 'DesignDocs/page.md', { ...record.files[0].hunks[0], line: 50 })).toBeNull()
    const json = updateHit('{"value":2}', 'DesignDocs/a.json', { line: 1, heading: '', removed: false, text: '' })!
    expect(json.kind).toBe('json')
    expect(selectionForHit('{"value":2}', json)).not.toBeNull()
  })
  it('confines update navigation to the exact document scope', () => {
    expect(updateDocumentPath(record.files[0], 'DesignDocs')).toBe('DesignDocs/page.md')
    expect(updateDocumentPath({ ...record.files[0], path: 'Assets/page.md' }, 'DesignDocs')).toBeNull()
    expect(updateDocumentPath({ ...record.files[0], path: 'DesignDocs/../outside.md' }, 'DesignDocs')).toBeNull()
    expect(updateDocumentPath({ ...record.files[0], path: 'Unity/DesignDocs/page.md' }, 'Unity/DesignDocs')).toBe('DesignDocs/page.md')
  })
  it('renders applied updates with hashes, unread state, scrollable file list, deleted diff and popup only once requested', () => {
    const props = { records: [record, { ...record, id: 'pending', applied: false }], busy: false, status: '最新です', onSync: () => {}, onRead: () => {}, onNavigate: () => {}, popup: null, onDismiss: () => {} }
    const html = renderToStaticMarkup(<UpdatePanel {...props} />)
    expect(html).toContain('1 未読'); expect(html).toContain('変更前後の差分'); expect(html).toContain('削除内容の差分'); expect(html).not.toContain('pending'); expect(html).not.toContain('aria-modal="true"')
    const popup = renderToStaticMarkup(<UpdatePanel {...props} popup={record.id} />)
    expect(popup).toContain('設計書が更新されました'); expect(popup).toContain('後で見る')
    expect(updateCounts(record)).toEqual({ added: 0, modified: 1, deleted: 1, renamed: 0 })
  })
})
