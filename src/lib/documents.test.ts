import { describe, expect, it } from 'vitest'
import { findDocumentMatches, parseFields, validateStructuredDocument } from './documents'

describe('structured document utilities', () => {
  it('reads stable field ids from a fields document', () => {
    expect(parseFields('{"fields":[{"id":"FLD-001"}]}')).toEqual([{ id: 'FLD-001' }])
  })

  it('detects duplicate field ids and invalid field types', () => {
    const issues = validateStructuredDocument(JSON.stringify({ fields: [
      { id: 'FLD-001', name: 'one', type: 'string' },
      { id: 'FLD-001', name: 'two', type: 'float' },
    ] }))
    expect(issues).toContain('Duplicate field id: FLD-001')
    expect(issues).toContain('Field 2 has an unsupported type')
  })

  it('rejects defaults whose JSON values do not match the declared field type', () => {
    const issues = validateStructuredDocument(JSON.stringify({ fields: [
      { id: 'FLD-BOOL', name: 'enabled', type: 'boolean', defaultValue: 'true' },
    ] }))
    expect(issues).toContain('Field 1 has a default value that does not match its type')
  })

  it('rejects a non-array fields property', () => {
    expect(validateStructuredDocument('{"fields":"not-an-array"}')).toContain('fields must be an array')
  })

  it('does not modify unknown Markdown content while searching', () => {
    const document = '| a | b |\n| --- | --- |\n| `x` | [link](./other.md) |\n\n```unknown\nkept\n```'
    expect(findDocumentMatches([{ path: 'DesignDocs/page.md', kind: 'markdown' }], { 'DesignDocs/page.md': document }, 'unknown')).toHaveLength(1)
    expect(document.endsWith('```unknown\nkept\n```')).toBe(true)
  })
})
