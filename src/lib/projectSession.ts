const lastProjectKey = 'designbook.lastUnityProject.v1'
type ProjectStorage = Pick<Storage, 'getItem' | 'setItem'>

export type DocFile = { path: string; kind: 'markdown' | 'json' | 'other' }
export type GitState = { branch: string; changed: string[]; document_changed: string[]; other_changed: string[]; changes: { path: string; previous_path: string | null; status: string }[]; ahead: number; behind: number; clean: boolean; has_upstream: boolean; tracking_error: string | null }
export type ProjectInfo = { unity_root: string; git_root: string; documents_path: string | null; has_documents: boolean }
export type SessionInvoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>

// Shared by manual selection and startup restoration: read the files and Git anew.
export async function loadProjectSnapshot(path: string, invoke: SessionInvoke) {
  const project = await invoke<ProjectInfo>('open_repository', { path })
  const [files, status, directories] = await Promise.all([
    invoke<DocFile[]>('list_documents'), invoke<GitState>('git_status'), invoke<string[]>('list_document_directories'),
  ])
  const first = files.find((file) => file.kind === 'markdown' || file.kind === 'json')
  const firstContent = first ? await invoke<string>('read_document', { path: first.path }) : ''
  return { project, files, status, directories, first, firstContent }
}

export function rememberProject(storage: ProjectStorage, path: string) {
  storage.setItem(lastProjectKey, path)
}

export async function restoreProject(storage: ProjectStorage, openProject: (path: string) => Promise<void>) {
  const path = storage.getItem(lastProjectKey)
  if (path) await openProject(path)
}

// A successful mutation must be followed by fresh repository state, never a local count adjustment.
export async function mutateAndRefresh<T>(mutate: () => Promise<T>, refresh: () => Promise<void>): Promise<T> {
  const result = await mutate()
  try { await refresh() }
  catch (error) { throw new Error(`${String(result)}\n操作は完了しましたが、最新の Git 状態を読み込めませんでした: ${String(error)}`) }
  return result
}
