import { describe, expect, it, vi } from 'vitest'
import { renderToStaticMarkup } from 'react-dom/server'
import { acceptEditorUpdate, preserveOriginalMarkdown, selectionForHit, splitHighlight, type SearchMatch } from './documentSearch'
import { SearchLocationView } from '../SearchLocationView'

const hit: SearchMatch = { path: 'DesignDocs/a.md', kind: 'markdown', line: 2, column: 4, length: 4, query: '修炼属性', excerpt: '😀 修炼属性', excerpt_offset: 3 }

describe('read-only documents and precise search navigation', () => {
  it('preserves original formatting when entering edit mode without changing content or undoing to the original', () => {
    expect(preserveOriginalMarkdown('| a | b |\n', '| a | b |\n', '|a|b|')).toBe('|a|b|')
    expect(preserveOriginalMarkdown('changed', '| a | b |\n', '|a|b|')).toBe('changed')
  })
  it('ignores read-only and initialization updates, and accepts only explicit editing', () => {
    const onChange = vi.fn()
    acceptEditorUpdate(false, true, 'accidental click', onChange)
    acceptEditorUpdate(true, false, 'initial markdown normalization', onChange)
    expect(onChange).not.toHaveBeenCalled()
    acceptEditorUpdate(true, true, 'intentional edit', onChange)
    acceptEditorUpdate(false, true, 'after leaving edit mode', onChange)
    expect(onChange).toHaveBeenCalledExactlyOnceWith('intentional edit')
  })
  it('maps UTF-16 columns and CRLF lines to the exact text selection', () => {
    const text = '# 概要\r\n😀 修炼属性 与 修炼属性\r\nend'
    const selection = selectionForHit(text, hit)!
    expect(text.slice(selection.start, selection.end)).toBe('修炼属性')
    const second = selectionForHit(text, { ...hit, column: 11 })!
    expect(second.start).toBeGreaterThan(selection.end)
    expect(text.slice(second.start, second.end)).toBe('修炼属性')
  })
  it('rejects stale or invalid results rather than jumping to unrelated content', () => {
    expect(selectionForHit('different content', hit)).toBeNull()
    expect(selectionForHit('修炼属性', { ...hit, line: 0 })).toBeNull()
    expect(selectionForHit('修炼属性', { ...hit, line: 1, column: 20 })).toBeNull()
    expect(selectionForHit('CULTIVATION', { ...hit, line: 1, column: 1, length: 11, query: 'cultivation' })).toEqual({ start: 0, end: 11 })
  })
  it('highlights the selected occurrence safely in the read-only line view', () => {
    const content = '# 概要\n😀 修炼属性 <script>alert(1)</script>'
    const html = renderToStaticMarkup(<SearchLocationView content={content} hit={hit} editable={false} onChange={() => {}} onClose={() => {}} />)
    expect(html).toContain('<mark>修炼属性</mark>')
    expect(html).toContain('閲覧専用')
    expect(html).not.toContain('<textarea')
    expect(html).not.toContain('<script>')
    expect(splitHighlight('😀 修炼属性', 3, 4)).toEqual({ before: '😀 ', hit: '修炼属性', after: '' })
  })
  it('shows a source editor only after entering edit mode', () => {
    const html = renderToStaticMarkup(<SearchLocationView content={'# 概要\n😀 修炼属性'} hit={hit} editable onChange={() => {}} onClose={() => {}} />)
    expect(html).toContain('<textarea')
    expect(html).toContain('原文を編集')
  })
})
