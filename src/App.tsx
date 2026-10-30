import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'
import { invoke, isTauri } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { confirm as confirmDialog, open } from '@tauri-apps/plugin-dialog'
import { Crepe } from '@milkdown/crepe'
import { parseFields, validateStructuredDocument } from './lib/documents'
import { extractImageReferences, markdownImageLink } from './lib/markdown'
import { defaultPushTarget, maskRemoteUrl, pushActionDisabled, pushConfirmation, pushDisabledReason, pushAssessmentReason, pushWorktreeWarning, remoteBranchChoices } from './lib/gitManagement'
import { rememberProject, restoreProject, mutateAndRefresh, loadProjectSnapshot, type DocFile, type GitState, type ProjectInfo } from './lib/projectSession'
import { createCloseRequestHandler } from './lib/windowClose'
import { containsEntry, parentDirectory, relocatedPath, canDropEntry, entryProtectionReason } from './lib/treeActions'
import { acceptEditorUpdate, preserveOriginalMarkdown, selectionForHit, splitHighlight, type SearchMatch, type SearchResults } from './lib/documentSearch'
import { SearchLocationView } from './SearchLocationView'
import { observeRenderedHit } from './lib/renderedSearch'
import '@milkdown/crepe/theme/common/style.css'
import '@milkdown/crepe/theme/frame.css'
import { BookOpen, ChevronDown, ChevronRight, CircleHelp, Clock3, FileJson2, FileText, Folder, GitBranch, GitCommitHorizontal, GitCompareArrows, GitFork, HardDrive, History, Image, LayoutGrid, ListFilter, LoaderCircle, Plus, RefreshCw, Search, Settings2, ShieldCheck, Sparkles, Upload, X, CloudDownload, CloudUpload } from 'lucide-react'
import { UpdatePanel } from './UpdatePanel'
import { prepareWorkspace, updateDocumentPath, updateHit, type UpdateRecord, type ChangeFile, type ChangeHunk, type SyncResult } from './lib/workspace'
import './App.css'

type GitBranchInfo = { name: string; current: boolean; remote: boolean; upstream: string | null; ahead: number; behind: number }
type GitRemoteInfo = { name: string; url: string }
type GitPushPreview = { current_branch: string; remote: string; remote_branch: string; changed_count: number; ahead: number | null; behind: number | null; target_exists: boolean; ready: boolean; message: string }
type SetupPreview = { entries: { path: string; kind: 'directory' | 'file' }[]; changed_files: string[] }
const demoDocs = [
  { path: 'DesignDocs/README.md', kind: 'markdown' as const },
  { path: 'DesignDocs/00_総合設計/世界観と概要.md', kind: 'markdown' as const },
  { path: 'DesignDocs/02_処理仕様/修炼循环.md', kind: 'markdown' as const },
  { path: 'DesignDocs/03_項目仕様/character.json', kind: 'json' as const },
  { path: 'DesignDocs/05_世界観・ゲームルール/修仙境界.md', kind: 'markdown' as const },
  { path: 'DesignDocs/06_開発規約/ドキュメント規約.md', kind: 'markdown' as const },
]
const demoContent: Record<string, string> = {
  'DesignDocs/README.md': '# Not the Protagonist — 設計書\n\nこのプロジェクトの設計書を管理します。\n\n## はじめに\n\n左のナビゲーションから文書を選択してください。設計書の正本はこのリポジトリ内の Markdown と JSON です。\n\n> 変更はローカルに保存されます。コミットとプッシュは、ユーザーが操作した場合のみ実行されます。\n\n| 項目 | 内容 |\n| --- | --- |\n| エンジン | Unity 6 |\n| ジャンル | 3D 修仙シミュレーション RPG |\n\n```mermaid\nflowchart LR\n  Explore --> Cultivate --> Encounter\n  Encounter --> Explore\n```',
  'DesignDocs/00_総合設計/世界観と概要.md': '# 世界観と概要\n\n主人公ではない一人の修行者として、変化する世界で暮らします。\n\n## 体験の柱\n\n- 世界はプレイヤーの行動を待たずに進む\n- 修行と人間関係が長期的な選択をつくる\n- 物語はシステムの相互作用から生まれる',
  'DesignDocs/02_処理仕様/修炼循环.md': '# 修炼循环\n\n## 基本フロー\n\n1. 場所と行動を選ぶ\n2. 時間を進める\n3. 資源、状態、周囲の人物を更新する\n4. 発生した出来事を確認する\n\n```mermaid\nflowchart TD\n  A[Choose an activity] --> B[Advance time]\n  B --> C[Resolve world events]\n  C --> D[Review outcome]\n  D --> A\n```',
  'DesignDocs/03_項目仕様/character.json': JSON.stringify({ fields: [{ id: 'FLD-CULT-001', name: 'cultivation_speed', displayName: '修炼速度', type: 'number', defaultValue: 1, description: '角色基础修炼速度倍率', constraints: { minimum: 0 } }, { id: 'FLD-CHAR-002', name: 'display_name', displayName: '显示名称', type: 'string', defaultValue: '新角色', description: '角色的显示名称' }] }, null, 2),
  'DesignDocs/05_世界観・ゲームルール/修仙境界.md': '# 修仙境界\n\n境界は長期的な成長の目安です。\n\n- **炼气**：基礎を築く\n- **筑基**：身体と精神を整える\n- **金丹**：内なる力を安定させる',
  'DesignDocs/06_開発規約/ドキュメント規約.md': '# ドキュメント規約\n\n- Markdown を本文の正本として管理します。\n- 構造化フィールドには変更後も維持する `id` を付けます。\n- 不明な Mermaid や Markdown 構文を保持してから保存します。',
}

function Editor({ markdown, onChange, editable, onError, searchHit, onNotice, snapshotRef }: { markdown: string; onChange: (text: string) => void; editable: boolean; onError: (error: unknown) => void; searchHit: SearchMatch | null; onNotice: (message: string) => void; snapshotRef: { current: () => string | null } }) {
  const host = useRef<HTMLDivElement>(null)
  const latest = useRef({ onChange, onError, onNotice })
  const editing = useRef(editable)
  const editor = useRef<Crepe | null>(null)
  const initialMarkdown = useRef(markdown)
  const latestMarkdown = useRef(markdown)
  const initialized = useRef(false)
  const clearHighlight = useRef<() => void>(() => {})
  useLayoutEffect(() => { latest.current = { onChange, onError, onNotice }; editing.current = editable; latestMarkdown.current = markdown }, [onChange, onError, onNotice, editable, markdown])
  const navigate = useCallback(() => {
    clearHighlight.current(); clearHighlight.current = () => {}
    if (!initialized.current || !host.current || !searchHit || editable) return
    clearHighlight.current = observeRenderedHit(host.current, latestMarkdown.current, searchHit, () => {
      latest.current.onNotice('文書を閲覧表示で開きました。検索位置がリンク先 URL などの非表示部分にある場合は「原文を表示」で確認できます。')
    })
  }, [searchHit, editable])
  const navigation = useRef(navigate)
  useLayoutEffect(() => { navigation.current = navigate; navigate() }, [navigate])
  useEffect(() => {
    if (!host.current) return
    const instance = new Crepe({ root: host.current, defaultValue: initialMarkdown.current, features: { [Crepe.Feature.Placeholder]: true } })
    editor.current = instance
    instance.setReadonly(!editing.current)
    let ready = false
    let cancelled = false
    let canonicalInitial = initialMarkdown.current
    instance.on((listener) => listener.markdownUpdated((_, value) => acceptEditorUpdate(editing.current, ready, preserveOriginalMarkdown(value, canonicalInitial, initialMarkdown.current), latest.current.onChange)))
    const created = instance.create()
    void created.then(() => { if (!cancelled) { canonicalInitial = instance.getMarkdown(); snapshotRef.current = () => preserveOriginalMarkdown(instance.getMarkdown(), canonicalInitial, initialMarkdown.current); instance.setReadonly(!editing.current); ready = true; initialized.current = true; navigation.current() } }).catch((error) => { if (!cancelled) latest.current.onError(error) })
    return () => { ready = false; initialized.current = false; cancelled = true; snapshotRef.current = () => null; clearHighlight.current(); if (editor.current === instance) editor.current = null; void created.then(() => instance.destroy()).catch(() => {}) }
  }, [snapshotRef])
  useLayoutEffect(() => { editor.current?.setReadonly(!editable) }, [editable])
  return <><div className="milkdown-host" ref={host} onBlurCapture={() => { if (editing.current && initialized.current) { const value = snapshotRef.current(); if (value !== null) latest.current.onChange(value) } }} /><MermaidPreview markdown={markdown} /></>
}

function MermaidPreview({ markdown }: { markdown: string }) {
  const host = useRef<HTMLDivElement>(null)
  const diagrams = useMemo(() => Array.from(markdown.matchAll(/```mermaid\s*\n([\s\S]*?)```/g), (match) => match[1].trim()), [markdown])
  useEffect(() => {
    if (!host.current || diagrams.length === 0) return
    let cancelled = false
    void import('mermaid').then(async ({ default: mermaid }) => {
      mermaid.initialize({ startOnLoad: false, securityLevel: 'strict', theme: 'neutral' })
      return Promise.all(diagrams.map((source, index) => mermaid.render(`designbook-diagram-${index}`, source)))
    }).then((items) => {
      if (cancelled || !host.current) return
      host.current.innerHTML = items.map((item) => `<div class="diagram-card">${item.svg}</div>`).join('')
    }).catch(() => { if (host.current) host.current.textContent = '図表を表示できません。Mermaid の構文を確認してください。' })
    return () => { cancelled = true }
  }, [diagrams])
  return diagrams.length ? <div className="mermaid-preview" ref={host} aria-label="Mermaid 図表プレビュー" /> : null
}

function App() {
  const [repo, setRepo] = useState('')
  const [projectInfo, setProjectInfo] = useState<ProjectInfo | null>(null)
  const [updates, setUpdates] = useState<UpdateRecord[]>([])
  const [updatePopup, setUpdatePopup] = useState<string | null>(null)
  const [syncStatus, setSyncStatus] = useState('')
  const syncing = useRef(false)
  const [docsReady, setDocsReady] = useState(false)
  const [docs, setDocs] = useState<DocFile[]>(demoDocs)
  const [current, setCurrent] = useState(demoDocs[0].path)
  const [content, setContent] = useState(demoContent[demoDocs[0].path])
  const [saved, setSaved] = useState(content)
  const [query, setQuery] = useState('')
  const [searchMatches, setSearchMatches] = useState<SearchMatch[]>([])
  const [searchPending, setSearchPending] = useState(false)
  const [searchError, setSearchError] = useState('')
  const [searchTruncated, setSearchTruncated] = useState(false)
  const [searchSkipped, setSearchSkipped] = useState<string[]>([])
  const [searchTarget, setSearchTarget] = useState<SearchMatch | null>(null)
  const [showSearchSource, setShowSearchSource] = useState(false)
  const [updateHighlight, setUpdateHighlight] = useState(false)
  useEffect(() => {
    if (!updateHighlight) return
    const timer = window.setTimeout(() => { setSearchTarget(null); setUpdateHighlight(false) }, 5000)
    return () => window.clearTimeout(timer)
  }, [updateHighlight, searchTarget])
  const jsonSource = useRef<HTMLTextAreaElement>(null)
  const [editing, setEditing] = useState(false)
  const readEditorSnapshot = useRef<() => string | null>(() => null)
  const [git, setGit] = useState<GitState>({ branch: '', changed: [], document_changed: [], other_changed: [], changes: [], ahead: 0, behind: 0, clean: true, has_upstream: false, tracking_error: null })
  const [notice, setNotice] = useState('サンプルを表示中 — Git リポジトリを開くと編集できます')
  const [busy, setBusy] = useState(false)
  const [mode, setMode] = useState<'write' | 'fields'>('write')
  const [panel, setPanel] = useState<'history' | 'diff' | null>(null)
  const [gitManagerOpen, setGitManagerOpen] = useState(false)
  const [gitBranches, setGitBranches] = useState<GitBranchInfo[]>([])
  const [gitRemotes, setGitRemotes] = useState<GitRemoteInfo[]>([])
  const [branchStart, setBranchStart] = useState('')
  const [newBranchName, setNewBranchName] = useState('')
  const [selectedLocalBranch, setSelectedLocalBranch] = useState('')
  const [selectedRemote, setSelectedRemote] = useState('')
  const [remoteNameInput, setRemoteNameInput] = useState('')
  const [remoteUrlInput, setRemoteUrlInput] = useState('')
  const [selectedRemoteBranch, setSelectedRemoteBranch] = useState('')
  const [pushBranch, setPushBranch] = useState('')
  const [commitMessage, setCommitMessage] = useState('')
  const [createPushBranch, setCreatePushBranch] = useState(false)
  const [pushSetUpstream, setPushSetUpstream] = useState(false)
  const [pushPreview, setPushPreview] = useState<GitPushPreview | null>(null)
  const [history, setHistory] = useState<string[]>([])
  const [diff, setDiff] = useState('')
  const [setupPreview, setSetupPreview] = useState<SetupPreview | null>(null)
  const [newItemKind, setNewItemKind] = useState<'document' | 'folder' | null>(null)
  const [newItemName, setNewItemName] = useState('')
  const [newItemParent, setNewItemParent] = useState('')
  const [newDocumentFormat, setNewDocumentFormat] = useState<'markdown' | 'json'>('markdown')
  const [documentDirectories, setDocumentDirectories] = useState<string[]>([])
  const [selectedDirectory, setSelectedDirectory] = useState('')
  const [treeMenu, setTreeMenu] = useState<{ path: string; directory: boolean; x: number; y: number } | null>(null)
  const [renamePath, setRenamePath] = useState<string | null>(null)
  const [renameName, setRenameName] = useState('')
  const [draggedPath, setDraggedPath] = useState('')
  const [dropDirectory, setDropDirectory] = useState('')
  const [validationIssues, setValidationIssues] = useState<string[]>([])
  const [missingImages, setMissingImages] = useState<string[]>([])
  const [externalChanged, setExternalChanged] = useState(false)
  const [editorRevision, setEditorRevision] = useState(0)
  const externalNoticeShown = useRef(false)
  const closeState = useRef({ dirty: false, saved: content })
  const restorationStarted = useRef(false)
  const dirty = content !== saved
  useLayoutEffect(() => { closeState.current = { dirty, saved } }, [dirty, saved])
  const isJson = current.endsWith('.json')
  useEffect(() => {
    if (!isJson || !searchTarget || editing || showSearchSource || !jsonSource.current) return
    const selection = selectionForHit(content, searchTarget)
    if (!selection) return
    jsonSource.current.focus()
    jsonSource.current.setSelectionRange(selection.start, selection.end)
    jsonSource.current.scrollTop = Math.max(0, (searchTarget.line - 5) * 19)
  }, [content, isJson, searchTarget, editing, showSearchSource, mode])
  const searchable = useMemo(() => query.trim() && isTauri() && repo ? docs.filter((doc) => searchMatches.some((hit) => hit.path === doc.path)) : docs, [docs, query, repo, searchMatches])
  const searchGroups = useMemo(() => {
    const groups = new Map<string, SearchMatch[]>()
    for (const hit of searchMatches) { const group = groups.get(hit.path) ?? []; group.push(hit); groups.set(hit.path, group) }
    return [...groups.entries()]
  }, [searchMatches])
  const documentRoot = documentDirectories[0] ?? 'DesignDocs'
  const treeDirectories = useMemo(() => {
    const available = documentDirectories.length ? documentDirectories : [...new Set(docs.map((file) => file.path.split('/').slice(0, -1).join('/')))]
    return available.filter((directory) => !query.trim() || searchable.some((file) => file.path.startsWith(`${directory}/`)))
      .sort((left, right) => left.split('/').length - right.split('/').length || left.localeCompare(right))
  }, [documentDirectories, docs, query, searchable])
  const filesInDirectory = (directory: string) => searchable.filter((file) => file.path.slice(0, file.path.lastIndexOf('/')) === directory)
  async function refresh(nextRepo = repo) {
    if (!isTauri() || !nextRepo) return
    const [files, status, directories, context] = await Promise.all([invoke<DocFile[]>('list_documents'), invoke<GitState>('git_status'), invoke<string[]>('list_document_directories'), invoke<ProjectInfo>('project_context')])
    setDocs(files); setGit(status); setDocumentDirectories(directories); setProjectInfo(context)
  }
  async function beginCreate(kind: 'document' | 'folder') {
    try {
      setBusy(true)
      const directories = await invoke<string[]>('list_document_directories')
      const root = directories[0] ?? 'DesignDocs'
      const currentParent = selectedDirectory || root
      if (!directories.includes(currentParent)) { setNotice('選択したフォルダーが存在しません。作成先を選び直してください。'); return }
      setDocumentDirectories(directories)
      setNewItemParent(directories.includes(currentParent) ? currentParent : root)
      setNewItemName('')
      setNewDocumentFormat('markdown')
      setNewItemKind(kind)
    } catch (error) { setNotice(`保存先フォルダーを読み込めません: ${String(error)}`) }
    finally { setBusy(false) }
  }
  async function createNewItem() {
    if (!newItemKind) return
    if (!newItemName.trim()) { setNotice('名前を入力してください'); return }
    try {
      setBusy(true)
      const createdPath = newItemKind === 'document'
        ? await invoke<string>('create_document', { parentPath: newItemParent, name: newItemName.trim(), format: newDocumentFormat })
        : await invoke<string>('create_document_folder', { parentPath: newItemParent, name: newItemName.trim() })
      const [files, status, directories] = await Promise.all([
        invoke<DocFile[]>('list_documents'), invoke<GitState>('git_status'), invoke<string[]>('list_document_directories'),
      ])
      setDocs(files); setGit(status); setDocumentDirectories(directories)
      setSelectedDirectory(newItemKind === 'folder' ? createdPath : newItemParent)
      if (newItemKind === 'document' && !dirty) {
        const createdContent = await invoke<string>('read_document', { path: createdPath })
        setCurrent(createdPath); setContent(createdContent); setSaved(createdContent); setEditorRevision((value) => value + 1)
        setEditing(false); setSearchTarget(null)
        setMode('write'); setValidationIssues([]); setMissingImages([])
      }
      setNotice(`${newItemKind === 'document' ? '文書' : 'フォルダー'}を作成しました: ${createdPath}`)
      setNewItemKind(null)
    } catch (error) { setNotice(`作成できませんでした: ${String(error)}`) }
    finally { setBusy(false) }
  }
  async function synchronizeWorkspace(project = projectInfo, openedPath = current, unsaved = dirty || externalChanged) {
    if (!project || syncing.current) return
    const pending = readEditorSnapshot.current()
    // Opening a new project has no editor buffer to protect. Later retries inspect the live editor too.
    const protect = unsaved || (project === projectInfo && pending !== null && pending !== saved)
    syncing.current = true
    setBusy(true); setSyncStatus('GitHub の更新を確認しています…')
    try {
      const result = await invoke<SyncResult>('sync_workspace', { unityRoot: project.unity_root, unsaved: protect })
      setUpdates(result.records); setUpdatePopup(result.new_id); setSyncStatus(result.message)
      if (!protect) {
        const [files, status, directories, context] = await Promise.all([invoke<DocFile[]>('list_documents'), invoke<GitState>('git_status'), invoke<string[]>('list_document_directories'), invoke<ProjectInfo>('project_context')])
        setDocs(files); setGit(status); setDocumentDirectories(directories); setProjectInfo(context)
        setDocsReady(directories.length > 0)
        const next = files.find((file) => file.path === openedPath) ?? files.find((file) => file.kind === 'markdown' || file.kind === 'json')
        const text = next ? await invoke<string>('read_document', { path: next.path }) : ''
        setCurrent(next?.path ?? ''); setContent(text); setSaved(text); setEditing(false); setExternalChanged(false)
        setSearchTarget(null); setSearchMatches([]); setQuery(''); setEditorRevision((value) => value + 1)
      }
    } catch (error) {
      setSyncStatus(`自動同期できません。ローカルの設計書を表示します。再試行してください: ${String(error)}`)
      try { setUpdates(await invoke<UpdateRecord[]>('update_records')) } catch { /* retain the explicit synchronization error */ }
    } finally { syncing.current = false; setBusy(false) }
  }
  // 保存前の編集を保護しつつ、一定間隔で main の受信だけを実行する。
  const autoSync = useRef(() => {})
  useLayoutEffect(() => { autoSync.current = () => { if (repo && !busy && !editing) void synchronizeWorkspace() } })
  useEffect(() => {
    if (!repo || !isTauri()) return
    const timer = window.setInterval(() => autoSync.current(), 60000)
    return () => window.clearInterval(timer)
  }, [repo])
  async function readUpdates(id?: string) {
    try { setUpdates(await invoke<UpdateRecord[]>('mark_updates_read', { id: id ?? null })) }
    catch (error) { setNotice(`既読状態を保存できません: ${String(error)}`) }
  }
  async function navigateUpdate(record: UpdateRecord, file: ChangeFile, hunk: ChangeHunk | null) {
    setDiff(file.patch); setPanel('diff')
    if (!hunk || hunk.removed || busy) return
    const pending = readEditorSnapshot.current()
    if (dirty || externalChanged || (pending !== null && pending !== saved)) { setNotice('未保存の編集を保護しました。差分だけを表示します。保存後に移動してください。'); return }
    const scope = projectInfo?.documents_path && projectInfo.git_root ? projectInfo.documents_path.replaceAll('\\', '/').slice(projectInfo.git_root.replaceAll('\\', '/').length + 1) : 'DesignDocs'
    const path = updateDocumentPath(file, scope)
    if (!path) { setNotice('本文の位置を特定できない形式です。読み取り専用の差分を表示します。'); return }
    try {
      const text = await invoke<string>('locate_update', { recordId: record.id, path: file.path, line: hunk.line })
      const hit = updateHit(text, path, hunk)
      if (!hit) { setNotice('位置を安全に特定できません。差分を表示します。'); return }
      setCurrent(path); setContent(text); setSaved(text); setMode('write'); setEditing(false)
      setSearchTarget(hit); setUpdateHighlight(true); setShowSearchSource(false); setSelectedDirectory(parentDirectory(path))
      setExternalChanged(false); setValidationIssues([]); setMissingImages([]); setEditorRevision((value) => value + 1)
      setPanel(null); setNotice(`更新箇所: ${path} · ${hunk.heading}。右側の「変更前後の差分」から差分を再表示できます。`)
      void readUpdates(record.id)
    } catch (error) { setNotice(String(error)) }
  }
  async function chooseRepo(restorePath?: string) {
    try {
      if (busy) return
      if (dirty && !await confirmDialog('未保存の変更を破棄して別のプロジェクトを開きますか？')) return
      const selected = restorePath ?? await open({ directory: true, multiple: false, title: '設計書リポジトリを開く' })
      if (typeof selected !== 'string') return
      setBusy(true)
      if (!await prepareWorkspace(selected, invoke, (message) => confirmDialog(message, { title: '設計書 Worktree の作成', kind: 'warning', okLabel: '作成する', cancelLabel: 'キャンセル' }))) return
      const { project, files, status, directories, first, firstContent } = await loadProjectSnapshot(selected, invoke)
      setProjectInfo(project); setUpdateHighlight(false); setUpdates([]); setUpdatePopup(null); setSyncStatus('GitHub の更新を確認しています…')
      setRepo(project.unity_root); setDocsReady(project.has_documents); setDocs(files); setGit(status); setDocumentDirectories(directories)
      setSelectedDirectory(directories[0] ?? ''); setTreeMenu(null); setRenamePath(null); setDraggedPath(''); setDropDirectory('')
      setEditing(false); setSearchTarget(null); setSearchError(''); setSearchSkipped([]); setSearchTruncated(false); setSearchPending(false)
      setCurrent(first?.path ?? ''); setContent(firstContent); setSaved(firstContent)
      setQuery(''); setSearchMatches([]); setHistory([]); setDiff(''); setPanel(null); setSetupPreview(null); setNewItemKind(null); setMode('write')
      setGitManagerOpen(false); setGitBranches([]); setGitRemotes([]); setPushPreview(null); setSelectedRemote(''); setPushBranch(''); setSelectedLocalBranch(''); setBranchStart(''); setSelectedRemoteBranch(''); setRemoteNameInput(''); setRemoteUrlInput(''); setCommitMessage(''); setPushSetUpstream(false); setCreatePushBranch(false)
      setValidationIssues([]); setMissingImages([]); setExternalChanged(false); setEditorRevision((value) => value + 1)
      externalNoticeShown.current = false
      setNotice(project.has_documents ? `独立リポジトリの設計書を開きました: ${selected}` : `Unity プロジェクトを開きました。設計書ディレクトリはまだありません: ${selected}`)
      try { rememberProject(localStorage, project.unity_root) }
      catch (error) { setNotice(`プロジェクトは開きましたが、次回の復元パスを保存できません: ${String(error)}`) }
      await synchronizeWorkspace(project, first?.path ?? '', false)
    } catch (e) {
      try { if (repo) await invoke('open_repository', { path: repo }); else await invoke('close_repository') }
      catch { try { await invoke('close_repository') } catch { /* backend is unavailable in browser preview */ } }
      setNotice(`${restorePath ? '前回のプロジェクトを復元できませんでした。フォルダー選択から再選択してください' : 'プロジェクトを開けませんでした'}: ${String(e)}`)
    } finally { setBusy(false) }
  }
  useEffect(() => {
    if (!isTauri() || restorationStarted.current) return
    restorationStarted.current = true
    void restoreProject(localStorage, chooseRepo).catch((error) => setNotice(`前回のプロジェクト設定を読めません。フォルダーを再選択してください: ${String(error)}`))
  })
  async function previewDocumentSetup() {
    try {
      setBusy(true)
      setSetupPreview(await invoke<SetupPreview>('preview_document_setup'))
    } catch (error) { setNotice(`初期化プレビューを作成できません: ${String(error)}`) }
    finally { setBusy(false) }
  }
  async function createDocumentSetup() {
    if (!setupPreview) return
    try {
      setBusy(true)
      const project = await invoke<ProjectInfo>('create_document_setup', { confirmed: true, expectedChangedFiles: setupPreview.changed_files })
      setRepo(project.unity_root); setDocsReady(project.has_documents)
      const [files, status, directories] = await Promise.all([invoke<DocFile[]>('list_documents'), invoke<GitState>('git_status'), invoke<string[]>('list_document_directories')])
      const first = files.find((file) => file.kind === 'markdown' || file.kind === 'json')
      const firstContent = first ? await invoke<string>('read_document', { path: first.path }) : ''
      setDocs(files); setGit(status); setDocumentDirectories(directories)
      setCurrent(first?.path ?? ''); setContent(firstContent); setSaved(firstContent)
      setEditing(false); setSearchTarget(null); setSelectedDirectory(directories[0] ?? '')
      setSetupPreview(null); setNotice('設計書ディレクトリを作成し、ファイル一覧を再読み込みしました')
    } catch (error) { setNotice(`設計書ディレクトリを作成できません: ${String(error)}`) }
    finally { setBusy(false) }
  }
  async function selectFile(path: string, target?: SearchMatch) {
    if (busy || syncing.current) return
    setUpdateHighlight(false)
    const pending = readEditorSnapshot.current()
    if ((dirty || (pending !== null && pending !== saved)) && !await confirmDialog('未保存の変更があります。破棄して選択した位置を開きますか？', { kind: 'warning' })) return
    try {
      const text = isTauri() && repo ? await invoke<string>('read_document', { path }) : (demoContent[path] ?? `# ${path.split('/').at(-1)?.replace(/\.md$/, '')}\n\nこのファイルはサンプルです。ローカルリポジトリを開くと実ファイルを編集できます。`)
      setCurrent(path); setContent(text); setSaved(text); setMode('write')
      setEditing(false); setSearchTarget(target && target.line > 0 ? target : null); setShowSearchSource(false); setEditorRevision((value) => value + 1)
      if (target?.line) setNotice(selectionForHit(text, target) ? `検索位置: ${path} · 行 ${target.line}、列 ${target.column}` : '検索後にファイルが変更されました。もう一度検索してください。')
      setSelectedDirectory(parentDirectory(path))
      setValidationIssues([]); setMissingImages([]); setExternalChanged(false)
      externalNoticeShown.current = false
    } catch (e) { setNotice(`ファイルを読み込めません: ${String(e)}`) }
  }
  function showTreeMenu(event: React.MouseEvent, path: string, directory: boolean) {
    event.preventDefault(); event.stopPropagation()
    if (!repo || busy) return
    setSelectedDirectory(directory ? path : parentDirectory(path))
    setTreeMenu({ path, directory, x: Math.max(0, Math.min(event.clientX, window.innerWidth - 180)), y: Math.max(0, Math.min(event.clientY, window.innerHeight - 180)) })
  }
  async function changeTreeEntry(source: string, destinationParent: string, name: string, deleting = false) {
    if (busy || !repo || !isTauri()) return
    const protection = entryProtectionReason(source, current, dirty, externalChanged)
    if (protection) { setNotice(protection); return }
    if (deleting && !await confirmDialog(`「${source}」を完全に削除しますか？フォルダーの場合は中のファイルもすべて削除されます。ごみ箱には移動しません。`, { title: '削除の確認', kind: 'warning', okLabel: '完全に削除', cancelLabel: 'キャンセル' })) return
    try {
      setBusy(true); setTreeMenu(null)
      if (deleting) {
        await invoke('delete_document_entry', { path: source, confirmed: true })
        if (containsEntry(source, current)) { setCurrent(''); setContent(''); setSaved(''); setExternalChanged(false); setValidationIssues([]); setMissingImages([]); setEditorRevision((value) => value + 1) }
        if (containsEntry(source, selectedDirectory)) setSelectedDirectory(parentDirectory(source))
      } else {
        const target = await invoke<string>('move_document_entry', { path: source, parentPath: destinationParent, name })
        setCurrent((path) => relocatedPath(path, source, target))
        setSelectedDirectory((path) => relocatedPath(path, source, target))
        setEditorRevision((value) => value + 1)
      }
      setRenamePath(null); setPanel(null); setHistory([]); setDiff(''); setPushPreview(null)
      if (containsEntry(source, current)) { setEditing(false); setSearchTarget(null) }
      await refresh()
      setNotice(deleting ? `削除しました: ${source}` : '移動・名前変更が完了しました。相対リンクは自動更新されません。参照先を確認してください。')
    } catch (error) { setNotice(`ファイル操作に失敗しました: ${String(error)}`); try { await refresh() } catch { /* preserve operation error */ } }
    finally { setBusy(false); setDraggedPath(''); setDropDirectory('') }
  }
  async function save() {
    if (!isTauri() || !repo) { setNotice('保存するにはローカル Git リポジトリを開いてください。'); return }
    if (externalChanged && !window.confirm('このファイルは外部で変更されています。現在の編集内容で置き換えますか？')) return
    if (isJson) {
      const localIssues = validateStructuredDocument(content)
      if (localIssues.length) { setValidationIssues(localIssues); setNotice('JSON の検証に失敗しました'); return }
      try {
        const issues = await invoke<string[]>('validate_document', { path: current, content })
        setValidationIssues(issues)
        if (issues.length) { setNotice('JSON Schema の検証に失敗しました'); return }
      } catch (e) { setNotice(`JSON を検証できません: ${String(e)}`); return }
    }
    try { setBusy(true); await invoke('save_document', { path: current, content }); setSaved(content); setExternalChanged(false); setNotice('保存しました'); await refresh() }
    catch (e) { setNotice(`保存できません: ${String(e)}`) } finally { setBusy(false) }
  }

  async function validateCurrent() {
    const localIssues = validateStructuredDocument(content)
    if (localIssues.length) { setValidationIssues(localIssues); setNotice('JSON の検証に失敗しました'); return }
    try {
      const issues = await invoke<string[]>('validate_document', { path: current, content })
      setValidationIssues(issues)
      setNotice(issues.length ? 'JSON Schema の検証に失敗しました' : 'JSON と JSON Schema は有効です')
    } catch (error) { setNotice(`JSON を検証できません: ${String(error)}`) }
  }

  async function importImage(replace: boolean) {
    if (!editing) { setNotice('画像を変更するには「編集」を押してください。'); return }
    if (!repo || !isTauri()) { setNotice('画像を追加するには Tauri デスクトップアプリで設計書を開いてください。'); return }
    const refs = extractImageReferences(content).filter((ref) => !/^(https?:|data:|#)/i.test(ref))
    let replacePath: string | null = null
    if (replace) {
      if (!refs.length) { setNotice('置き換える画像参照がありません。'); return }
      replacePath = window.prompt(`置き換える画像パスを入力してください:\n${refs.join('\n')}`, refs[0])
      if (!replacePath) return
      if (!refs.includes(replacePath)) { setNotice('文書内にその画像参照が見つかりません。'); return }
    }
    try {
      const selected = await open({ multiple: false, directory: false, title: replace ? '置き換える画像を選択' : '挿入する画像を選択', filters: [{ name: '画像', extensions: ['png', 'jpg', 'jpeg', 'svg'] }] })
      if (typeof selected !== 'string') return
      const result = await invoke<{ path: string }>('import_image', { documentPath: current, sourcePath: selected, replacePath })
      if (!replacePath) { setContent((previous) => `${previous.trimEnd()}\n\n${markdownImageLink(current, result.path)}\n`); setEditorRevision((value) => value + 1) }
      setNotice(replacePath ? '画像を置き換えました' : '画像を images ディレクトリへコピーし、Markdown に挿入しました')
    } catch (e) { setNotice(`画像を処理できません: ${String(e)}`) }
  }
  function changeSearchQuery(value: string) {
    setQuery(value); setSearchMatches([]); setSearchError(''); setSearchSkipped([]); setSearchTruncated(false)
    setSearchPending(Boolean(value.trim() && repo && isTauri()))
  }
  useEffect(() => {
    if (!query.trim() || !repo || !isTauri()) return
    if (busy) return
    let active = true
    const timer = window.setTimeout(() => {
      setSearchPending(true); setSearchError(''); setSearchSkipped([]); setSearchTruncated(false)
      void invoke<SearchResults>('search_documents', { query }).then((result) => {
        if (active) { setSearchMatches(result.matches); setSearchSkipped(result.skipped); setSearchTruncated(result.truncated) }
      }).catch((error) => { if (active) { setSearchMatches([]); setSearchError(`検索できません: ${String(error)}`) } })
        .finally(() => { if (active) setSearchPending(false) })
    }, 180)
    return () => { active = false; window.clearTimeout(timer) }
  }, [repo, query, docs, busy])
  async function gitAction(action: 'git_diff' | 'git_commit' | 'git_history') {
    if (!isTauri() || !repo) { setNotice('Git 操作にはローカルリポジトリを開いてください。'); return }
    if (action === 'git_commit' && (dirty || externalChanged)) { setNotice('Commit 前に未保存の編集・外部変更を保存して確認してください。'); return }
    try {
      setBusy(true)
      if (action === 'git_diff') { setDiff(await invoke<string>(action)); setPanel('diff') }
      else if (action === 'git_history') { setHistory(isJson ? await invoke<string[]>('git_field_history', { path: current }) : await invoke<string[]>(action, { path: current })); setPanel('history') }
      else if (action === 'git_commit') { const message = window.prompt('コミットメッセージ'); if (!message) return; setNotice(await invoke<string>(action, { message })) }
      else { setNotice(await invoke<string>(action)) }
      await refresh()
    } catch (e) { setNotice(`Git 操作を完了できません: ${String(e)}`) } finally { setBusy(false) }
  }
  async function loadGitManager(resetTarget = false) {
    const [branches, remotes] = await Promise.all([invoke<GitBranchInfo[]>('git_branches'), invoke<GitRemoteInfo[]>('git_remotes')])
    setGitBranches(branches); setGitRemotes(remotes)
    const firstLocal = branches.find((branch) => !branch.remote && branch.current) ?? branches.find((branch) => !branch.remote)
    const firstRemote = remotes[0]
    setBranchStart((value) => value || firstLocal?.name || '')
    setSelectedLocalBranch((value) => branches.some((branch) => !branch.remote && branch.name === value) ? value : firstLocal?.name || '')
    setSelectedRemote((value) => remotes.some((remote) => remote.name === value) ? value : firstRemote?.name || '')
    setRemoteNameInput((value) => value || firstRemote?.name || '')
    const target = projectInfo?.worktree_root ? { remote: remotes.some((remote) => remote.name === "origin") ? "origin" : "", branch: "main" } : defaultPushTarget(branches, remotes.map((remote) => remote.name))
    const currentRemoteName = target.remote
    const currentRemoteBranch = target.branch
    setSelectedRemoteBranch((value) => value || currentRemoteBranch || '')
    setSelectedRemote((value) => !resetTarget && remotes.some((remote) => remote.name === value) ? value : currentRemoteName)
    setPushBranch((value) => resetTarget ? currentRemoteBranch : value)
    setSelectedLocalBranch(firstLocal?.name || '')
    setPushPreview(null)
  }
  async function openGitManager() {
    if (!repo || !isTauri()) { setNotice('Git 管理にはローカルリポジトリを開いてください。'); return }
    try { setBusy(true); setPushPreview(null); await refresh(); await loadGitManager(true); setGitManagerOpen(true) }
    catch (error) { setNotice(`Git 情報を読み込めません: ${String(error)}`) }
    finally { setBusy(false) }
  }
  async function runGitManagerAction(command: string, args: Record<string, unknown>, confirmation?: string) {
    if (confirmation && !window.confirm(confirmation)) return
    try {
      setBusy(true)
      if (command === 'git_pull_ff' && projectInfo?.worktree_root) { await synchronizeWorkspace(); await loadGitManager(); return }
      const result = await mutateAndRefresh(() => invoke<string>(command, args), async () => { await refresh(); await loadGitManager(command === 'git_switch_branch') })
      setNotice(result || 'Git 操作が完了しました')
      setPushPreview(null)
      if (command === 'git_remote_change') { setRemoteUrlInput(''); if (args.action === 'add' && typeof args.name === 'string') { setSelectedRemote(args.name); setRemoteNameInput(args.name) } }
    } catch (error) { setNotice(`Git 操作に失敗しました: ${String(error)}`); try { await refresh(); await loadGitManager() } catch { /* preserve the Git error */ } }
    finally { setBusy(false) }
  }
  async function checkPushTarget() {
    if (busy || dirty || externalChanged || selectedLocalBranch !== git.branch || pushActionDisabled(git, selectedRemote, pushBranch)) return
    try {
      setBusy(true); setPushPreview(null)
      await refresh()
      setPushPreview(await invoke<GitPushPreview>('git_push_preview', { remote: selectedRemote, branch: pushBranch.trim() }))
    } catch (error) {
      setPushPreview({ current_branch: selectedLocalBranch, remote: selectedRemote, remote_branch: pushBranch.trim(), changed_count: git.changed.length, ahead: null, behind: null, target_exists: false, ready: false, message: `送信先の履歴を検証できませんでした: ${String(error)}` })
    } finally { setBusy(false) }
  }
  async function pushDaily() {
    if (pushBlockReason) return
    try {
      setBusy(true); setPushPreview(null)
      const preview = await invoke<GitPushPreview>('git_push_preview', { remote: selectedRemote, branch: pushBranch.trim() })
      setPushPreview(preview)
      await refresh()
      if (!preview.ready || preview.current_branch !== selectedLocalBranch) { setNotice(preview.message); return }
      const confirmed = await confirmDialog(`${preview.message}\n${pushConfirmation({ ...git, ahead: preview.ahead ?? 0, behind: preview.behind ?? 0 }, selectedRemote, pushBranch.trim())}`, { title: 'Push の確認', kind: 'warning' })
      if (!confirmed) return
      setNotice(await mutateAndRefresh(() => invoke<string>('git_push_target', { remote: selectedRemote, branch: pushBranch.trim(), setUpstream: pushSetUpstream, expectedSource: selectedLocalBranch }), async () => { await refresh(); await loadGitManager() }))
    } catch (error) { setNotice(`Push に失敗しました: ${String(error)}`) }
    finally { setBusy(false) }
  }
  async function commitDaily() {
    if (busy || dirty || externalChanged || !commitMessage.trim()) return
    await runGitManagerAction('git_commit', { message: commitMessage.trim() })
  }
  useEffect(() => {
    if (!repo || !current || !isTauri() || busy) return
    let active = true
    const timer = window.setInterval(async () => {
      try {
        const external = await invoke<string>('read_document', { path: current })
        if (!active) return
        if (external !== saved && !dirty) { setContent(external); setSaved(external); setExternalChanged(false); externalNoticeShown.current = false; setNotice('外部で変更されたファイルを再読み込みしました') }
        else if (external !== saved && dirty) {
          setExternalChanged(true)
          if (!externalNoticeShown.current) { externalNoticeShown.current = true; setNotice('外部変更と未保存の編集があります。保存前に確認します。') }
        } else if (external === saved) { setExternalChanged(false); externalNoticeShown.current = false }
      } catch (error) { if (active) setNotice(`外部変更を確認できません: ${String(error)}`) }
    }, 2500)
    return () => { active = false; window.clearInterval(timer) }
  }, [repo, current, dirty, saved, busy])

  useEffect(() => {
    if (!repo || !docsReady || !isTauri() || busy) return
    let active = true
    const timer = window.setInterval(() => {
      void Promise.all([invoke<DocFile[]>('list_documents'), invoke<GitState>('git_status'), invoke<string[]>('list_document_directories'), invoke<ProjectInfo>('project_context')])
        .then(([files, status, directories, context]) => { if (active) { setDocs(files); setGit(status); setDocumentDirectories(directories); setProjectInfo(context) } })
        .catch((error) => { if (active) setNotice(`ワークスペースを更新できません: ${String(error)}`) })
    }, 5000)
    return () => { active = false; window.clearInterval(timer) }
  }, [repo, docsReady, busy])

  useEffect(() => {
    if (!repo || !current || isJson || !isTauri() || busy) return
    const refs = extractImageReferences(content).filter((ref) => !/^(https?:|data:|#)/i.test(ref))
    const timer = window.setTimeout(() => { void invoke<string[]>('check_image_references', { documentPath: current, references: refs }).then(setMissingImages).catch((error) => setNotice(`画像参照を確認できません: ${String(error)}`)) }, 350)
    return () => window.clearTimeout(timer)
  }, [repo, current, isJson, content, busy])

  useEffect(() => {
    if (!isTauri()) return
    let active = true
    let unsubscribe: (() => void) | undefined
    const currentWindow = getCurrentWindow()
    const handleCloseRequested = createCloseRequestHandler({
      hasUnsavedChanges: () => { const pending = readEditorSnapshot.current(); return closeState.current.dirty || (pending !== null && pending !== closeState.current.saved) },
      confirmDiscard: () => confirmDialog('未保存の編集を破棄して終了しますか？', {
        title: 'DesignBook Studio を終了',
        kind: 'warning',
        okLabel: '保存せずに終了',
        cancelLabel: 'キャンセル',
      }),
      destroyWindow: () => currentWindow.destroy(),
      onError: (error) => setNotice(`終了処理に失敗しました。ウィンドウは開いたままです: ${String(error)}`),
    })
    void currentWindow.onCloseRequested((event) => {
      if (active) void handleCloseRequested(event)
    }).then((dispose) => { unsubscribe = dispose; if (!active) dispose() })
      .catch((error) => { if (active) setNotice(`終了時の未保存保護を登録できませんでした: ${String(error)}`) })
    return () => { active = false; unsubscribe?.() }
  }, [])
  const fields = isJson ? parseFields(content) : []
  const localBranches = gitBranches.filter((branch) => !branch.remote)
  const commitBlockReason = busy ? '処理中です。完了後に操作してください。' : dirty || externalChanged ? '未保存の編集または未確認の外部変更があります。保存・確認してください。' : !git.branch ? 'ローカルブランチを確認できません。' : !git.document_changed.length ? 'DesignDocs にコミットする変更がありません。' : !commitMessage.trim() ? 'コミットメッセージを入力してください。' : ''
  const pushInputReason = pushDisabledReason(git, selectedRemote, pushBranch, { busy, dirty, externalChanged, source: selectedLocalBranch })
  const pushBlockReason = pushInputReason || pushAssessmentReason(pushPreview, selectedLocalBranch, selectedRemote, pushBranch)
  const remoteBranches = remoteBranchChoices(gitBranches, selectedRemote)
  const updateField = (index: number, key: string, value: string) => { if (!editing) return; const parsed = JSON.parse(content); const list = Array.isArray(parsed) ? parsed : (parsed.fields ?? parsed.properties); const field = list[index] as Record<string, unknown>; if (key === 'defaultValue') { const type = field.type; field[key] = ['number', 'integer'].includes(String(type)) && value.trim() ? Number(value) : type === 'boolean' ? value.toLowerCase() === 'true' : ['array', 'object'].includes(String(type)) ? (() => { try { return JSON.parse(value) } catch { return value } })() : value } else field[key] = value; setContent(JSON.stringify(parsed, null, 2)) }
  const addField = () => { if (!editing) return; const parsed = JSON.parse(content); const list = Array.isArray(parsed) ? parsed : (parsed.fields ??= []); list.push({ id: `FLD-NEW-${crypto.randomUUID().slice(0, 8)}`, name: 'new_field', displayName: '新しい項目', type: 'string', defaultValue: '', description: '' }); setContent(JSON.stringify(parsed, null, 2)) }

  return <main className="app-shell" onDragOver={(event) => event.preventDefault()} onDrop={(event) => { event.preventDefault(); setDraggedPath(''); setDropDirectory('') }}>
    <header className="topbar">
      <div className="brand"><div className="brand-mark"><BookOpen size={18} /></div><span>DesignBook <b>Studio</b></span><span className="beta">LOCAL</span></div>
      <div className="repo-pill"><GitBranch size={15} /><span>{repo ? repo.split(/[\\/]/).at(-1) : 'プロジェクト未選択'}</span><ChevronDown size={14} /></div>
      <div className="top-spacer" />
      <div className="branch" title={projectInfo?.documents_path ?? projectInfo?.worktree_root ?? ""}><GitBranch size={14} />XiuxianGame_DesignDocs / Docs: {git.branch || "—"}{projectInfo?.worktree_root && <small> · {projectInfo.worktree_root.split(/[\\/]/).at(-1)}\DesignDocs</small>}</div>
      <div className={`save-state ${dirty ? 'unsaved' : ''}`}><span className="status-dot" />{dirty ? '未保存の変更' : 'すべて保存済み'}</div>
      <button className="icon-button" title="差分を確認" onClick={() => void gitAction('git_diff')}><GitCompareArrows size={16} /></button>
      <button className="push-button" disabled={!repo || busy} onClick={() => void openGitManager()}><GitBranch size={15} />Git 管理（Commit / Push）</button>
    </header>
    {repo && docsReady && git.changed.length > 0 && <div className="git-warning" role="status"><ShieldCheck size={14} /><span><b>{git.branch}</b> に未コミット変更が {git.changed.length} 件あります。{git.has_upstream ? ` リモート状態: ahead ${git.ahead} / behind ${git.behind}。` : ' 追跡リモートは未設定です。'}保存はファイルのみ、Commit はローカル作成、Push は既存コミットのみ送信します。</span></div>}
    <div className="workspace">
      <aside className="sidebar">
        <button className="project-row" disabled={busy} onClick={() => void chooseRepo()}><div className="project-icon"><Sparkles size={16} /></div><span className="project-label">{repo ? repo.split(/[\\/]/).at(-1) : 'プロジェクトを開く'}</span><ChevronDown size={14} /></button>
        <div className="side-actions"><span>ワークスペース</span><div className="side-create-actions"><button disabled={!repo || !docsReady || busy} onClick={() => void beginCreate('document')} title="新しい文書"><FileText size={14} /><Plus size={9} /></button><button disabled={!repo || !docsReady || busy} onClick={() => void beginCreate('folder')} title="新しいフォルダー"><Folder size={14} /><Plus size={9} /></button></div></div>
        {!repo && <button className="open-repo-card" disabled={busy} onClick={() => void chooseRepo()}><Folder size={17} /><span>Unity プロジェクトを開く</span><ChevronRight size={14} /></button>}
        <div className="search-box"><Search size={15} /><input aria-label="文書名・本文を検索" placeholder="文書名・本文を検索..." value={query} onChange={(e) => changeSearchQuery(e.target.value)} onKeyDown={(event) => { if (event.key === "Escape") changeSearchQuery('') }} />{query && <button className="search-clear" aria-label="検索をクリア" onClick={() => changeSearchQuery('')}><X size={13} /></button>}</div>
        <nav className="file-tree">
          <div className="tree-location" title={selectedDirectory || documentRoot}>作成先: {selectedDirectory || documentRoot}</div>
          {query.trim() && <section className="search-results" aria-label="本文検索結果">
            <div className="search-results-summary" role="status">{!repo ? '本文検索にはプロジェクトを開いてください' : searchPending ? '検索中…' : `${searchMatches.length} 件の一致 · ${searchGroups.length} ファイル（保存済みの本文）`}</div>
            {searchError && <p role="alert" className="search-error">{searchError}</p>}
            {searchTruncated && <p className="search-error">先頭1000件を表示しています。検索語を絞り込んでください。</p>}
            {searchSkipped.length > 0 && <details className="search-error"><summary>{searchSkipped.length} ファイルを検索できませんでした（5MB超過・読み取り不可・非UTF-8）</summary>{searchSkipped.map((path) => <p key={path}>{path}</p>)}</details>}
            {repo && !searchPending && !searchError && searchMatches.length === 0 && <p className="empty-tree">一致する文書名・本文がありません。</p>}
            {searchGroups.map(([path, hits]) => <section key={path} className="search-result-file"><div className="search-result-path" title={path}><FileText size={13} />{path}<b>{hits.length}</b></div>
              {hits.map((hit) => { const parts = splitHighlight(hit.excerpt, hit.excerpt_offset, hit.length); return <button className="search-result-hit" disabled={busy} key={`${hit.line}:${hit.column}`} onClick={() => void selectFile(path, hit)} title={`${path}:${hit.line}:${hit.column}`}>
                <span className="search-hit-line">{hit.line ? `${hit.line}:${hit.column}` : '名前'}</span><span>{hit.line ? <>{parts.before}<mark>{parts.hit}</mark>{parts.after}</> : 'ファイル名が一致 — 開く'}</span>
              </button> })}
            </section>)}
          </section>}
          {!query.trim() && treeDirectories.map((directory) => {
            const files = filesInDirectory(directory)
            const depth = Math.max(0, directory.split('/').length - documentRoot.split('/').length)
            return <section className="tree-group" key={directory}>
              <div role="button" tabIndex={0} aria-label={`フォルダー ${directory}`} aria-pressed={selectedDirectory === directory}
                className={`group-heading tree-folder ${selectedDirectory === directory ? 'folder-selected' : ''} ${dropDirectory === directory ? 'drop-target' : ''}`}
                style={{ paddingLeft: 5 + depth * 12 }}
                onClick={() => { setSelectedDirectory(directory); setTreeMenu(null) }}
                onKeyDown={(event) => { if (event.key === 'Enter' || event.key === ' ') { event.preventDefault(); setSelectedDirectory(directory) } }}
                onContextMenu={(event) => showTreeMenu(event, directory, true)}
                draggable={Boolean(repo && !busy && directory !== documentRoot)}
                onDragStart={(event) => { event.dataTransfer.setData('application/x-designbook-path', directory); event.dataTransfer.effectAllowed = 'move'; setDraggedPath(directory); setTreeMenu(null) }}
                onDragEnd={() => { setDraggedPath(''); setDropDirectory('') }}
                onDragOver={(event) => { if (!busy && canDropEntry(draggedPath, directory)) { event.preventDefault(); event.dataTransfer.dropEffect = 'move'; setDropDirectory(directory) } }}
                onDragLeave={() => setDropDirectory('')}
                onDrop={(event) => { event.preventDefault(); event.stopPropagation(); if (!busy && canDropEntry(draggedPath, directory)) { setSelectedDirectory(directory); void changeTreeEntry(draggedPath, directory, draggedPath.split('/').at(-1) ?? '') } setDropDirectory('') }}>
                <ChevronDown size={13} /><Folder size={14} /><span title={directory}>{directory === documentRoot ? directory : directory.split('/').at(-1)}</span><span className="count">{files.length}</span>
              </div>
              {files.map((file) => <button key={file.path} disabled={busy} draggable={Boolean(repo && !busy)}
                onDragStart={(event) => { event.dataTransfer.setData('application/x-designbook-path', file.path); event.dataTransfer.effectAllowed = 'move'; setDraggedPath(file.path); setTreeMenu(null) }}
                onDragEnd={() => { setDraggedPath(''); setDropDirectory('') }}
                onContextMenu={(event) => showTreeMenu(event, file.path, false)}
                className={`file-row ${current === file.path ? 'selected' : ''}`} onClick={() => void selectFile(file.path)} title={searchMatches.find((match) => match.path === file.path)?.excerpt}>
                <span className="tree-indent" />{file.kind === 'json' ? <FileJson2 size={14} className="json-icon" /> : <FileText size={14} className="md-icon" />}<span>{file.path.split('/').at(-1)?.replace(/\.md$/, '')}</span>{current === file.path && dirty && <i className="dirty-dot" />}
              </button>)}
            </section>
          })}
          {isTauri() && repo && docsReady && docs.length === 0 && <div className="empty-tree">Markdown / JSON ファイルが見つかりません</div>}
        </nav>
        <div className="sidebar-bottom"><button><Settings2 size={15} />設定</button><button><CircleHelp size={15} />ヘルプと使い方</button><div className="user-card"><div className="avatar">D</div><div><b>DesignBook Studio</b><small>ローカルワークスペース</small></div><ChevronDown size={14} /></div></div>
      </aside>
      <section className="main-column">
        <div className="breadcrumbs"><span>{current.split('/')[0] || 'DesignDocs'}</span><ChevronRight size={13} /><span>{current.split('/').slice(1, -1).join(' / ') || 'ホーム'}</span><ChevronRight size={13} /><b>{current.split('/').at(-1)?.replace(/\.md$|\.json$/, '')}</b><div className="crumb-actions"><button title="変更履歴" onClick={() => void gitAction('git_history')}><History size={15} /></button><button title="ファイルの詳細"><ListFilter size={15} /></button></div></div>
        <div className="document-scroll">
          {repo && isTauri() && !docsReady ? <section className="empty-docs"><div className="empty-docs-icon"><Folder size={24} /></div><h1>この Unity プロジェクトには設計書ディレクトリがありません</h1><p>既存ファイルは変更していません。作成前に、追加するディレクトリと README.md を確認できます。</p><button disabled={busy} onClick={() => void previewDocumentSetup()}><Plus size={15} />作成内容をプレビュー</button></section> : <article className="document">
            <div className="doc-meta"><span className="doc-badge"><FileText size={13} />{isJson ? 'JSON フィールド仕様' : '設計ドキュメント'}</span><span className="meta-separator">·</span><span>最終更新: たった今</span><span className="meta-spacer" /><button onClick={() => void gitAction('git_history')}><Clock3 size={14} />履歴</button></div>
            <h1 className="document-title">{current.split('/').at(-1)?.replace(/\.md$|\.json$/, '').replace(/[-_]/g, ' ')}</h1>
            <div className="author-row"><div className="avatar small">D</div><span>Design Team</span><span className="meta-separator">·</span><span>ドキュメント</span><span className="meta-separator">·</span><span>約 2 分</span></div>
            <div className="editor-toolbar"><div className="toolbar-left">
              <button className={editing ? 'active' : ''} disabled={!repo || !current || busy} aria-pressed={editing} onClick={() => { if (editing) { const pending = readEditorSnapshot.current(); if (pending !== null) setContent(pending) } setEditing(!editing) }}><BookOpen size={15} />{editing ? '閲覧に戻る' : '編集'}</button>
              <span className="reading-mode-label">{editing ? '編集中' : '閲覧専用'}</span>
              {isJson && <><button className={mode === 'fields' ? 'active' : ''} onClick={() => { setMode('fields'); setSearchTarget(null) }}><LayoutGrid size={15} />フィールド</button><button className={mode === 'write' ? 'active' : ''} onClick={() => { setMode('write'); setSearchTarget(null) }}><FileJson2 size={15} />JSON</button><button onClick={() => void validateCurrent()}><ShieldCheck size={14} />検証</button></>}
              {!isJson && <><button disabled={!editing || busy} onClick={() => void importImage(false)}><Image size={14} />画像挿入</button><button disabled={!editing || busy} onClick={() => void importImage(true)}><Image size={14} />置換</button></>}
            </div><div className="toolbar-right"><span className="saved-label">{externalChanged ? '外部変更あり' : dirty ? '● 未保存' : '✓ 保存済み'}</span><button className="save-button" disabled={!dirty || busy} onClick={() => void save()}>{busy ? <LoaderCircle size={14} className="spin" /> : null}保存</button></div></div>
            {searchTarget && !showSearchSource && <div className="search-reading-location" role="status"><span>検索位置 · 行 {searchTarget.line}、列 {searchTarget.column} · {editing ? '編集中' : '閲覧専用'}</span><button onClick={() => setShowSearchSource(true)}>原文を表示</button><button onClick={() => setSearchTarget(null)}>強調を解除</button></div>}
            {validationIssues.length > 0 && <ul className="validation-issues">{validationIssues.map((issue, index) => <li key={`${index}-${issue}`}>{issue}</li>)}</ul>}
            {missingImages.length > 0 && <div className="image-warning"><Image size={14} /><span>画像参照が見つかりません: {missingImages.join('、')}</span></div>}
            {searchTarget && showSearchSource ? <SearchLocationView content={content} hit={searchTarget} editable={editing} onChange={setContent} onClose={() => { setShowSearchSource(false); setEditorRevision((value) => value + 1) }} /> : isJson && mode === 'fields' ? <section className="fields-editor"><div className="field-intro"><div><h2>構造化フィールド</h2><p>安定した ID で変更履歴を追跡します。</p></div><button className="add-field" disabled={!editing || busy} onClick={addField}><Plus size={14} />項目を追加</button></div><div className="field-table-wrap"><table className="field-table"><thead><tr><th>フィールド名</th><th>表示名</th><th>型</th><th>初期値</th><th>説明</th><th>フィールド ID</th></tr></thead><tbody>{fields.map((field: Record<string, unknown>, index: number) => <tr key={String(field.id ?? index)}>{['name', 'displayName', 'type', 'defaultValue', 'description', 'id'].map((key) => <td key={key}><input value={String(field[key] ?? '')} readOnly={!editing || key === 'id'} onChange={(e) => updateField(index, key, e.target.value)} aria-label={key} /></td>)}</tr>)}</tbody></table></div>{!fields.length && <p className="json-error">有効な JSON の `fields` 配列がありません。JSON タブから形式を確認してください。</p>}</section> : isJson ? <div className="json-source-wrap"><div className="json-note"><ShieldCheck size={15} />編集内容は JSON Schema として検証してから原子的に保存されます。</div><textarea ref={jsonSource} className="json-source" readOnly={!editing} aria-label={editing ? "JSON を編集" : "JSON 閲覧専用"} value={content} onChange={(e) => setContent(e.target.value)} spellCheck={false} /></div> : <Editor key={`${current}:${editorRevision}`} markdown={content} onChange={setContent} editable={editing} searchHit={searchTarget} onNotice={setNotice} snapshotRef={readEditorSnapshot} onError={(error) => setNotice(`文書を表示できません: ${String(error)}`)} />}
            <div className="doc-footer"><div className="footer-rule" /><div className="footer-note"><ShieldCheck size={15} /><span>Markdown を正本としてローカルに保存します。独立 Worktree で管理し、起動時に安全な更新を確認します。</span></div><button onClick={() => void gitAction('git_history')}><History size={14} />変更履歴を見る</button></div>
          </article>}
        </div>
        <footer className="statusbar"><span><HardDrive size={13} />{repo ? repo : 'サンプルワークスペース'}</span><span className="statusbar-right">設計書 {git.document_changed.length} / その他 {git.other_changed.length} 変更ファイル <span>·</span>{git.tracking_error ? '追跡先の履歴未検証' : !git.has_upstream ? '追跡先未設定' : git.behind ? `${git.behind} 件のリモート更新` : git.ahead ? `${git.ahead} 件の未送信コミット` : '既知の追跡先と一致（未 Fetch）'}</span></footer>
      </section>
      <aside className="right-panel">{repo && <UpdatePanel records={updates} busy={busy} status={syncStatus} onSync={() => void synchronizeWorkspace()} onRead={(id) => void readUpdates(id)} onNavigate={(record, file, hunk) => void navigateUpdate(record, file, hunk)} popup={updatePopup} onDismiss={() => setUpdatePopup(null)} />}<div className="panel-tabs"><button className="active">概要</button><button onClick={() => void gitAction('git_history')}>履歴</button><button onClick={() => void gitAction('git_diff')}>変更</button></div><section className="panel-section"><div className="panel-heading"><span>このページについて</span><button><Settings2 size={14} /></button></div><div className="property-card"><div className="property-icon purple"><FileText size={16} /></div><div><b>ドキュメント</b><small>Markdown ページ</small></div></div><div className="property-line"><span>保存場所</span><b>{current.split('/').slice(0, -1).join('/')}</b></div><div className="property-line"><span>状態</span><b><i className={dirty ? 'state-dot orange' : 'state-dot'} />{dirty ? '未保存' : '保存済み'}</b></div><div className="property-line"><span>形式</span><b>{isJson ? 'JSON' : 'Markdown'}</b></div></section><section className="panel-section git-section"><div className="panel-heading"><span>Git ワークスペース</span><button onClick={() => void refresh()}><RefreshCw size={13} /></button></div><div className="git-summary"><div className="git-avatar"><GitBranch size={15} /></div><div><b>{git.branch}</b><small>{git.changed.length ? `設計書 ${git.document_changed.length} / その他 ${git.other_changed.length} 件の変更` : 'ワーキングツリーはクリーン'}</small></div></div><button className="panel-action" onClick={() => void gitAction('git_diff')}><GitCompareArrows size={14} />差分を確認</button><button className="panel-action" onClick={() => void gitAction('git_history')}><History size={14} />コミット履歴</button></section><section className="panel-section activity-section"><div className="panel-heading"><span>最近のアクティビティ</span><button><ChevronDown size={14} /></button></div><div className="activity"><div className="activity-line" /><div className="activity-icon"><Sparkles size={13} /></div><div><b>{git.changed.length ? 'ローカルの変更' : 'ワークスペースを準備しました'}</b><small>{git.changed.length ? git.changed.join(', ') : '設計書を開いて編集を始めましょう'}</small><time>今</time></div></div></section><div className="tip-card"><div className="tip-icon"><Image size={14} /></div><div><b>Markdown と Mermaid</b><p>表、画像参照、Mermaid 図はファイルに保持され、Git で追跡できます。</p></div></div></aside>
    </div>
    <div className="toast" role="status"><span className="toast-dot" />{notice}<button onClick={() => setNotice('')}><X size={13} /></button></div>
    {panel && <div className="modal-backdrop" onClick={() => setPanel(null)}><section className="modal" onClick={(e) => e.stopPropagation()}><header><div><h2>{panel === 'diff' ? '変更差分' : 'ファイル履歴'}</h2><p>{current}</p></div><button onClick={() => setPanel(null)}><X size={17} /></button></header>{panel === 'diff' ? <pre className="diff-view">{diff || '作業ツリーに変更はありません。'}</pre> : <ol className="history-list">{history.map((item, i) => <li key={i}><GitCommitHorizontal size={15} /><code>{item}</code></li>)}{!history.length && <li>このファイルのコミット履歴はありません。</li>}</ol>}</section></div>}
    {gitManagerOpen && <div className="modal-backdrop git-manager-backdrop" onClick={() => setGitManagerOpen(false)}><section className="modal git-manager-modal" onClick={(event) => event.stopPropagation()}><header><div><h2>Commit → Push</h2><p>Unity: {repo} · 設計書 Git: {projectInfo?.git_root ?? repo}</p></div><button onClick={() => setGitManagerOpen(false)}><X size={17} /></button></header>
      <div className="git-manager-status"><GitBranch size={16} /><b>現在のブランチ: {git.branch || 'detached HEAD'}</b><span>未コミット {git.changed.length} 件</span><span>{git.tracking_error ? "追跡先の履歴未検証 · ahead/behind 未確定" : git.has_upstream ? `ahead ${git.ahead} / behind ${git.behind}` : "upstream 未設定 · ahead/behind 未確定"}</span></div>
      <div className="git-manager-grid">
        <section className="git-manager-section"><h3><GitCommitHorizontal size={15} />1. Commit</h3><p className="git-manager-summary">現在のブランチ: {git.branch || 'detached HEAD'} · 設計書の未コミット {git.document_changed.length} 件 / その他 {git.other_changed.length} 件</p><small>設計書ディレクトリの変更をまとめて Commit します。先にエディターの編集を保存してください。</small><ul className="git-manager-list">{git.document_changed.map((path) => <li key={path}>[設計書] {path}</li>)}{git.other_changed.map((path) => <li key={path}>[Unity・対象外] {path} — この Commit には含まれません</li>)}</ul><label>コミットメッセージ<input disabled={busy} value={commitMessage} onChange={(event) => setCommitMessage(event.target.value)} placeholder="変更内容を入力" /></label><button disabled={Boolean(commitBlockReason)} onClick={() => void commitDaily()}>Commit</button>{commitBlockReason && <small role="status">{commitBlockReason}</small>}{(dirty || externalChanged) && <small className="git-manager-warning">未保存の編集または外部変更があります。保存・確認後に続行してください。</small>}</section>
        <section className="git-manager-section"><h3><CloudUpload size={15} />2. Push</h3>
          <label>送信元ローカルブランチ<select disabled={busy} value={selectedLocalBranch} onChange={(event) => { setSelectedLocalBranch(event.target.value); setPushPreview(null) }}>{localBranches.map((branch) => <option key={branch.name} value={branch.name}>{branch.name}{branch.current ? '（現在）' : ''}</option>)}</select></label>
          {selectedLocalBranch !== git.branch && <><small>Push 前に選択したブランチへ切り替えてください。</small><button disabled={Boolean(projectInfo?.worktree_root) || busy || git.changed.length > 0 || dirty || externalChanged} onClick={() => void runGitManagerAction('git_switch_branch', { name: selectedLocalBranch }, `${selectedLocalBranch} に切り替えますか？`)}>このブランチへ切替</button></>}
          <label>送信先リモート<select disabled={busy} value={selectedRemote} onChange={(event) => { setSelectedRemote(event.target.value); setPushBranch(''); setPushPreview(null) }}><option value="">選択してください</option>{gitRemotes.map((remote) => <option key={remote.name} value={remote.name}>{remote.name}</option>)}</select></label>
          <label>送信先ブランチ{createPushBranch ? <input disabled={busy} value={pushBranch} onChange={(event) => { setPushBranch(event.target.value); setPushPreview(null) }} placeholder="新しいリモートブランチ名" /> : <select disabled={busy} value={pushBranch} onChange={(event) => { setPushBranch(event.target.value); setPushPreview(null) }}><option value="">選択してください</option>{[...new Set([...remoteBranches, ...(pushBranch ? [pushBranch] : [])])].map((branch) => <option key={branch} value={branch}>{branch}{remoteBranches.includes(branch) ? '' : '（新規）'}</option>)}</select>}</label>
          <label className="git-manager-check"><input disabled={busy} type="checkbox" checked={createPushBranch} onChange={(event) => { setCreatePushBranch(event.target.checked); setPushBranch(''); setPushPreview(null) }} />新しい送信先ブランチを作成</label>
          <label className="git-manager-check"><input disabled={busy} type="checkbox" checked={pushSetUpstream} onChange={(event) => setPushSetUpstream(event.target.checked)} />成功時に upstream を設定</label>
          <p className="git-manager-summary">{selectedLocalBranch || '未選択'} → {selectedRemote || '未選択'}/{pushBranch || '未選択'}</p>
          {git.tracking_error && <p role="status" className="git-manager-warning">{git.tracking_error}</p>}{pushPreview && <p role="status" className="git-manager-summary">{pushPreview.message}</p>}
          {git.changed.length > 0 && <details className="git-manager-warning"><summary>{pushWorktreeWarning(git)}</summary><ul className="git-manager-list">{git.document_changed.map((path) => <li key={path}>[設計書・送信対象外] {path}</li>)}{git.other_changed.map((path) => <li key={path}>[Unity・送信対象外] {path}</li>)}</ul></details>}<button disabled={Boolean(pushInputReason)} onClick={() => void checkPushTarget()}>送信先を安全確認（読み取り専用）</button><p role="status" className="git-manager-summary">{pushBlockReason || "安全確認済みです。Push 時にもう一度リモート履歴を検証します。"}</p>{!git.has_upstream && <small>upstream は未設定です。remote と送信先を選択すれば初回 Push が可能です。</small>}<button disabled={Boolean(pushBlockReason)} onClick={() => void pushDaily()}><Upload size={14} />Push</button><small>送信先を自動確認してから最終確認を表示します。未コミット変更は送信されません。履歴不明・分岐・リモート先行時は停止し、force push は行いません。</small>
        </section>
      </div><details className="git-advanced"><summary>詳細 Git 管理（ブランチ・リモート・Fetch / Pull）</summary>
      <div className="git-manager-grid">
        <section className="git-manager-section"><h3><GitFork size={15} />ブランチ</h3>{projectInfo?.worktree_root && <small>設計書は main 固定です。Unity 開発ブランチの切替は行いません。ゲームのリポジトリから独立しています。</small>}<label>ローカルブランチ<select value={selectedLocalBranch} onChange={(event) => setSelectedLocalBranch(event.target.value)}>{localBranches.map((branch) => <option key={branch.name} value={branch.name}>{branch.name}{branch.current ? ' (現在)' : ''} · upstream: {branch.upstream ?? '未設定'}{branch.upstream ? ` · ahead ${branch.ahead}/behind ${branch.behind}` : ''}</option>)}</select></label><button disabled={Boolean(projectInfo?.worktree_root) || busy || !selectedLocalBranch || git.changed.length > 0 || dirty || externalChanged} onClick={() => void runGitManagerAction('git_switch_branch', { name: selectedLocalBranch }, `${selectedLocalBranch} に切り替えます。未コミット変更はありませんか？`)}>選択したブランチへ切替</button>{(git.changed.length > 0 || dirty || externalChanged) && <small className="git-manager-warning">未コミット変更またはエディターの未保存編集があるため切替できません。先に保存するか変更を解決してください。</small>}<label>作成元<select value={branchStart} onChange={(event) => setBranchStart(event.target.value)}>{gitBranches.map((branch) => <option key={branch.name} value={branch.name}>{branch.remote ? `Remote: ${branch.name}` : `Local: ${branch.name}`}</option>)}</select></label><div className="git-manager-inline"><input aria-label="新しいブランチ名" value={newBranchName} onChange={(event) => setNewBranchName(event.target.value)} placeholder="新しいブランチ名" /><button disabled={Boolean(projectInfo?.worktree_root) || busy || !newBranchName.trim() || !branchStart} onClick={() => void runGitManagerAction('git_create_branch', { name: newBranchName.trim(), startPoint: branchStart }, `「${newBranchName}」を ${branchStart} から作成しますか？`)}>作成</button></div><h4>既知のリモートブランチ</h4><ul className="git-manager-list">{gitBranches.filter((branch) => branch.remote).map((branch) => <li key={branch.name}>{branch.name}</li>)}{!gitBranches.some((branch) => branch.remote) && <li>Fetch 済みのリモートブランチはありません。</li>}</ul></section>
        <section className="git-manager-section"><h3><HardDrive size={15} />リモートと upstream</h3><label>リモート<select value={selectedRemote} onChange={(event) => { setSelectedRemote(event.target.value); setPushPreview(null); setRemoteNameInput(event.target.value); setSelectedRemoteBranch('') }}>{gitRemotes.map((remote) => <option key={remote.name} value={remote.name}>{remote.name} · {remote.url}</option>)}</select></label><div className="git-manager-inline"><input aria-label="リモート名" value={remoteNameInput} onChange={(event) => setRemoteNameInput(event.target.value)} placeholder="リモート名" /><input aria-label="リモート URL" type="password" autoComplete="off" value={remoteUrlInput} onChange={(event) => setRemoteUrlInput(event.target.value)} placeholder="リモート URL（入力値は非表示）" /></div><div className="git-manager-actions"><button disabled={busy || !remoteNameInput || !remoteUrlInput} onClick={() => void runGitManagerAction('git_remote_change', { action: 'add', name: remoteNameInput, url: remoteUrlInput }, `リモート「${remoteNameInput}」に ${maskRemoteUrl(remoteUrlInput)} を追加しますか？`)}>追加</button><button disabled={busy || !selectedRemote || !remoteUrlInput} onClick={() => void runGitManagerAction('git_remote_change', { action: 'set-url', name: selectedRemote, url: remoteUrlInput }, `リモート「${selectedRemote}」の URL を ${maskRemoteUrl(remoteUrlInput)} に変更しますか？`)}>URL 変更</button><button disabled={busy || !selectedRemote} onClick={() => void runGitManagerAction('git_remote_change', { action: 'remove', name: selectedRemote, url: null }, `リモート「${selectedRemote}」を削除します。関連する追跡設定と URL がローカル設定から削除されます。リモートサーバー上のデータは削除されません。続行しますか？`)}>削除</button></div><label>既存 upstream に設定<select value={selectedRemoteBranch} onChange={(event) => setSelectedRemoteBranch(event.target.value)}><option value="">リモートブランチを選択</option>{remoteBranches.map((branch) => <option key={branch} value={branch}>{selectedRemote}/{branch}</option>)}</select></label><button disabled={busy || !selectedRemote || !selectedRemoteBranch} onClick={() => void runGitManagerAction('git_set_upstream', { remote: selectedRemote, branch: selectedRemoteBranch }, `現在のブランチ ${git.branch} の upstream を ${selectedRemote}/${selectedRemoteBranch} に設定しますか？`)}>選択した既存 upstream を設定</button><small>一覧にないブランチは既存 upstream として設定できません。未作成のブランチは Push で作成してください。</small></section>

        <section className="git-manager-section"><h3><CloudDownload size={15} />Fetch / Pull</h3><label>Fetch 対象リモート<select value={selectedRemote} onChange={(event) => { setSelectedRemote(event.target.value); setPushPreview(null) }}>{gitRemotes.map((remote) => <option key={remote.name} value={remote.name}>{remote.name}</option>)}</select></label><div className="git-manager-actions"><button disabled={busy || !selectedRemote} onClick={() => void runGitManagerAction('git_fetch', { remote: selectedRemote }, `${selectedRemote} から Fetch します。作業ツリーのファイルは変更しません。`)}><CloudDownload size={14} />Fetch</button><button disabled={busy} onClick={() => void runGitManagerAction('git_fetch', { remote: null }, 'すべての remote から Fetch します。作業ツリーのファイルは変更しません。')}>すべて Fetch</button></div><label>Pull 対象ブランチ<select value={selectedRemoteBranch} onChange={(event) => setSelectedRemoteBranch(event.target.value)}><option value="">リモートブランチを選択</option>{remoteBranches.map((branch) => <option key={branch} value={branch}>{selectedRemote}/{branch}</option>)}</select></label><button disabled={busy || !selectedRemote || !selectedRemoteBranch || git.changed.length > 0 || dirty || externalChanged} onClick={() => void runGitManagerAction('git_pull_ff', { remote: selectedRemote, branch: selectedRemoteBranch }, `Fetch 後、${selectedRemote}/${selectedRemoteBranch} を Fast-forward のみで Pull します。未コミット変更がないことを確認してください。分岐や競合の可能性がある場合は停止します。`)}><RefreshCw size={14} />Fast-forward Pull</button><small>Fetch と Pull は別操作です。Pull は未コミット変更がある場合に拒否し、fast-forward できない履歴は統合しません。</small></section>
      </div></details><footer><button className="setup-cancel" onClick={() => setGitManagerOpen(false)}>閉じる</button><button className="setup-confirm" disabled={busy} onClick={() => void loadGitManager()}><RefreshCw size={14} />状態を再読み込み</button></footer></section></div>}
    {treeMenu && <div className="tree-menu-shade" onClick={() => setTreeMenu(null)} onContextMenu={(event) => { event.preventDefault(); setTreeMenu(null) }}>
      <div className="tree-context-menu" role="menu" style={{ left: treeMenu.x, top: treeMenu.y }} onClick={(event) => event.stopPropagation()}>
        {treeMenu.directory && <><button role="menuitem" onClick={() => { setTreeMenu(null); void beginCreate('document') }}>新しい文書</button><button role="menuitem" onClick={() => { setTreeMenu(null); void beginCreate('folder') }}>新しいフォルダー</button></>}
        <button role="menuitem" disabled={busy || treeMenu.path === documentRoot} onClick={() => { setRenamePath(treeMenu.path); setRenameName(treeMenu.path.split('/').at(-1) ?? ''); setTreeMenu(null) }}>名前を変更</button>
        <button role="menuitem" className="delete-entry" disabled={busy || treeMenu.path === documentRoot} onClick={() => { const path = treeMenu.path; setTreeMenu(null); void changeTreeEntry(path, '', '', true) }}>削除</button>
      </div>
    </div>}
    {renamePath && <div className="modal-backdrop"><section className="modal create-item-modal" role="dialog" aria-modal="true" aria-label="名前を変更"><header><h2>名前を変更</h2><button disabled={busy} onClick={() => setRenamePath(null)}><X size={17} /></button></header>
      <form onSubmit={(event) => { event.preventDefault(); void changeTreeEntry(renamePath, parentDirectory(renamePath), renameName) }}>
        <p className="create-item-hint">{renamePath}</p><label>新しい名前（文書は拡張子を含む）<input autoFocus value={renameName} onChange={(event) => setRenameName(event.target.value)} disabled={busy} /></label>
        <p className="create-item-hint">同名項目は上書きしません。相対リンクは自動更新されません。</p><footer><button type="button" disabled={busy} onClick={() => setRenamePath(null)}>キャンセル</button><button type="submit" disabled={busy || !renameName.trim() || renameName === renamePath.split('/').at(-1)}>名前を変更</button></footer>
      </form></section></div>}
    {newItemKind && <div className="modal-backdrop" onClick={() => setNewItemKind(null)}><section className="modal create-item-modal" onClick={(event) => event.stopPropagation()}><header><div><h2>{newItemKind === 'document' ? '新しい文書' : '新しいフォルダー'}</h2><p>設計書ディレクトリ内に新規作成します。</p></div><button onClick={() => setNewItemKind(null)}><X size={17} /></button></header><form onSubmit={(event) => { event.preventDefault(); void createNewItem() }}><p className="create-item-hint">保存先: <b>{newItemParent}</b></p><label>{newItemKind === 'document' ? '文書名' : 'フォルダー名'}<input autoFocus value={newItemName} onChange={(event) => setNewItemName(event.target.value)} placeholder={newItemKind === 'document' ? '例: ゲーム概要' : '例: 07_運用仕様'} /></label>{newItemKind === 'document' && <label>形式<select value={newDocumentFormat} onChange={(event) => setNewDocumentFormat(event.target.value as 'markdown' | 'json')}><option value="markdown">Markdown (.md)</option><option value="json">JSON (.json)</option></select></label>}<p className="create-item-hint">同名項目は上書きしません。パス区切り文字や親フォルダーへの移動は使用できません。</p><footer><button type="button" className="setup-cancel" onClick={() => setNewItemKind(null)}>キャンセル</button><button type="submit" className="setup-confirm" disabled={busy || !newItemName.trim()}><Plus size={15} />作成</button></footer></form></section></div>}
    {setupPreview && <div className="modal-backdrop" onClick={() => setSetupPreview(null)}><section className="modal setup-modal" onClick={(event) => event.stopPropagation()}><header><div><h2>設計書ディレクトリの作成プレビュー</h2><p>{repo}</p></div><button onClick={() => setSetupPreview(null)}><X size={17} /></button></header><p className="setup-summary">以下の {setupPreview.entries.length} 項目だけを作成します。既存の {setupPreview.changed_files.length} 件の Git 変更はそのまま保持します。</p><ul className="setup-list">{setupPreview.entries.map((entry) => <li key={entry.path}><Folder size={14} /><code>{entry.path}</code><span>{entry.kind === 'directory' ? 'ディレクトリ' : 'ファイル'}</span></li>)}</ul><footer><button className="setup-cancel" onClick={() => setSetupPreview(null)}>キャンセル</button><button className="setup-confirm" disabled={busy} onClick={() => void createDocumentSetup()}><Plus size={15} />確認して作成</button></footer></section></div>}
  </main>
}

export default App
