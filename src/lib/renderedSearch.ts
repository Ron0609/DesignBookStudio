import { selectionForHit, type SearchMatch } from './documentSearch'

export type RenderedCandidate = { text: string; context: string; offset: number; length: number }

export function normalizeReadingText(text: string): string {
  return text.replace(/!?\[([^\]]*)\]\([^)]*\)/g, '$1').replace(/[`*_~#>|]/g, ' ').replace(/\s+/g, ' ').trim().toLowerCase()
}

export function literalMatches(text: string, query: string): { offset: number; length: number }[] {
  if (!query) return []
  const pattern = new RegExp(query.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), 'giu')
  return Array.from(text.matchAll(pattern), (match) => ({ offset: match.index, length: match[0].length }))
}

export function selectRenderedCandidate(content: string, hit: SearchMatch, candidates: RenderedCandidate[]): number {
  if (!selectionForHit(content, hit) || !candidates.length) return -1
  const lines = content.split('\n')
  const line = lines[hit.line - 1]
  const context = normalizeReadingText(line)
  const prefix = normalizeReadingText(line.slice(0, hit.column - 1))
  const suffix = normalizeReadingText(line.slice(hit.column - 1 + hit.length))
  const scores = candidates.map((candidate) => {
    const block = normalizeReadingText(candidate.context)
    let score = context === block ? 1000 : block && (context.includes(block) || block.includes(context)) ? 500 : 0
    const before = normalizeReadingText(candidate.text.slice(0, candidate.offset))
    const after = normalizeReadingText(candidate.text.slice(candidate.offset + candidate.length))
    for (let i = 1; i <= Math.min(80, before.length, prefix.length); i++) { if (before.at(-i) !== prefix.at(-i)) break; score += 2 }
    for (let i = 0; i < Math.min(80, after.length, suffix.length); i++) { if (after[i] !== suffix[i]) break; score += 2 }
    // Context outside the hit distinguishes repeated terms in different paragraphs/table rows.
    const words = context.match(/[\p{L}\p{N}]+/gu) ?? []
    for (const word of words) if (block.includes(word)) score += Math.min(word.length, 30)
    return score
  })
  const best = Math.max(...scores)
  const ties = scores.flatMap((score, index) => score === best ? [index] : [])
  const preceding = lines.slice(0, hit.line - 1).filter((previous) => normalizeReadingText(previous) === context)
    .reduce((count, previous) => count + literalMatches(previous, hit.query).length, 0)
  const sameLine = literalMatches(line.slice(0, hit.column - 1), hit.query).length
  return ties[Math.min(preceding + sameLine, ties.length - 1)]
}

// Highlight DOM Ranges rather than inserting <mark> into the ProseMirror document.
export function revealRenderedHit(root: HTMLElement, content: string, hit: SearchMatch): (() => void) | null {
  if (!selectionForHit(content, hit)) return null
  const documentRoot = root.querySelector<HTMLElement>('.ProseMirror') ?? root
  const blocks = new Map<HTMLElement, Text[]>()
  const walker = document.createTreeWalker(documentRoot, NodeFilter.SHOW_TEXT)
  let node: Node | null
  while ((node = walker.nextNode())) {
    const parent = node.parentElement
    if (!parent || parent.closest('button,script,style,textarea,[aria-hidden="true"],.cm-gutters,.drag-preview')) continue
    const block = parent.closest<HTMLElement>('p,h1,h2,h3,h4,h5,h6,pre,li,td,th') ?? documentRoot
    const texts = blocks.get(block) ?? []
    texts.push(node as Text); blocks.set(block, texts)
  }
  const candidates: (RenderedCandidate & { block: HTMLElement; nodes: Text[] })[] = []
  for (const [block, nodes] of blocks) {
    const text = nodes.map((node) => node.data).join('')
    const row = block.closest('tr')
    const context = row ? [...row.querySelectorAll('th,td')].map((cell) => cell.textContent ?? '').join(' ') : text
    for (const match of literalMatches(text, hit.query)) candidates.push({ text, context, ...match, block, nodes })
  }
  const index = selectRenderedCandidate(content, hit, candidates)
  if (index < 0) return null
  const target = candidates[index]
  const range = document.createRange()
  let offset = 0
  for (const node of target.nodes) {
    if (target.offset >= offset && target.offset < offset + node.length) range.setStart(node, target.offset - offset)
    const end = target.offset + target.length
    if (end > offset && end <= offset + node.length) { range.setEnd(node, end - offset); break }
    offset += node.length
  }
  target.block.scrollIntoView({ block: 'center', inline: 'nearest' })
  if (typeof CSS !== 'undefined' && CSS.highlights && typeof Highlight !== 'undefined') {
    CSS.highlights.set('designbook-search-result', new Highlight(range))
    return () => { CSS.highlights.delete('designbook-search-result') }
  }
  const selection = window.getSelection()
  selection?.removeAllRanges(); selection?.addRange(range)
  return () => { if (selection?.rangeCount && selection.getRangeAt(0) === range) selection.removeAllRanges() }
}

export function observeRenderedHit(root: HTMLElement, content: string, hit: SearchMatch, onMissing: () => void): () => void {
  let cleanup: (() => void) | null = null
  let frame = 0
  let disposed = false
  const attempt = () => {
    if (disposed) return
    cleanup?.()
    cleanup = revealRenderedHit(root, content, hit)
  }
  // Table/code node views can finish mounting after Crepe.create() resolves.
  const observer = new MutationObserver(() => {
    cancelAnimationFrame(frame)
    frame = requestAnimationFrame(attempt)
  })
  observer.observe(root, { childList: true, subtree: true, characterData: true })
  frame = requestAnimationFrame(attempt)
  const timer = window.setTimeout(() => {
    observer.disconnect()
    cancelAnimationFrame(frame)
    attempt()
    if (!cleanup) onMissing()
  }, 2000)
  return () => { disposed = true; observer.disconnect(); cancelAnimationFrame(frame); window.clearTimeout(timer); cleanup?.() }
}
