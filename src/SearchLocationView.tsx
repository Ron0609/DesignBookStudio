import { useEffect, useLayoutEffect, useRef } from 'react'
import { selectionForHit, splitHighlight, type SearchMatch } from './lib/documentSearch'

export function SearchLocationView({ content, hit, editable, onChange, onClose }: {
  content: string; hit: SearchMatch; editable: boolean; onChange: (text: string) => void; onClose: () => void
}) {
  const markedLine = useRef<HTMLDivElement>(null)
  const source = useRef<HTMLTextAreaElement>(null)
  const latestContent = useRef(content)
  useLayoutEffect(() => { latestContent.current = content }, [content])
  useEffect(() => {
    if (!editable) { markedLine.current?.scrollIntoView({ block: 'center' }); return }
    const selection = selectionForHit(latestContent.current, hit)
    if (source.current && selection) {
      source.current.focus()
      source.current.setSelectionRange(selection.start, selection.end)
      source.current.scrollTop = Math.max(0, (hit.line - 5) * 22)
    }
    // Position only on entering this hit/mode; do not reset the caret on every keystroke.
  }, [hit, editable])
  const valid = selectionForHit(content, hit) !== null
  return <section className="search-location-view" aria-label="検索位置（原文）">
    <div className="search-location-header"><span>原文 · 行 {hit.line}、列 {hit.column}{editable ? ' · 編集中' : ' · 閲覧専用'}</span><button onClick={onClose}>文書表示へ戻る</button></div>
    {!valid && !editable && <p role="status">検索後にファイルが変わりました。再検索してください。</p>}
    {editable ? <textarea ref={source} className="search-source-editor" aria-label="原文を編集" value={content} spellCheck={false} onChange={(event) => onChange(event.target.value)} /> :
      <div className="search-source-lines">{content.split('\n').map((line, index) => {
        const selected = valid && index + 1 === hit.line
        const parts = splitHighlight(line, hit.column - 1, hit.length)
        return <div key={index} ref={selected ? markedLine : undefined} className={`search-source-line ${selected ? 'matched-line' : ''}`}>
          <span className="source-line-number">{index + 1}</span><span className="source-line-content">{selected ? <>{parts.before}<mark>{parts.hit}</mark>{parts.after}</> : line || ' '}</span>
        </div>
      })}</div>}
  </section>
}
