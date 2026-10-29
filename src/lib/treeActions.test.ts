import { describe, expect, it } from 'vitest'
import { canDropEntry, containsEntry, parentDirectory, relocatedPath, entryProtectionReason } from './treeActions'
import tauriConfig from '../../src-tauri/tauri.conf.json'

describe('file tree actions', () => {
  it('allows HTML drag and drop in the Windows webview', () => {
    expect(tauriConfig.app.windows[0].dragDropEnabled).toBe(false)
  })
  it('moves documents between selected folders and remaps open descendants', () => {
    expect(parentDirectory('DesignDocs/A/a.md')).toBe('DesignDocs/A')
    expect(canDropEntry('DesignDocs/A/a.md', 'DesignDocs/B')).toBe(true)
    expect(relocatedPath('DesignDocs/A/a.md', 'DesignDocs/A', 'DesignDocs/B/A')).toBe('DesignDocs/B/A/a.md')
    expect(relocatedPath('DesignDocs/AB/a.md', 'DesignDocs/A', 'DesignDocs/B/A')).toBe('DesignDocs/AB/a.md')
  })
  it('rejects drops into self, descendants and the unchanged parent', () => {
    expect(canDropEntry('DesignDocs/A', 'DesignDocs/A')).toBe(false)
    expect(canDropEntry('DesignDocs/A', 'DesignDocs/A/sub')).toBe(false)
    expect(canDropEntry('DesignDocs/A/a.md', 'DesignDocs/A')).toBe(false)
    expect(canDropEntry('', 'DesignDocs/B')).toBe(false)
    expect(containsEntry('DesignDocs/A', 'DesignDocs/AB/a.md')).toBe(false)
  })
  it('protects unsaved and externally changed editors when mutating their file or ancestor', () => {
    expect(entryProtectionReason('DesignDocs/A', 'DesignDocs/A/a.md', true, false)).toContain('未保存')
    expect(entryProtectionReason('DesignDocs/A/a.md', 'DesignDocs/A/a.md', false, true)).toContain('外部変更')
    expect(entryProtectionReason('DesignDocs/B', 'DesignDocs/A/a.md', true, false)).toBe('')
    expect(entryProtectionReason('DesignDocs/A', 'DesignDocs/A/a.md', false, false)).toBe('')
  })
})
