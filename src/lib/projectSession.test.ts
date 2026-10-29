import { describe, expect, it, vi } from 'vitest'
import { rememberProject, restoreProject, mutateAndRefresh, loadProjectSnapshot, type SessionInvoke } from './projectSession'

function memoryStorage() {
  const data = new Map<string, string>()
  return { getItem: (key: string) => data.get(key) ?? null, setItem: (key: string, value: string) => { data.set(key, value) } }
}

describe('project restoration and Git refresh', () => {
  it('restores the complete project via backend validation, tree, directories, Git and file reads', async () => {
    const storage = memoryStorage()
    rememberProject(storage, 'X:/temporary/Unity')
    const commands: string[] = []
    let generation = 0
    const invoke: SessionInvoke = async <T>(command: string, args?: Record<string, unknown>) => {
      commands.push(command)
      const values: Record<string, unknown> = {
        open_repository: { unity_root: args?.path, git_root: 'X:/temporary', has_documents: true, documents_path: 'X:/temporary/Unity/DesignDocs' },
        list_documents: [{ path: 'DesignDocs/deep/page.md', kind: 'markdown' }],
        list_document_directories: ['DesignDocs', 'DesignDocs/deep'],
        git_status: { document_changed: generation === 0 ? ['DesignDocs/deep/page.md'] : [], other_changed: ['Assets/game.asset'], ahead: generation },
        read_document: 'actual document',
      }
      return values[command] as T
    }
    const first = await loadProjectSnapshot('X:/temporary/Unity', invoke)
    expect(first.status.document_changed).toHaveLength(1)
    generation = 1
    // A new session reuses only the saved path, never the previous tree or Git snapshot.
    await restoreProject(storage, async (path) => {
      const restored = await loadProjectSnapshot(path, invoke)
      expect(restored.files).toEqual([{ path: 'DesignDocs/deep/page.md', kind: 'markdown' }])
      expect(restored.directories).toContain('DesignDocs/deep')
      expect(restored.firstContent).toBe('actual document')
      expect(restored.status.document_changed).toEqual([])
      expect(restored.status.other_changed).toEqual(['Assets/game.asset'])
      expect(restored.status.ahead).toBe(1)
    })
    expect(commands).toEqual(Array(2).fill(['open_repository', 'list_documents', 'git_status', 'list_document_directories', 'read_document']).flat())
  })

  it('stops restoration if backend validation fails and does not create files or a repository', async () => {
    const commands: string[] = []
    const invoke: SessionInvoke = async (command) => { commands.push(command); throw new Error('path missing or access denied') }
    await expect(loadProjectSnapshot('X:/missing', invoke)).rejects.toThrow('access denied')
    expect(commands).toEqual(['open_repository'])
  })
  it('reopens the persisted project after a restart and reloads real state', async () => {
    const storage = memoryStorage()
    rememberProject(storage, 'X:/temporary/Unity')
    const reopen = vi.fn(async () => {})
    await restoreProject(storage, reopen)
    expect(reopen).toHaveBeenCalledWith('X:/temporary/Unity')
  })
  it('does nothing on first launch and reports inaccessible saved projects without replacing the path', async () => {
    const storage = memoryStorage()
    const open = vi.fn(async () => { throw new Error('access denied') })
    await restoreProject(storage, open)
    expect(open).not.toHaveBeenCalled()
    rememberProject(storage, 'X:/missing')
    await expect(restoreProject(storage, open)).rejects.toThrow('access denied')
    await expect(restoreProject(storage, open)).rejects.toThrow('access denied')
    expect(open).toHaveBeenLastCalledWith('X:/missing')
  })
  it.each(['Commit', 'Push'])('%s reloads authoritative Git state before reporting success', async (operation) => {
    const events: string[] = []
    const result = await mutateAndRefresh(async () => { events.push(operation); return 'real result' }, async () => { events.push('status/upstream/branches') })
    expect(events).toEqual([operation, 'status/upstream/branches'])
    expect(result).toBe('real result')
  })
  it('propagates Commit failure without returning success', async () => {
    const refresh = vi.fn(async () => {})
    await expect(mutateAndRefresh(async () => { throw new Error('Commit hook failed') }, refresh)).rejects.toThrow('Commit hook failed')
    expect(refresh).not.toHaveBeenCalled()
  })
  it('does not report success if refreshed Git state is unavailable', async () => {
    await expect(mutateAndRefresh(async () => 'hash', async () => { throw new Error('status failed') })).rejects.toThrow('status failed')
  })
})
