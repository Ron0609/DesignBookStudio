export type SearchMatch = { path: string; kind: string; excerpt: string; line: number; column: number; length: number; excerpt_offset: number; query: string }
export type SearchResults = { matches: SearchMatch[]; truncated: boolean; skipped: string[] }

export function selectionForHit(content: string, hit: SearchMatch): { start: number; end: number } | null {
  if (hit.line < 1 || hit.column < 1 || hit.length < 1) return null
  const lines = content.split('\n')
  const line = lines[hit.line - 1]?.replace(/\r$/, '')
  if (line === undefined || hit.column - 1 + hit.length > line.length) return null
  const start = lines.slice(0, hit.line - 1).reduce((total, previous) => total + previous.length + 1, 0) + hit.column - 1
  if (content.slice(start, start + hit.length).toLowerCase() !== hit.query.toLowerCase()) return null
  return { start, end: start + hit.length }
}

export function splitHighlight(text: string, start: number, length: number) {
  return { before: text.slice(0, start), hit: text.slice(start, start + length), after: text.slice(start + length) }
}

export function acceptEditorUpdate(editable: boolean, initialized: boolean, value: string, onChange: (text: string) => void) {
  if (editable && initialized) onChange(value)
}

export function preserveOriginalMarkdown(value: string, canonicalInitial: string, original: string): string {
  return value === canonicalInitial ? original : value
}
