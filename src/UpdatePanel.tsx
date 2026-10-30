import { useState } from 'react'
import { updateCounts, type ChangeFile, type ChangeHunk, type UpdateRecord } from './lib/workspace'

export function UpdatePanel({ records, busy, status, onSync, onRead, onNavigate, popup, onDismiss }: {
  records: UpdateRecord[]; busy: boolean; status: string;
  onSync: () => void; onRead: (id?: string) => void;
  onNavigate: (record: UpdateRecord, file: ChangeFile, hunk: ChangeHunk | null) => void;
  popup: string | null; onDismiss: () => void;
}) {
  const [collapsed, setCollapsed] = useState(false)
  const [selected, setSelected] = useState('')
  const [shownDiff, setShownDiff] = useState('')
  const applied = records.filter((record) => record.applied).slice().reverse()
  const active = applied.find((record) => record.id === selected) ?? applied[0]
  const notification = applied.find((record) => record.id === popup)
  const counts = notification ? updateCounts(notification) : null
  return <section className="github-updates" aria-label="GitHub 更新履歴">
    <div className="github-updates-heading"><button aria-expanded={!collapsed} onClick={() => setCollapsed(!collapsed)}><span>更新履歴 / 変更箇所 {applied.filter((r) => !r.read).length > 0 && `(${applied.filter((r) => !r.read).length} 未読)`}</span><span aria-hidden="true">{collapsed ? '▸' : '▾'}</span></button></div>
    {!collapsed && <><p role="status">{status || 'GitHub の main と比較します。'}</p><div className="github-update-actions"><button disabled={busy} onClick={onSync}>{busy ? '確認中…' : '更新を確認'}</button><button disabled={busy || !applied.some((r) => !r.read)} onClick={() => onRead()}>すべて既読</button></div>
      <label>同期記録<select aria-label="同期記録" value={active?.id ?? ''} onChange={(event) => setSelected(event.target.value)}>{applied.map((record) => <option key={record.id} value={record.id}>{record.read ? '既読' : '未読'} · {new Date(record.timestamp * 1000).toLocaleString('ja-JP')}</option>)}</select></label>
      {!active && <p>設計書の同期更新はまだありません。</p>}
      {active && <><small className="update-hashes">{active.before} → {active.after}</small><button disabled={busy || active.read} onClick={() => onRead(active.id)}>この更新を既読</button><div className="update-file-list">{active.files.map((file) => <details key={file.path} open><summary>{file.status.startsWith('A') ? '＋追加' : file.status.startsWith('D') ? '－削除' : file.status.startsWith('R') ? '↪名前変更' : '～変更'} {file.path}{file.previous_path && <small>旧: {file.previous_path}</small>}</summary><button disabled={busy} onClick={() => onNavigate(active, file, null)}>変更前後の差分</button>{file.hunks.map((hunk, index) => <button disabled={busy} key={index} onClick={() => { setShownDiff(file.path); onNavigate(active, file, hunk) }}>{hunk.removed ? '－削除内容の差分' : `→ ${hunk.heading}`}<small>{hunk.text}</small></button>)}{shownDiff === file.path && <pre className="update-inline-diff" aria-label="変更前後の差分">{file.patch}</pre>}</details>)}</div></>}
    </>}
    {notification && counts && <div className="modal-backdrop" role="presentation"><section className="history-modal update-notification" role="dialog" aria-modal="true" aria-label="設計書が更新されました"><header><h2>設計書が更新されました</h2><button onClick={onDismiss}>×</button></header><p>追加 {counts.added} / 変更 {counts.modified} / 削除 {counts.deleted} / 名前変更 {counts.renamed}</p><small>{notification.before} → {notification.after}</small><ul>{notification.files.map((file) => <li key={file.path}>[{file.status}] {file.path}</li>)}</ul><footer><button onClick={() => { setSelected(notification.id); setCollapsed(false); onRead(notification.id); onDismiss() }}>更新を見る</button><button onClick={onDismiss}>後で見る</button></footer></section></div>}
  </section>
}
