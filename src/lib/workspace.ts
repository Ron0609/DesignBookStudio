import type { SessionInvoke } from './projectSession'
import type { SearchMatch } from './documentSearch'

export type WorkspacePreview = { unity_root: string; git_root: string; worktree: string; development_branch: string; head: string; upstream: string | null; existing: boolean; blockers: string[]; source_changes: string[]; staged: string[]; document_changes: string[]; unpushed_documents: string[]; worktree_changes: string[]; sparse_ready: boolean; sparse_scope: string }
export type ChangeHunk = { line: number; heading: string; removed: boolean; text: string }
export type ChangeFile = { path: string; previous_path: string | null; status: string; hunks: ChangeHunk[]; patch: string }
export type UpdateRecord = { id: string; before: string; after: string; timestamp: number; read: boolean; applied: boolean; files: ChangeFile[] }
export type SyncResult = { message: string; records: UpdateRecord[]; new_id: string | null }

export async function prepareWorkspace(path: string, invoke: SessionInvoke, confirm: (message: string) => Promise<boolean>) {
  const preview = await invoke<WorkspacePreview>('preview_workspace', { path })
  if (preview.blockers.length) throw new Error(`${preview.blockers.join('\n')}\n${preview.document_changes.join('\n')}\n${preview.unpushed_documents.join('\n')}`)
  if (!preview.existing || preview.sparse_ready === false) {
    const approved = await confirm(`${preview.existing ? "既存 Worktree を DesignDocs 専用 Sparse Checkout に変換しますか？" : "設計書用の独立 Sparse Worktree を作成しますか？"}\nUnity: ${preview.unity_root}\n開発ブランチ: ${preview.development_branch}\n設計書ブランチ: main\n作成先: ${preview.worktree}\n元の未コミット変更: ${preview.source_changes.length} 件 / ステージ済み: ${preview.staged.length} 件\n検出範囲: ${preview.sparse_scope || "DesignDocs"} のみ。Assets・Packages・ProjectSettings は専用側に展開しません。${preview.existing ? "専用側の変更のない追跡ファイルを対象外から非表示にします。未コミット・無視・未追跡ファイルがあれば停止します。" : "Git 管理情報に Worktree を登録します。"} Unity 側のファイルとブランチは変更しません。`)
    if (!approved) return false
    await invoke('create_workspace', { path, confirmed: true })
  }
  return true
}

export function updateCounts(record: UpdateRecord) {
  const counts = { added: 0, modified: 0, deleted: 0, renamed: 0 }
  for (const file of record.files) {
    if (file.status.startsWith('A')) counts.added++
    else if (file.status.startsWith('D')) counts.deleted++
    else if (file.status.startsWith('R')) counts.renamed++
    else counts.modified++
  }
  return counts
}

export function updateDocumentPath(file: ChangeFile, scope: string) {
  if (!file.path.startsWith(`${scope}/`)) return null
  const path = `DesignDocs/${file.path.slice(scope.length + 1)}`
  if (path.split('/').some((part) => part === '..' || part === '.' || !part)) return null
  return /\.(md|markdown|json|mmd)$/i.test(path) ? path : null
}

export function updateHit(content: string, path: string, hunk: ChangeHunk): SearchMatch | null {
  if (hunk.removed || hunk.line < 1) return null
  const line = content.split('\n')[hunk.line - 1]?.replace(/\r$/, '')
  if (!line) return null
  const query = line.match(/[\p{L}\p{N}][\p{L}\p{N}\p{Zs}]{0,59}/u)?.[0]?.trim()
  if (!query) return null
  return { path, kind: path.endsWith('.json') ? 'json' : 'markdown', line: hunk.line, column: line.indexOf(query) + 1, length: query.length, excerpt: line, excerpt_offset: line.indexOf(query), query }
}
