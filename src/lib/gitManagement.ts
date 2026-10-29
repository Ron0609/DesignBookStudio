export type ManagedBranch = {
  name: string
  current: boolean
  remote: boolean
  upstream: string | null
  ahead: number
  behind: number
}

export type PushStatus = {
  branch: string
  changed: string[]
  ahead: number
  behind: number
}

export function remoteBranchChoices(branches: ManagedBranch[], remote: string): string[] {
  if (!remote) return []
  const prefix = `${remote}/`
  return branches
    .filter((branch) => branch.remote && branch.name.startsWith(prefix) && branch.name !== `${remote}/HEAD`)
    .map((branch) => branch.name.slice(prefix.length))
}

export function pushActionDisabled(status: PushStatus, remote: string, branch: string): boolean {
  return !remote || !branch.trim() || !status.branch
}

export type PushAssessment = { current_branch: string; remote: string; remote_branch: string; ready: boolean; message: string }

export function pushAssessmentReason(preview: PushAssessment | null, source: string, remote: string, branch: string): string {
  if (!preview) return 'リモート履歴は未検証です。「送信先を安全確認」を押してください。'
  if (preview.current_branch !== source || preview.remote !== remote || preview.remote_branch !== branch.trim()) {
    return '送信元または送信先が変わりました。安全確認をやり直してください。'
  }
  return preview.ready ? '' : preview.message
}

export function pushDisabledReason(status: PushStatus, remote: string, branch: string, options: {
  busy: boolean; dirty: boolean; externalChanged: boolean; source: string
}): string {
  if (options.busy) return 'Git 情報を読み込み中、または操作を実行中です。'
  if (options.dirty) return 'エディターに未保存の編集があります。先に保存してください。'
  if (options.externalChanged) return '外部でファイルが変更されています。内容を確認してください。'
  if (!status.branch) return '現在のローカルブランチがありません（detached HEAD）。'
  if (options.source !== status.branch) return '送信元に選択したローカルブランチへ切り替えてください。'
  if (!remote) return '送信先 remote を選択してください。'
  if (!branch.trim()) return '送信先ブランチを選択または入力してください。'
  return ''
}

export function pushWorktreeWarning(status: PushStatus): string {
  return status.changed.length ? `未コミット変更 ${status.changed.length} 件は送信されません。作業ファイルとステージ内容を保持し、既存コミットのみを Push します。` : ''
}

export function pushConfirmation(status: PushStatus, remote: string, branch: string): string {
  return `${status.branch} の既存コミットを ${remote}/${branch} に Push します。未コミット変更 ${status.changed.length} 件、ahead ${status.ahead} / behind ${status.behind}。${pushWorktreeWarning(status)} force push は行いません。送信対象はこのブランチの未送信コミット全体です。続行しますか？`
}

export function maskRemoteUrl(url: string): string {
  return url
    .replace(/(\b[a-z][a-z0-9+.-]*:\/\/)[^/@\s]+@/gi, '$1***@')
    .replace(/([?&])([^=&#\s]+)=([^&#\s]*)/g, '$1$2=***')
    .replace(/([?&])([^=&#\s]+)(?=(&|#|$|\s))/g, '$1***')
}

export function defaultPushTarget(branches: ManagedBranch[], remotes: string[]): { remote: string; branch: string } {
  const current = branches.find((branch) => !branch.remote && branch.current)
  const remote = [...remotes].sort((a, b) => b.length - a.length).find((name) => current?.upstream?.startsWith(`${name}/`)) ?? remotes[0] ?? ''
  return { remote, branch: current?.upstream?.startsWith(`${remote}/`) ? current.upstream.slice(remote.length + 1) : current?.name ?? '' }
}
