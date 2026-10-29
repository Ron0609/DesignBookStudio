import { describe, expect, it } from 'vitest'
import { literalMatches, normalizeReadingText, selectRenderedCandidate } from './renderedSearch'
import type { SearchMatch } from './documentSearch'

const base: SearchMatch = { path: 'DesignDocs/a.md', kind: 'markdown', excerpt: '', excerpt_offset: 0, line: 1, column: 1, length: 4, query: '修炼属性' }

describe('rendered reading-mode search navigation', () => {
  it('selects the corresponding rendered table row, retaining visible code/link text', () => {
    const source = '| 修炼属性 | 基础属性 |\n| `docs/修炼属性.md` | 修炼公式、速度 |'
    const hit = { ...base, line: 2, column: source.split('\n')[1].indexOf('修炼属性') + 1 }
    const candidates = [
      { text: '修炼属性', context: '修炼属性 基础属性', offset: 0, length: 4 },
      { text: 'docs/修炼属性.md', context: 'docs/修炼属性.md 修炼公式、速度', offset: 5, length: 4 },
    ]
    expect(selectRenderedCandidate(source, hit, candidates)).toBe(1)
    expect(normalizeReadingText('| **标题** | [说明](target.md) |')).toBe('标题 说明')
  })
  it('locates a later occurrence inside the same paragraph', () => {
    const source = '修炼属性 增长之后 修炼属性'
    const candidates = literalMatches(source, '修炼属性').map((match) => ({ text: source, context: source, ...match }))
    expect(selectRenderedCandidate(source, { ...base, column: source.lastIndexOf('修炼属性') + 1 }, candidates)).toBe(1)
  })
  it('distinguishes identical paragraphs by their source occurrence', () => {
    const source = '修炼属性\n\n修炼属性'
    const candidates = Array(2).fill({ text: '修炼属性', context: '修炼属性', offset: 0, length: 4 })
    expect(selectRenderedCandidate(source, { ...base, line: 3 }, candidates)).toBe(1)
  })
  it('uses literal Unicode-aware ranges and declines stale or invisible matches', () => {
    expect(literalMatches('😀 A.B a.b axb', 'a.b')).toEqual([{ offset: 3, length: 3 }, { offset: 7, length: 3 }])
    expect(selectRenderedCandidate('changed', base, [])).toBe(-1)
    expect(selectRenderedCandidate('修炼属性', base, [])).toBe(-1)
  })
})
