use super::*;
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};
use tauri::Manager;

#[derive(Clone, Serialize)]
pub struct WorkspacePreview {
    pub unity_root: String,
    pub git_root: String,
    pub worktree: String,
    pub development_branch: String,
    pub head: String,
    pub upstream: Option<String>,
    pub existing: bool,
    pub blockers: Vec<String>,
    pub source_changes: Vec<String>,
    pub staged: Vec<String>,
    pub document_changes: Vec<String>,
    pub unpushed_documents: Vec<String>,
    pub worktree_changes: Vec<String>,
    pub sparse_ready: bool,
    pub sparse_scope: String,
}


// 独立設計書リポジトリだけを受け入れ、ゲーム側の Git 操作を防止する。
const DESIGN_ROOT: &str = r"D:\UnityProjects\Xiuxian_DesignBook";
fn standalone(root: &Path) -> Result<(), String> {
    let canonical = root.canonicalize().map_err(|_| "設計書パスを解決できません")?;
    let root = canonical.as_path();
    if !root.join(".git").is_dir() || branch(root)? != "main" {
        return Err("独立した設計書リポジトリの main を開いてください".into());
    }
    reject_link(&fs::symlink_metadata(root.join(".git")).map_err(|_| "Git 管理情報を検査できません")?)?;
    if git_text(root, &["config", "--get", "designbook.repository"])? != "XiuxianGame_DesignDocs" {
        return Err("設計書リポジトリの識別情報が一致しません".into());
    }
    if discover_git_root(root)? != root.canonicalize().map_err(|_| "設計書パスを解決できません")? || common(root)? != root.join(".git").canonicalize().map_err(|_| "Git 管理情報を解決できません")? {
        return Err("共有 Worktree は使用できません".into());
    }
    // 作成承認前は origin 未設定で閲覧できる。設定後は送受信先を固定する。
    if let Ok(url) = git_text(root, &["remote", "get-url", "origin"]) {
        if url != "https://github.com/Ron0609/XiuxianGame_DesignDocs.git" {
            return Err("origin が XiuxianGame_DesignDocs と一致しません".into());
        }
        let push = git_text(root, &["remote", "get-url", "--push", "origin"])?;
        if push != url { return Err("送信用 URL が origin と一致しません".into()); }
    }
    Ok(())
}
fn standalone_open() -> Result<Repository, String> {
    let root = Path::new(DESIGN_ROOT).canonicalize().map_err(|_| "独立設計書リポジトリがありません")?;
    standalone(&root)?;
    let documents = root.join("DesignDocs");
    reject_link(&fs::symlink_metadata(&documents).map_err(|_| "DesignDocs がありません")?)?;
    Ok(Repository { source_root: Some(root.clone()), root: root.clone(), git_root: root, documents, documents_relative: "DesignDocs".into(), git_documents_relative: "DesignDocs".into(), has_documents: true })
}

fn common(root: &Path) -> Result<PathBuf, String> {
    let path = git_text(root, &["rev-parse", "--git-common-dir"])?;
    root.join(path).canonicalize().map_err(|_| "Git 共通ディレクトリを確認できません".into())
}

fn branch(root: &Path) -> Result<String, String> { git_text(root, &["symbolic-ref", "--short", "HEAD"]) }

fn docs_scope(source: &Path, git_root: &Path) -> Result<String, String> {
    let relative = source.strip_prefix(git_root).map_err(|_| "Unity ルートが Git 範囲外です")?;
    Ok(relative.join("DesignDocs").to_string_lossy().replace('\\', "/"))
}

fn target_path(source: &Path) -> Result<PathBuf, String> {
    let source = source.canonicalize().map_err(|_| "Unity パスを解決できません")?;
    let git_root = discover_git_root(&source)?;
    let name = git_root.file_name().ok_or("Git リポジトリ名を取得できません")?.to_string_lossy();
    Ok(git_root.parent().ok_or("親ディレクトリがありません")?.join(format!("{name}_DesignBook")))
}

pub fn inspect(path: &Path) -> Result<WorkspacePreview, String> {
    if path.canonicalize().ok() == Path::new(DESIGN_ROOT).canonicalize().ok() && Path::new(DESIGN_ROOT).join(".git").is_dir() {
        let repo = standalone_open()?;
        let root = repo.root.to_string_lossy().to_string();
        return Ok(WorkspacePreview { unity_root: root.clone(), git_root: root.clone(), worktree: root, development_branch: "main".into(), head: git_text(&repo.root, &["rev-parse", "HEAD"])?, upstream: git_text(&repo.root, &["rev-parse", "--abbrev-ref", "@{upstream}"]).ok(), existing: true, blockers: vec![], source_changes: vec![], staged: vec![], document_changes: vec![], unpushed_documents: vec![], worktree_changes: vec![], sparse_ready: true, sparse_scope: "DesignDocs".into() });
    }
    let source = path.canonicalize().map_err(|_| "Unity プロジェクトを開けません")?;
    validate_unity_root(&source)?;
    let root = discover_git_root(&source)?;
    let target = target_path(&source)?;
    let status = git_status_impl(&root)?;
    let scope = docs_scope(&source, &root)?;
    let changes = git(&root, &["status", "--porcelain=v1", "-z", "--untracked-files=all", "--", &scope])?;
    if !changes.status.success() { return Err(safe_git_output(&changes)); }
    let document_changes: Vec<_> = parse_git_changes(&changes.stdout)?.into_iter().map(|c| format!("{} {}", c.status, c.path)).collect();
    let staged = git_text(&root, &["-c", "core.quotePath=false", "diff", "--cached", "--name-status"])?;
    let head = git_text(&root, &["rev-parse", "HEAD"])?;
    let upstream = git_text(&root, &["rev-parse", "--abbrev-ref", "@{upstream}"]).ok();
    let mut blockers: Vec<String> = Vec::new();
    let main_exists = git(&root, &["show-ref", "--verify", "--quiet", "refs/heads/main"])?.status.success();
    if !main_exists { blockers.push("ローカルの main がありません。自動作成・自動切替は行いません。".into()); }
    let mut unpushed_documents = if main_exists && git(&root, &["show-ref", "--verify", "--quiet", "refs/remotes/origin/main"])?.status.success() {
        git_text(&root, &["log", "--oneline", "origin/main..main", "--", &scope])?.lines().map(String::from).collect::<Vec<_>>()
    } else { blockers.push("origin/main をローカルで確認できません。移行前にリモート履歴を確認してください。".into()); Vec::new() };
    if main_exists {
        let source_only = git_text(&root, &["log", "--oneline", "main..HEAD", "--", &scope])?;
        unpushed_documents.extend(source_only.lines().map(|l| format!("開発側のみ: {l}")));
    }
    let existing = fs::symlink_metadata(&target).is_ok();
    let mut worktree_changes = Vec::new();
    if existing {
        blockers.retain(|message| !message.starts_with("origin/main"));
        if let Err(error) = validate_identity(&source, &target) { blockers.push(error); }
        else {
            worktree_changes = git_status_impl(&target)?.changes.iter().map(|c| format!("{} {}", c.status, c.path)).collect();
            if validate_sparse(&source, &target).is_err() {
                if let Err(error) = conversion_preflight(&source, &target) { blockers.push(error); }
            }
        }
    } else {
        if !document_changes.is_empty() { blockers.push("元の DesignDocs に未コミット変更があります。差分を確認・保護してから移行してください。".into()); }
        if !unpushed_documents.is_empty() { blockers.push("main または開発側に未共有の設計書コミットがあります。共有方針を確認してから移行してください。".into()); }
        // `worktree list` uses actual refs, including linked worktrees with spaces in their paths.
        let list = git_text(&root, &["worktree", "list", "--porcelain"])?;
        if list.lines().any(|line| line == "branch refs/heads/main") {
            blockers.push("main は別のワークツリーで使用中です。強制切替や開発ブランチの自動作成は行いません。".into());
        }
    }
    Ok(WorkspacePreview { unity_root: source.to_string_lossy().into(), git_root: root.to_string_lossy().into(), worktree: target.to_string_lossy().into(), development_branch: status.branch, head, upstream, existing, blockers, source_changes: status.changes.iter().map(|c| format!("{} {}", c.status, c.path)).collect(), staged: staged.lines().map(String::from).collect(), document_changes, unpushed_documents, worktree_changes, sparse_ready: existing && validate_sparse(&source, &target).is_ok(), sparse_scope: scope })
}

fn validate_identity(source: &Path, target: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(target).map_err(|_| "設計書 Worktree が存在しません")?;
    if reject_link(&metadata).is_err() || !metadata.is_dir() { return Err("Worktree のパスがリンクまたはディレクトリ以外です".into()); }
    if !target.join(".git").is_file() || reject_link(&fs::symlink_metadata(target.join(".git")).map_err(|_| "Worktree の .git を確認できません")?).is_err() {
        return Err("既存ディレクトリは正規のリンク Worktree ではありません。上書きしません。".into());
    }
    let actual = discover_git_root(target)?;
    if actual != target.canonicalize().map_err(|_| "Worktree を解決できません")? || common(source)? != common(target)? {
        return Err("既存ディレクトリは同じ Git リポジトリの Worktree ではありません".into());
    }
    let worktrees = git_text(source, &["worktree", "list", "--porcelain"])?;
    let registered = worktrees.split("\n\n").any(|record| record.lines().find_map(|l| l.strip_prefix("worktree ")).and_then(|p| Path::new(p).canonicalize().ok()).is_some_and(|p| p == actual));
    if !registered { return Err("Git に登録された Worktree ではありません".into()); }
    if branch(target)? != "main" { return Err("設計書 Worktree は main である必要があります".into()); }
    // All file operations in this session are confined to DesignDocs, never to a fallback docs directory.
    if let Ok(metadata) = fs::symlink_metadata(target.join("DesignDocs")) {
        if reject_link(&metadata).is_err() || !metadata.is_dir() { return Err("DesignDocs がリンクまたはディレクトリ以外です".into()); }
    }
    Ok(())
}

fn is_scoped(path: &str, scope: &str) -> bool {
    path == scope || path.strip_prefix(scope).is_some_and(|rest| rest.starts_with('/'))
}

fn sparse_pattern(scope: &str) -> String {
    let mut pattern = String::from("/");
    for ch in scope.chars() { if matches!(ch, '\\' | '[' | ']' | '*' | '?' | '!' | '#') { pattern.push('\\'); } pattern.push(ch); }
    pattern.push('/');
    pattern
}

fn sparse_scope(source: &Path) -> Result<String, String> { docs_scope(source, &discover_git_root(source)?) }

fn config_flag(root: &Path, key: &str) -> bool {
    git_text(root, &["config", "--bool", "--get", key]).is_ok_and(|value| value == "true")
}

// Worktree-local sparse config is essential: shared core.sparseCheckout would affect Unity.
fn validate_sparse(source: &Path, target: &Path) -> Result<(), String> {
    validate_identity(source, target)?;
    if !config_flag(target, "extensions.worktreeConfig") ||
        git_text(target, &["config", "--worktree", "--bool", "core.sparseCheckout"]).as_deref() != Ok("true") ||
        git_text(target, &["config", "--worktree", "--bool", "core.sparseCheckoutCone"]).as_deref() != Ok("false") ||
        config_flag(target, "index.sparse") {
        return Err("DesignDocs 専用 Sparse Checkout の設定が必要です。変換内容を確認してください。".into());
    }
    let scope = sparse_scope(source)?;
    let list = git_text(target, &["sparse-checkout", "list"])?;
    if list != sparse_pattern(&scope) { return Err("Sparse Checkout の範囲が DesignDocs と一致しません".into()); }
    Ok(())
}

pub fn validate_target(source: &Path, target: &Path) -> Result<(), String> {
    if target.join(".git").is_dir() { return standalone(target); }
    validate_sparse(source, target)
}

// Inventory outside the selected scope, including ignored files. Git sparse-checkout can
// prune ignored content, so a clean `git status` alone is insufficient before conversion.
fn outside_inventory(target: &Path, scope: &str) -> Result<Vec<String>, String> {
    let mut pending = vec![target.to_path_buf()];
    let mut files = Vec::new();
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory).map_err(|_| "Worktree の内容を検査できません")? {
            let entry = entry.map_err(|_| "Worktree の項目を検査できません")?;
            let path = entry.path();
            let relative = path.strip_prefix(target).map_err(|_| "Worktree 範囲外です")?.to_string_lossy().replace('\\', "/");
            if relative == ".git" || is_scoped(&relative, scope) { continue; }
            let metadata = fs::symlink_metadata(&path).map_err(|_| "項目の属性を検査できません")?;
            reject_link(&metadata)?;
            if metadata.is_dir() {
                if !scope.starts_with(&format!("{relative}/")) { files.push(format!("{relative}/")); }
                pending.push(path);
            } else { files.push(relative); }
        }
    }
    Ok(files)
}

fn conversion_preflight(source: &Path, target: &Path) -> Result<(), String> {
    validate_identity(source, target)?;
    if !git_status_impl(target)?.clean { return Err("Worktree に未コミット・未追跡変更があります。Sparse 変換を停止しました。既存ファイルは保持しています。".into()); }
    if ["MERGE_HEAD", "rebase-merge", "rebase-apply", "CHERRY_PICK_HEAD"].iter().any(|name| git_text(target, &["rev-parse", "--git-path", name]).is_ok_and(|p| target.join(p).exists())) { return Err("Git 操作が進行中のため Sparse 変換できません".into()); }
    let tracked = git(target, &["ls-files", "-z"])?;
    if !tracked.status.success() { return Err("追跡ファイルを検査できません".into()); }
    let tracked: std::collections::HashSet<String> = tracked.stdout.split(|b| *b == 0).filter(|p| !p.is_empty()).map(|p| String::from_utf8_lossy(p).into_owned()).collect();
    for path in outside_inventory(target, &sparse_scope(source)?)? {
        if !tracked.contains(&path) && !(path.ends_with('/') && tracked.iter().any(|file| file.starts_with(&path))) { return Err(format!("Sparse 範囲外に既存の未追跡・無視ファイルがあります: {path}。削除せず変換を停止しました。")); }
    }
    Ok(())
}

fn source_sparse_state(source: &Path) -> Result<(bool, bool, bool, Vec<u8>), String> {
    let index = git(source, &["ls-files", "-t", "-z"])?;
    if !index.status.success() { return Err("Unity 側の index 状態を検査できません".into()); }
    Ok((config_flag(source, "core.sparseCheckout"), config_flag(source, "core.sparseCheckoutCone"), config_flag(source, "index.sparse"), index.stdout))
}

fn configure_sparse(source: &Path, target: &Path, freshly_created: bool) -> Result<(), String> {
    validate_identity(source, target)?;
    if !freshly_created { conversion_preflight(source, target)?; }
    let before = source_sparse_state(source)?;
    if before.0 && !config_flag(source, "extensions.worktreeConfig") { return Err("Unity 側が共有 Sparse 設定を使用しています。設定の分離を確認してから再試行してください。".into()); }
    let scope = sparse_scope(source)?;
    // `set` enables extensions.worktreeConfig and stores sparsity per worktree.
    // Cone mode includes ancestor files; use one anchored directory pattern instead.
    git_command_result(target, &["-c", "core.hooksPath=NUL", "-c", "submodule.recurse=false", "sparse-checkout", "set", "--no-cone", "--no-sparse-index", &sparse_pattern(&scope)], "DesignDocs 専用 Sparse Checkout 設定")?;
    if freshly_created {
        // --no-checkout deliberately leaves the linked index empty. Populate it from HEAD
        // only after sparsity is active, so no Unity assets are ever materialized.
        git_command_result(target, &["-c", "core.hooksPath=NUL", "-c", "submodule.recurse=false", "read-tree", "-mu", "HEAD"], "Sparse index の初期化")?;
    }
    if !git_status_impl(target)?.clean { return Err("Sparse 設定後に未コミット変更を検出しました。Commit せず停止します。".into()); }
    if source_sparse_state(source)? != before { return Err("Unity 側の Sparse/index 状態が変化しました。自動修復せず処理を停止します。".into()); }
    validate_sparse(source, target)?;
    let outside = outside_inventory(target, &scope)?;
    if !outside.is_empty() { return Err(format!("Sparse 範囲外のファイルが残っています。削除せず停止しました: {}", outside.join(", "))); }
    Ok(())
}

// Git for Windows does not accept Rust's verbatim \\?\ prefix for worktree targets.
fn git_path(path: &Path) -> String {
    let value = path.to_string_lossy().replace('\\', "/");
    if let Some(unc) = value.strip_prefix("//?/UNC/") { format!("//{unc}") }
    else { value.strip_prefix("//?/").unwrap_or(&value).to_string() }
}

pub fn create(path: &Path, confirmed: bool) -> Result<WorkspacePreview, String> {
    if !confirmed { return Err("Worktree 作成は確認されていません".into()); }
    let preview = inspect(path)?;
    if !preview.blockers.is_empty() { return Err(preview.blockers.join("\n")); }
    if preview.existing {
        if !preview.sparse_ready { configure_sparse(Path::new(&preview.unity_root), Path::new(&preview.worktree), false)?; }
        return inspect(Path::new(&preview.unity_root));
    }
    let source = Path::new(&preview.unity_root);
    let target = Path::new(&preview.worktree);
    // Git itself refuses a new directory collision and branch occupancy race; never use --force.
    fs::create_dir(target).map_err(|_| "作成先が既に存在するか、作成権限がありません。上書きしません。")?;
    git_command_result(Path::new(&preview.git_root), &["-c", "core.hooksPath=NUL", "worktree", "add", "--no-checkout", "--", &git_path(target), "main"], "設計書 Worktree 作成")?;
    configure_sparse(source, target, true)?;
    inspect(source)
}

pub fn open(path: &Path) -> Result<Repository, String> {
    // 選択画面や旧保存パスからも、移行後は固定された独立リポジトリを開く。
    if path.canonicalize().ok() == Path::new(DESIGN_ROOT).canonicalize().ok() && Path::new(DESIGN_ROOT).join(".git").is_dir() { return standalone_open(); }
    let preview = inspect(path)?;
    if !preview.existing || !preview.blockers.is_empty() { return Err(format!("設計書 Worktree を開けません。{}", preview.blockers.join("\n"))); }
    let source = PathBuf::from(preview.unity_root);
    let target = PathBuf::from(preview.worktree).canonicalize().map_err(|_| "Worktree を読み込めません")?;
    validate_sparse(&source, &target)?;
    let source_root = discover_git_root(&source)?;
    let relative_unity = source.strip_prefix(&source_root).map_err(|_| "Unity パスが範囲外です")?;
    let unity_in_worktree = target.join(relative_unity);
    // This is a design-only worktree. Assets and ProjectSettings are intentionally absent.
    let mut checked = target.clone();
    for component in relative_unity.components() {
        checked.push(component);
        match fs::symlink_metadata(&checked) {
            Ok(metadata) => reject_link(&metadata)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(_) => return Err("設計書の親パスを検査できません".into()),
        }
    }
    let documents = unity_in_worktree.join("DesignDocs");
    if let Ok(metadata) = fs::symlink_metadata(&documents) {
        if reject_link(&metadata).is_err() || !metadata.is_dir() { return Err("DesignDocs にパス競合があります".into()); }
    }
    let has_documents = documents.is_dir();
    let git_documents_relative = documents.strip_prefix(&target).map_err(|_| "設計書パスが範囲外です")?.to_string_lossy().replace('\\', "/");
    Ok(Repository { root: unity_in_worktree, git_root: target, documents, documents_relative: "DesignDocs".into(), git_documents_relative, has_documents, source_root: Some(source) })
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ChangeFile { pub path: String, pub previous_path: Option<String>, pub status: String, pub hunks: Vec<ChangeHunk>, pub patch: String }
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ChangeHunk { pub line: usize, pub heading: String, pub removed: bool, pub text: String }
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct UpdateRecord { pub id: String, pub before: String, pub after: String, pub timestamp: u64, pub read: bool, pub applied: bool, pub files: Vec<ChangeFile> }
#[derive(Serialize)]
pub struct SyncResult { pub message: String, pub records: Vec<UpdateRecord>, pub new_id: Option<String> }

static JOURNAL_LOCK: Mutex<()> = Mutex::new(());

fn record_path(app: &tauri::AppHandle, repo: &Repository) -> Result<PathBuf, String> {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    repo.git_root.hash(&mut hash);
    let directory = app.path().app_data_dir().map_err(|_| "アプリデータの保存先を取得できません")?.join("github-updates");
    fs::create_dir_all(&directory).map_err(|_| "更新記録の保存先を作成できません")?;
    Ok(directory.join(format!("{:x}.json", hash.finish())))
}

pub fn load_records(path: &Path) -> Result<Vec<UpdateRecord>, String> {
    if !path.exists() { return Ok(Vec::new()); }
    let data = fs::read(path).map_err(|_| "更新記録を読み取れません")?;
    serde_json::from_slice(&data).map_err(|_| "更新記録が破損しています。上書きせず同期を停止しました".into())
}
fn save_records(path: &Path, records: &[UpdateRecord]) -> Result<(), String> {
    atomic_write(path, &serde_json::to_vec_pretty(records).map_err(|_| "更新記録を変換できません")?)
}

fn diff_records(repo: &Repository, before: &str, after: &str) -> Result<Vec<ChangeFile>, String> {
    let output = git(&repo.git_root, &["diff", "--name-status", "-z", "--find-renames", before, after, "--", &repo.git_documents_relative])?;
    if !output.status.success() { return Err(safe_git_output(&output)); }
    let mut parts = output.stdout.split(|b| *b == 0).filter(|p| !p.is_empty());
    let mut files = Vec::new();
    while let Some(status) = parts.next() {
        let status = String::from_utf8_lossy(status).into_owned();
        let first = String::from_utf8_lossy(parts.next().ok_or("差分のパスがありません")?).into_owned();
        let (path, previous_path) = if status.starts_with('R') || status.starts_with('C') {
            (String::from_utf8_lossy(parts.next().ok_or("変更先のパスがありません")?).into_owned(), Some(first))
        } else { (first, None) };
        let mut args = vec!["diff", "--no-ext-diff", "--no-textconv", "--unified=3", "--find-renames", before, after, "--", path.as_str()];
        if let Some(old) = &previous_path { args.push(old); }
        let raw = git(&repo.git_root, &args)?;
        if !raw.status.success() { return Err("ファイル差分を取得できません".into()); }
        if raw.stdout.len() > 2_000_000 { files.push(ChangeFile { path, previous_path, status, hunks: Vec::new(), patch: "差分が 2 MB を超えるため省略しました。".into() }); continue; }
        let patch = String::from_utf8_lossy(&raw.stdout).into_owned();
        let new_content = git(&repo.git_root, &["show", &format!("{after}:{path}")]).ok()
            .filter(|o| o.status.success() && o.stdout.len() <= 5_000_000)
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
        let hunks = parse_hunks(&patch, &new_content, status.starts_with('D'));
        files.push(ChangeFile { path, previous_path, status, hunks, patch });
    }
    Ok(files)
}

fn parse_hunks(patch: &str, content: &str, deleted: bool) -> Vec<ChangeHunk> {
    let mut hunks = Vec::new();
    let lines: Vec<_> = content.lines().collect();
    let mut patch_lines = patch.lines().peekable();
    while let Some(header) = patch_lines.next() {
        if !header.starts_with("@@ ") { continue; }
        let range = header.split("@@").nth(1).unwrap_or("");
        let mut new_line: usize = range.split_whitespace().find_map(|p| p.strip_prefix('+')).unwrap_or("0").split(',').next().and_then(|n| n.parse().ok()).unwrap_or(0);
        let mut first_added = None;
        while patch_lines.peek().is_some_and(|l| !l.starts_with("@@ ")) {
            let line = patch_lines.next().unwrap();
            if line.starts_with('+') { if first_added.is_none() { first_added = Some(new_line); } new_line += 1; }
            else if line.starts_with(' ') { new_line += 1; }
        }
        let position = first_added.unwrap_or(0);
        let heading = lines.iter().take(position).rev().find(|l| l.starts_with('#')).map(|s| s.trim_start_matches('#').trim().to_string())
            .unwrap_or_else(|| format!("行 {position}"));
        hunks.push(ChangeHunk { line: position, heading, removed: deleted || first_added.is_none(), text: header.into() });
    }
    hunks
}

// Drain both pipes while waiting so a credential helper or a full stderr pipe cannot hang the UI.
fn timed_fetch(root: &Path, args: &[&str]) -> Result<Output, String> {
    use std::io::Read;
    use std::process::Stdio;
    let mut child = git_command(root, args).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|_| "Git を起動できません")?;
    let stdout = child.stdout.take().ok_or("Git stdout を取得できません")?;
    let stderr = child.stderr.take().ok_or("Git stderr を取得できません")?;
    let out = std::thread::spawn(move || { let mut data = Vec::new(); let _ = stdout.take(8_000_000).read_to_end(&mut data); data });
    let err = std::thread::spawn(move || { let mut data = Vec::new(); let _ = stderr.take(8_000_000).read_to_end(&mut data); data });
    let start = std::time::Instant::now();
    loop {
        if let Some(status) = child.try_wait().map_err(|_| "Git の状態を取得できません")? {
            return Ok(Output { status, stdout: out.join().unwrap_or_default(), stderr: err.join().unwrap_or_default() });
        }
        if start.elapsed() > std::time::Duration::from_secs(45) {
            let _ = child.kill(); let _ = child.wait();
            return Err("接続確認がタイムアウトしました。ローカルの設計書は保持されています。接続を確認して再試行してください。".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

fn require_managed(repo: &Repository) -> Result<(), String> {
    let source = repo.source_root.as_ref().ok_or("独立 Worktree を開いてください")?;
    validate_target(source, &repo.git_root)
}

pub fn sync(repo: &Repository, journal: &Path, unsaved: bool) -> Result<SyncResult, String> {
    let _journal_guard = JOURNAL_LOCK.lock().map_err(|_| "更新記録をロックできません")?;
    require_managed(repo)?;
    let mut records = load_records(journal)?;
    let before = git_text(&repo.git_root, &["rev-parse", "HEAD"])?;
    // A journal entry is durable before fast-forward. Recover a successful update after a crash.
    let mut recovered = None;
    for record in &mut records {
        if !record.applied && record.after == before { record.applied = true; recovered = Some(record.id.clone()); }
    }
    save_records(journal, &records)?;
    let paused = |message: String, records| Ok(SyncResult { message, records, new_id: recovered.clone() });
    if unsaved { return paused("未保存の編集があるため自動同期を停止しました。保存してから再確認してください。".into(), records); }
    if !git_status_impl(&repo.git_root)?.clean { return paused("設計書 Worktree に未コミット変更があります。Commit するか内容を確認してから再同期してください。".into(), records); }
    if ["MERGE_HEAD", "rebase-merge", "rebase-apply", "CHERRY_PICK_HEAD"].iter().any(|name| git_text(&repo.git_root, &["rev-parse", "--git-path", name]).ok().is_some_and(|p| repo.git_root.join(p).exists())) {
        return paused("Git 操作が進行中です。解決してから再同期してください。".into(), records);
    }
    if require_remote(&repo.git_root, "origin").is_err() { return paused("GitHub 作成・初回 Push の承認待ちです。ローカル設計書を表示しています。".into(), records); }
    let fetch = timed_fetch(&repo.git_root, &["fetch", "--no-tags", "--no-recurse-submodules", "origin", "+refs/heads/main:refs/remotes/origin/main"])?;
    if !fetch.status.success() { return paused(format!("オフラインで表示しています。取得に失敗しました。接続・認証を確認して再試行してください。\n{}", safe_git_output(&fetch)), records); }
    let after = git_text(&repo.git_root, &["rev-parse", "refs/remotes/origin/main^{commit}"])?;
    if before == after { return paused("設計書は最新です。".into(), records); }
    if !git(&repo.git_root, &["merge-base", "--is-ancestor", &before, &after])?.status.success() {
        let message = if git(&repo.git_root, &["merge-base", "--is-ancestor", &after, &before])?.status.success() { "ローカルが先行しています。自動同期は不要です。" } else { "履歴が分岐しています。自動 merge/rebase は行いません。履歴を確認してください。" };
        return paused(message.into(), records);
    }
    require_managed(repo)?;
    if !git_status_impl(&repo.git_root)?.clean || git_text(&repo.git_root, &["rev-parse", "HEAD"])? != before { return paused("同期直前に状態が変わりました。再確認してください。".into(), records); }
    // Git may overwrite ignored untracked files during a fast-forward; detect collisions
    // independently of porcelain and of merge's --no-overwrite-ignore handling.
    let ignored = git(&repo.git_root, &["ls-files", "--others", "--ignored", "--exclude-standard", "-z"])?;
    let incoming = git(&repo.git_root, &["diff", "--name-only", "--no-renames", "-z", &before, &after])?;
    if !ignored.status.success() || !incoming.status.success() { return paused("無視ファイルと更新内容の衝突を検査できません。同期を停止しました。".into(), records); }
    let incoming: Vec<_> = incoming.stdout.split(|b| *b == 0).filter(|p| !p.is_empty()).map(|p| String::from_utf8_lossy(p).into_owned()).collect();
    for local in ignored.stdout.split(|b| *b == 0).filter(|p| !p.is_empty()).map(|p| String::from_utf8_lossy(p).into_owned()) {
        if incoming.iter().any(|path| is_scoped(path, &local) || is_scoped(&local, path)) {
            return paused(format!("無視されたローカルファイルと更新が衝突します: {local}。ファイルを保持して同期を停止しました。"), records);
        }
    }
    let files = diff_records(repo, &before, &after)?;
    let id = format!("{before}:{after}");
    let new_record = !files.is_empty() && !records.iter().any(|r| r.id == id && r.applied);
    if new_record {
        records.retain(|r| r.id != id);
        records.push(UpdateRecord { id: id.clone(), before: before.clone(), after: after.clone(), timestamp: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|_| "時刻を取得できません")?.as_secs(), read: false, applied: false, files });
        save_records(journal, &records)?;
    }
    let merge = git(&repo.git_root, &["-c", "core.hooksPath=NUL", "-c", "submodule.recurse=false", "merge", "--ff-only", "--no-overwrite-ignore", "--no-edit", &after])?;
    if !merge.status.success() { return paused(format!("早送り更新に失敗しました。変更は破棄していません。\n{}", safe_git_output(&merge)), records); }
    if git_text(&repo.git_root, &["rev-parse", "HEAD"])? != after { return Err("同期後のコミットを確認できません。状態を再読み込みしてください".into()); }
    if let Some(record) = records.iter_mut().find(|r| r.id == id) { record.applied = true; }
    save_records(journal, &records)?;
    Ok(SyncResult { message: "GitHub から安全に早送り更新しました。".into(), records, new_id: if new_record { Some(id) } else { recovered } })
}

#[tauri::command]
pub async fn preview_workspace(path: String) -> Result<WorkspacePreview, String> {
    tauri::async_runtime::spawn_blocking(move || { let _ = path; inspect(Path::new(DESIGN_ROOT)) }).await.map_err(|_| "Worktree 検査タスクに失敗しました")?
}
#[tauri::command]
pub async fn create_workspace(path: String, confirmed: bool) -> Result<WorkspacePreview, String> {
    tauri::async_runtime::spawn_blocking(move || { let _ = (path, confirmed); Err("独立リポジトリは移行手順で準備してください".into()) }).await.map_err(|_| "Worktree 作成タスクに失敗しました")?
}
#[tauri::command]
pub async fn sync_workspace(app: tauri::AppHandle, unity_root: String, unsaved: bool) -> Result<SyncResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<RepoState>();
        let mut guard = state.0.lock().map_err(|_| "プロジェクト状態を取得できません")?;
        let repo = guard.as_ref().ok_or("プロジェクトが開かれていません")?;
        if repo.source_root.as_ref().map(|p| p.to_string_lossy().to_string()).as_deref() != Some(&unity_root) { return Err("プロジェクトが切り替わりました".into()); }
        let _operation = operation_guard()?;
        let result = sync(repo, &record_path(&app, repo)?, unsaved);
        let source = repo.source_root.clone().ok_or("Unity プロジェクトがありません")?;
        if let Ok(updated) = open(&source) { *guard = Some(updated); }
        result
    }).await.map_err(|_| "同期タスクに失敗しました")?
}
#[tauri::command]
pub fn update_records(app: tauri::AppHandle, state: State<'_, RepoState>) -> Result<Vec<UpdateRecord>, String> {
    let repo = repository(&state)?;
    load_records(&record_path(&app, &repo)?)
}
#[tauri::command]
pub fn mark_updates_read(app: tauri::AppHandle, id: Option<String>, state: State<'_, RepoState>) -> Result<Vec<UpdateRecord>, String> {
    let repo = repository(&state)?;
    let _journal_guard = JOURNAL_LOCK.lock().map_err(|_| "更新記録をロックできません")?;
    let path = record_path(&app, &repo)?;
    let mut records = load_records(&path)?;
    for record in &mut records { if id.as_ref().is_none_or(|id| *id == record.id) { record.read = true; } }
    save_records(&path, &records)?;
    Ok(records)
}
#[tauri::command]
pub fn locate_update(record_id: String, path: String, line: usize, app: tauri::AppHandle, state: State<'_, RepoState>) -> Result<String, String> {
    let repo = repository(&state)?;
    let records = load_records(&record_path(&app, &repo)?)?;
    locate(&repo, &records, &record_id, &path, line)
}

fn locate(repo: &Repository, records: &[UpdateRecord], record_id: &str, path: &str, line: usize) -> Result<String, String> {
    let record = records.iter().find(|r| r.id == record_id && r.applied).ok_or("更新記録がありません")?;
    let file = record.files.iter().find(|f| f.path == path).ok_or("更新ファイルがありません")?;
    if file.status.starts_with('D') || !file.hunks.iter().any(|h| h.line == line && !h.removed) { return Err("削除または位置を特定できない変更です。差分を確認してください。".into()); }
    let source = safe_existing(repo, &path.strip_prefix(&format!("{}/", repo.git_documents_relative)).map(|p| format!("DesignDocs/{p}")).ok_or("設計書の範囲外です")?)?;
    let original = git(&repo.git_root, &["show", &format!("{}:{}", record.after, path)])?;
    if !original.status.success() { return Err("更新後の内容を取得できません".into()); }
    if fs::read_to_string(&source).map_err(|_| "ファイルを読み取れません")?.replace("\r\n", "\n") != String::from_utf8_lossy(&original.stdout).replace("\r\n", "\n") { return Err("文書が更新記録から変更されています。誤った位置に移動せず差分を表示します。".into()); }
    Ok(fs::read_to_string(source).map_err(|_| "文書を読み取れません")?)
}
#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture { base: PathBuf, source: PathBuf, writer: PathBuf }
    impl Drop for Fixture { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.base); } }
    fn run(root: &Path, args: &[&str]) { let output = git(root, args).unwrap(); assert!(output.status.success(), "{args:?}: {}", safe_git_output(&output)); }
    fn fixture() -> Fixture {
        let base = std::env::temp_dir().join(format!("designbook-vnext-{}-{}", std::process::id(), TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)));
        fs::create_dir_all(&base).unwrap();
        let source = base.join("Unity game"); let writer = base.join("writer"); let remote = base.join("remote.git");
        fs::create_dir_all(&source).unwrap();
        run(&base, &["init", "--bare", "--initial-branch=main", remote.to_str().unwrap()]);
        run(&source, &["init", "--initial-branch=main"]);
        run(&source, &["config", "user.name", "Isolated Test"]); run(&source, &["config", "user.email", "test@example.invalid"]); run(&source, &["config", "core.autocrlf", "false"]);
        fs::create_dir_all(source.join("Assets")).unwrap(); fs::write(source.join("Assets/keep.txt"), "game\n").unwrap();
        fs::create_dir_all(source.join("ProjectSettings")).unwrap(); fs::write(source.join("ProjectSettings/ProjectVersion.txt"), "m_EditorVersion: 6000.3\n").unwrap();
        fs::create_dir_all(source.join("DesignDocs")).unwrap(); fs::write(source.join("DesignDocs/page.md"), "# Battle\n\n## Attack\nold\n").unwrap();
        fs::write(source.join("DesignDocs/deleted.md"), "delete me\n").unwrap();
        fs::write(source.join("DesignDocs/rename.md"), "rename me\n").unwrap();
        fs::write(source.join("DesignDocs/data.json"), "{\"value\":1}\n").unwrap();
        run(&source, &["add", "."]); run(&source, &["commit", "-m", "initial"]);
        run(&source, &["remote", "add", "origin", remote.to_str().unwrap()]); run(&source, &["push", "-u", "origin", "main"]);
        run(&base, &["-c", "core.autocrlf=false", "clone", remote.to_str().unwrap(), writer.to_str().unwrap()]);
        run(&writer, &["config", "user.name", "Remote Writer"]); run(&writer, &["config", "user.email", "writer@example.invalid"]); run(&writer, &["config", "core.autocrlf", "false"]);
        run(&source, &["switch", "-c", "feature/player"]);
        Fixture { base, source, writer }
    }
    fn managed(f: &Fixture) -> Repository { create(&f.source, true).unwrap(); open(&f.source).unwrap() }
    fn publish(f: &Fixture) {
        run(&f.writer, &["add", "-A"]); run(&f.writer, &["commit", "-m", "remote docs update"]); run(&f.writer, &["push", "origin", "main"]);
    }
    #[test]
    fn independent_identity_accepts_main_and_rejects_shared_worktrees_wrong_remote_and_branch() {
        // 隔離した Git だけを使って、誤送信とゲーム側の混入を検証する。
        let f = fixture();
        run(&f.source, &["switch", "main"]);
        run(&f.source, &["config", "designbook.repository", "XiuxianGame_DesignDocs"]);
        assert!(standalone(&f.source).is_err());
        run(&f.source, &["remote", "remove", "origin"]);
        standalone(&f.source).unwrap();
        run(&f.source, &["remote", "add", "origin", "https://github.com/Ron0609/XiuxianGame_DesignDocs.git"]);
        standalone(&f.source).unwrap();
        run(&f.source, &["config", "remote.origin.pushurl", "https://github.com/Ron0609/XiuxianGame_Unity.git"]);
        assert!(standalone(&f.source).is_err());
        run(&f.source, &["config", "--unset", "remote.origin.pushurl"]);
        run(&f.source, &["switch", "feature/player"]);
        assert!(standalone(&f.source).is_err());
    }
    #[test]
    fn worktree_create_reuse_and_cancel_preserve_development_files_index_and_branch() {
        let f = fixture();
        fs::write(f.source.join("Assets/keep.txt"), "dirty game\n").unwrap();
        fs::write(f.source.join("Assets/staged.txt"), "staged game\n").unwrap(); run(&f.source, &["add", "Assets/staged.txt"]);
        let before = git(&f.source, &["status", "--porcelain=v1", "-z"]).unwrap().stdout;
        assert!(create(&f.source, false).is_err()); assert!(!target_path(&f.source).unwrap().exists());
        let repo = managed(&f); assert_eq!(branch(&repo.git_root).unwrap(), "main");
        assert_eq!(branch(&f.source).unwrap(), "feature/player"); assert_ne!(repo.git_root, f.source.canonicalize().unwrap());
        assert_eq!(repo.documents, repo.git_root.join("DesignDocs"));
        assert_eq!(open(&f.source).unwrap().git_root, repo.git_root);
        assert!(create(&f.source, true).unwrap().existing);
        assert_eq!(git(&f.source, &["status", "--porcelain=v1", "-z"]).unwrap().stdout, before);
        fs::write(repo.documents.join("page.md"), "# isolated document\n").unwrap();
        let message = git_commit_impl(&repo, "docs only").unwrap(); assert!(message.contains("Commit 成功"));
        assert_eq!(fs::read_to_string(f.source.join("DesignDocs/page.md")).unwrap(), "# Battle\n\n## Attack\nold\n");
        assert_eq!(git(&f.source, &["status", "--porcelain=v1", "-z"]).unwrap().stdout, before);
        assert!(git_text(&repo.git_root, &["show", "--pretty=", "--name-only", "HEAD"]).unwrap().lines().all(|p| p.starts_with("DesignDocs/")));
    }
    #[test]
    fn main_occupied_or_original_docs_dirty_blocks_creation_without_changes() {
        let f = fixture(); run(&f.source, &["switch", "main"]);
        assert!(inspect(&f.source).unwrap().blockers.iter().any(|m| m.contains("使用中")));
        assert!(create(&f.source, true).is_err());
        run(&f.source, &["switch", "feature/player"]);
        fs::write(f.source.join("DesignDocs/page.md"), "uncommitted original\n").unwrap();
        assert!(create(&f.source, true).is_err()); assert!(!target_path(&f.source).unwrap().exists());
        assert_eq!(fs::read_to_string(f.source.join("DesignDocs/page.md")).unwrap(), "uncommitted original\n");
    }
    #[test]
    fn unpushed_design_commit_and_path_collision_stop_migration() {
        let f = fixture(); fs::write(f.source.join("DesignDocs/page.md"), "source-only commit\n").unwrap();
        run(&f.source, &["add", "DesignDocs/page.md"]); run(&f.source, &["commit", "-m", "not on main"]);
        assert!(!inspect(&f.source).unwrap().unpushed_documents.is_empty()); assert!(create(&f.source, true).is_err());
        fs::create_dir_all(target_path(&f.source).unwrap()).unwrap();
        assert!(create(&f.source, true).is_err()); assert!(!target_path(&f.source).unwrap().join(".git").exists());
    }
    #[test]
    fn sync_detects_add_modify_delete_rename_json_and_persists_read_state_without_duplicates() {
        let f = fixture(); let repo = managed(&f); let journal = f.base.join("updates.json");
        let source_head = git_text(&f.source, &["rev-parse", "HEAD"]).unwrap();
        assert!(sync(&repo, &journal, false).unwrap().records.is_empty());
        fs::write(f.writer.join("DesignDocs/page.md"), "# Battle revised\n\n## Attack check\nnew one\nnew two\n").unwrap();
        fs::write(f.writer.join("DesignDocs/new.md"), "# NPC\ninteract\n").unwrap();
        fs::write(f.writer.join("DesignDocs/data.json"), "{\"value\":2}\n").unwrap();
        fs::remove_file(f.writer.join("DesignDocs/deleted.md")).unwrap();
        run(&f.writer, &["mv", "DesignDocs/rename.md", "DesignDocs/renamed.md"]); publish(&f);
        let result = sync(&repo, &journal, false).unwrap(); assert!(result.new_id.is_some());
        let record = &result.records[0]; assert!(record.applied); assert!(!record.read); assert_ne!(record.before, record.after);
        for status in ["A", "M", "D", "R"] { assert!(record.files.iter().any(|f| f.status.starts_with(status)), "missing {status}"); }
        assert!(record.files.iter().any(|f| f.path.ends_with("data.json") && !f.hunks.is_empty()));
        let page = record.files.iter().find(|f| f.path.ends_with("page.md")).unwrap(); assert!(!page.hunks[0].removed); assert!(page.hunks[0].line > 0);
        assert_eq!(git_text(&f.source, &["rev-parse", "HEAD"]).unwrap(), source_head);
        assert_eq!(fs::read_to_string(f.source.join("DesignDocs/page.md")).unwrap(), "# Battle\n\n## Attack\nold\n");
        let again = sync(&open(&f.source).unwrap(), &journal, false).unwrap(); assert!(again.new_id.is_none()); assert_eq!(again.records.len(), 1);
        let mut records = load_records(&journal).unwrap(); records[0].read = true; save_records(&journal, &records).unwrap(); assert!(load_records(&journal).unwrap()[0].read);
    }
    #[test]
    fn dirty_worktree_and_unsaved_editor_pause_without_fetch_or_overwrite() {
        let f = fixture(); let repo = managed(&f); let journal = f.base.join("updates.json");
        let head = git_text(&repo.git_root, &["rev-parse", "HEAD"]).unwrap();
        fs::write(f.writer.join("DesignDocs/page.md"), "remote\n").unwrap(); publish(&f);
        assert!(sync(&repo, &journal, true).unwrap().message.contains("未保存"));
        fs::write(repo.documents.join("page.md"), "local dirty\n").unwrap();
        assert!(sync(&repo, &journal, false).unwrap().message.contains("未コミット"));
        assert_eq!(git_text(&repo.git_root, &["rev-parse", "HEAD"]).unwrap(), head);
        assert_eq!(fs::read_to_string(repo.documents.join("page.md")).unwrap(), "local dirty\n");
        assert_eq!(git_text(&repo.git_root, &["rev-parse", "origin/main"]).unwrap(), head);
    }
    #[test]
    fn diverged_history_stops_and_failed_remote_keeps_local_readable_then_recovers() {
        let f = fixture(); let repo = managed(&f); let journal = f.base.join("updates.json");
        run(&repo.git_root, &["remote", "set-url", "origin", f.base.join("missing.git").to_str().unwrap()]);
        let failed = sync(&repo, &journal, false).unwrap(); assert!(failed.message.contains("オフライン")); assert!(open(&f.source).is_ok());
        run(&repo.git_root, &["remote", "set-url", "origin", f.base.join("remote.git").to_str().unwrap()]);
        assert!(sync(&repo, &journal, false).unwrap().message.contains("最新"));
        fs::write(repo.documents.join("page.md"), "local\n").unwrap(); git_commit_impl(&repo, "local").unwrap();
        fs::write(f.writer.join("DesignDocs/page.md"), "remote\n").unwrap(); publish(&f);
        let before = git_text(&repo.git_root, &["rev-parse", "HEAD"]).unwrap();
        assert!(sync(&repo, &journal, false).unwrap().message.contains("分岐"));
        assert_eq!(git_text(&repo.git_root, &["rev-parse", "HEAD"]).unwrap(), before);
    }
    #[test]
    fn journal_recovers_completed_fast_forward_and_rejects_corrupt_records() {
        let f = fixture(); let repo = managed(&f); let journal = f.base.join("updates.json");
        let before = git_text(&repo.git_root, &["rev-parse", "HEAD"]).unwrap();
        fs::write(f.writer.join("DesignDocs/page.md"), "remote\n").unwrap(); publish(&f);
        run(&repo.git_root, &["fetch", "origin"]); let after = git_text(&repo.git_root, &["rev-parse", "origin/main"]).unwrap();
        let files = diff_records(&repo, &before, &after).unwrap();
        save_records(&journal, &[UpdateRecord { id: "recover".into(), before, after: after.clone(), timestamp: 1, read: false, applied: false, files }]).unwrap();
        run(&repo.git_root, &["merge", "--ff-only", &after]);
        let recovered = sync(&repo, &journal, false).unwrap(); assert_eq!(recovered.new_id.as_deref(), Some("recover")); assert!(recovered.records[0].applied);
        fs::write(&journal, "corrupt").unwrap(); assert!(sync(&repo, &journal, false).is_err()); assert_eq!(fs::read_to_string(&journal).unwrap(), "corrupt");
    }
    #[test]
    fn removed_hunks_never_point_to_existing_context_and_headings_use_new_content() {
        let removed = parse_hunks("@@ -1,3 +1,2 @@\n # X\n-deleted\n context\n", "# X\ncontext\n", false);
        assert!(removed[0].removed); assert_eq!(removed[0].line, 0);
        let added = parse_hunks("@@ -1,2 +1,4 @@\n # X\n+## New section\n+added\n context\n", "# X\n## New section\nadded\ncontext\n", false);
        assert_eq!(added[0].line, 2); assert_eq!(added[0].heading, "New section");
    }
    #[test]
    fn navigation_rejects_stale_files_deleted_content_and_out_of_scope_paths() {
        let f = fixture(); let repo = managed(&f); let journal = f.base.join("updates.json");
        fs::write(f.writer.join("DesignDocs/page.md"), "# Updated section\nnew\n").unwrap();
        fs::remove_file(f.writer.join("DesignDocs/deleted.md")).unwrap(); publish(&f);
        let result = sync(&repo, &journal, false).unwrap(); let record = &result.records[0];
        let file = record.files.iter().find(|f| f.path.ends_with("page.md")).unwrap();
        let line = file.hunks[0].line;
        assert!(locate(&repo, &result.records, &record.id, &file.path, line).unwrap().contains("Updated section"));
        assert!(locate(&repo, &result.records, &record.id, "DesignDocs/deleted.md", 0).is_err());
        assert!(locate(&repo, &result.records, &record.id, "Assets/keep.txt", 1).is_err());
        fs::write(repo.documents.join("page.md"), "later local change\n").unwrap();
        assert!(locate(&repo, &result.records, &record.id, &file.path, line).is_err());
        assert_eq!(fs::read_to_string(repo.documents.join("page.md")).unwrap(), "later local change\n");
    }
    #[test]
    fn existing_dirty_worktree_is_reused_without_overwriting_and_wrong_branch_is_rejected() {
        let f = fixture(); let repo = managed(&f);
        fs::write(repo.documents.join("page.md"), "keep me\n").unwrap();
        assert_eq!(inspect(&f.source).unwrap().worktree_changes.len(), 1);
        assert!(open(&f.source).is_ok());
        run(&repo.git_root, &["switch", "-c", "wrong-branch"]);
        assert!(open(&f.source).is_err());
        assert_eq!(fs::read_to_string(repo.documents.join("page.md")).unwrap(), "keep me\n");
    }
    #[test]
    fn fast_forward_without_document_changes_does_not_notify() {
        let f = fixture(); let repo = managed(&f); let journal = f.base.join("updates.json");
        fs::write(f.writer.join("Assets/keep.txt"), "new game content\n").unwrap(); publish(&f);
        let result = sync(&repo, &journal, false).unwrap();
        assert!(result.new_id.is_none()); assert!(result.records.is_empty());
        assert_eq!(fs::read_to_string(f.source.join("Assets/keep.txt")).unwrap(), "game\n");
    }

    #[test]
    fn nested_unity_project_uses_sibling_of_git_root_not_a_directory_inside_development_tree() {
        let f = fixture(); run(&f.source, &["switch", "main"]);
        fs::create_dir(f.source.join("SubUnity")).unwrap();
        for path in ["Assets", "ProjectSettings", "DesignDocs"] { run(&f.source, &["mv", path, &format!("SubUnity/{path}")]); }
        run(&f.source, &["commit", "-m", "nested project"]); run(&f.source, &["push", "origin", "main"]);
        run(&f.source, &["switch", "feature/player"]); run(&f.source, &["merge", "--ff-only", "main"]);
        let unity = f.source.join("SubUnity");
        let before = git(&f.source, &["status", "--porcelain=v1", "-z"]).unwrap().stdout;
        let preview = create(&unity, true).unwrap(); let repo = open(&unity).unwrap();
        assert!(!Path::new(&preview.worktree).starts_with(f.source.canonicalize().unwrap()));
        assert_eq!(repo.git_documents_relative, "SubUnity/DesignDocs");
        assert_eq!(repo.root, repo.git_root.join("SubUnity"));
        assert_eq!(git(&f.source, &["status", "--porcelain=v1", "-z"]).unwrap().stdout, before);
        assert!(safe_existing(&repo, "DesignDocs/page.md").is_ok());
    }

    #[test]
    fn sparse_checkout_only_materializes_docs_and_preserves_development_index_and_config() {
        let f = fixture();
        let before = source_sparse_state(&f.source).unwrap();
        let repo = managed(&f);
        let mut entries: Vec<_> = fs::read_dir(&repo.git_root).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        entries.sort(); assert_eq!(entries, vec![".git", "DesignDocs"]);
        assert!(!repo.git_root.join("Assets").exists()); assert!(!repo.git_root.join("ProjectSettings").exists());
        assert_eq!(fs::read_to_string(repo.documents.join("page.md")).unwrap(), "# Battle\n\n## Attack\nold\n");
        assert_eq!(source_sparse_state(&f.source).unwrap(), before);
        assert!(git_status_impl(&repo.git_root).unwrap().clean);
        let entries = git_text(&repo.git_root, &["ls-files", "-t"]).unwrap();
        assert!(entries.lines().any(|line| line == "S Assets/keep.txt"));
        assert!(entries.lines().any(|line| line == "H DesignDocs/page.md"));
        assert_eq!(git_text(&repo.git_root, &["sparse-checkout", "list"]).unwrap(), "/DesignDocs/");
        assert!(inspect(&f.source).unwrap().sparse_ready);
    }
    #[test]
    fn full_worktree_requires_confirmation_then_converts_without_touching_source() {
        let f = fixture(); let target = target_path(&f.source).unwrap();
        run(&f.source, &["worktree", "add", &git_path(&target), "main"]);
        assert!(target.join("Assets/keep.txt").exists());
        let preview = inspect(&f.source).unwrap(); assert!(preview.existing); assert!(!preview.sparse_ready); assert!(preview.blockers.is_empty());
        assert!(open(&f.source).is_err()); assert!(create(&f.source, false).is_err()); assert!(target.join("Assets/keep.txt").exists());
        create(&f.source, true).unwrap(); let repo = open(&f.source).unwrap();
        assert!(!repo.git_root.join("Assets").exists()); assert!(repo.documents.join("page.md").exists());
        assert_eq!(fs::read_to_string(f.source.join("Assets/keep.txt")).unwrap(), "game\n");
        assert_eq!(branch(&f.source).unwrap(), "feature/player");
    }
    #[test]
    fn conversion_refuses_dirty_or_ignored_files_and_never_removes_them() {
        let f = fixture(); let target = target_path(&f.source).unwrap();
        run(&f.source, &["worktree", "add", &git_path(&target), "main"]);
        let info = git_text(&target, &["rev-parse", "--git-path", "info/exclude"]).unwrap();
        let exclude = target.join(info); fs::write(&exclude, "Assets/ignored.cache\n").unwrap();
        fs::write(target.join("Assets/ignored.cache"), "valuable ignored data\n").unwrap();
        assert!(git_status_impl(&target).unwrap().clean);
        assert!(create(&f.source, true).is_err());
        assert_eq!(fs::read_to_string(target.join("Assets/ignored.cache")).unwrap(), "valuable ignored data\n");
        assert!(target.join("Assets/keep.txt").exists());
    }
    #[test]
    fn sparse_commit_and_first_push_preserve_hidden_files_and_restart_status() {
        let f = fixture(); let repo = managed(&f);
        fs::write(repo.documents.join("page.md"), "# committed in sparse\n").unwrap();
        let message = git_commit_impl(&repo, "sparse docs only").unwrap(); assert!(message.contains("Commit 成功"));
        let files = git_text(&repo.git_root, &["diff-tree", "--no-commit-id", "--name-only", "-r", "HEAD"]).unwrap();
        assert_eq!(files, "DesignDocs/page.md");
        assert!(git_text(&repo.git_root, &["ls-tree", "-r", "HEAD", "--", "Assets", "ProjectSettings"]).unwrap().contains("Assets/keep.txt"));
        let reopened = open(&f.source).unwrap(); assert!(git_status_impl(&reopened.git_root).unwrap().clean);
        assert!(git_text(&repo.git_root, &["ls-files", "-t", "--", "Assets/keep.txt"]).unwrap().starts_with("S "));
        run(&repo.git_root, &["branch", "--unset-upstream"]);
        assert!(git_push_preview_impl(&repo.git_root, "origin", "main").unwrap().ready);
        git_push_target_impl(&repo.git_root, "origin", "main", true).unwrap();
        assert_eq!(git_text(&repo.git_root, &["rev-parse", "--abbrev-ref", "@{upstream}"]).unwrap(), "origin/main");
        assert_eq!(branch(&f.source).unwrap(), "feature/player");
        assert_eq!(fs::read_to_string(f.source.join("DesignDocs/page.md")).unwrap(), "# Battle\n\n## Attack\nold\n");
    }
    #[test]
    fn staged_out_of_scope_deletion_is_rejected_before_staging_docs() {
        let f = fixture(); let repo = managed(&f);
        run(&repo.git_root, &["rm", "--cached", "--sparse", "--", "Assets/keep.txt"]);
        let index = git(&repo.git_root, &["diff", "--cached", "--raw", "-z"]).unwrap().stdout;
        let head = git_text(&repo.git_root, &["rev-parse", "HEAD"]).unwrap();
        fs::write(repo.documents.join("page.md"), "new docs\n").unwrap();
        assert!(git_commit_impl(&repo, "must stop").unwrap_err().contains("範囲外"));
        assert_eq!(git(&repo.git_root, &["diff", "--cached", "--raw", "-z"]).unwrap().stdout, index);
        assert_eq!(git_text(&repo.git_root, &["rev-parse", "HEAD"]).unwrap(), head);
    }

    #[test]
    fn sparse_sync_never_materializes_game_files_and_protects_ignored_document_collisions() {
        let f = fixture(); let repo = managed(&f); let journal = f.base.join("updates.json");
        fs::write(f.writer.join("Assets/new-code.cs"), "class Added {}\n").unwrap(); publish(&f);
        let result = sync(&repo, &journal, false).unwrap(); assert!(result.new_id.is_none());
        assert!(!repo.git_root.join("Assets").exists());
        let exclude = repo.git_root.join(git_text(&repo.git_root, &["rev-parse", "--git-path", "info/exclude"]).unwrap());
        fs::write(&exclude, "DesignDocs/local-secret.md\n").unwrap();
        fs::write(repo.documents.join("local-secret.md"), "ignored local content\n").unwrap();
        fs::write(f.writer.join("DesignDocs/local-secret.md"), "remote tracked document\n").unwrap();
        publish(&f);
        let head = git_text(&repo.git_root, &["rev-parse", "HEAD"]).unwrap();
        let paused = sync(&repo, &journal, false).unwrap(); assert!(paused.message.contains("失敗") || paused.message.contains("衝突"), "{}", paused.message);
        assert_eq!(git_text(&repo.git_root, &["rev-parse", "HEAD"]).unwrap(), head);
        assert_eq!(fs::read_to_string(repo.documents.join("local-secret.md")).unwrap(), "ignored local content\n");
    }

}
