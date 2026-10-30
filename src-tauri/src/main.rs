#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod workspace;

use serde::Serialize;
use serde_json::Value;
use jsonschema::validator_for;
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::process::{Command, Output};
use std::sync::Mutex;
use tauri::State;

#[derive(Clone)]
struct Repository { source_root: Option<PathBuf>, root: PathBuf, git_root: PathBuf, documents: PathBuf, documents_relative: String, git_documents_relative: String, has_documents: bool }
struct RepoState(Mutex<Option<Repository>>);

#[derive(Serialize)]
struct DocumentFile {
    path: String,
    kind: String,
}

#[derive(Serialize)]
struct DocumentTree {
    files: Vec<DocumentFile>,
    directories: Vec<String>,
}

#[derive(Serialize)]
struct GitState {
    branch: String,
    changed: Vec<String>,
    changes: Vec<GitChange>,
    document_changed: Vec<String>,
    other_changed: Vec<String>,
    ahead: usize,
    behind: usize,
    clean: bool,
    has_upstream: bool,
    tracking_error: Option<String>,
}

#[derive(Serialize)]
struct GitChange { path: String, previous_path: Option<String>, status: String }

fn parse_git_changes(raw: &[u8]) -> Result<Vec<GitChange>, String> {
    let mut records = raw.split(|byte| *byte == 0).filter(|record| !record.is_empty());
    let mut changes = Vec::new();
    while let Some(record) = records.next() {
        if record.len() < 4 || record[2] != b' ' { return Err("Git status の形式が不正です".into()); }
        let status = String::from_utf8_lossy(&record[..2]).into_owned();
        let path = String::from_utf8_lossy(&record[3..]).into_owned();
        let previous_path = if status.contains('R') || status.contains('C') {
            Some(String::from_utf8_lossy(records.next().ok_or("Git rename の元パスがありません")?).into_owned())
        } else { None };
        changes.push(GitChange { path, previous_path, status });
    }
    Ok(changes)
}

fn scope_git_status(mut status: GitState, scope: &str) -> GitState {
    let in_scope = |path: &str| path == scope || path.strip_prefix(scope).is_some_and(|rest| rest.starts_with('/'));
    for change in &status.changes {
        if in_scope(&change.path) || change.previous_path.as_deref().is_some_and(&in_scope) {
            status.document_changed.push(change.path.clone());
        } else { status.other_changed.push(change.path.clone()); }
    }
    status
}

#[derive(Serialize, Clone)]
struct GitBranch { name: String, current: bool, remote: bool, upstream: Option<String>, ahead: usize, behind: usize }

#[derive(Serialize, Clone)]
struct GitRemote { name: String, url: String }

#[derive(Serialize)]
struct GitPushPreview { current_branch: String, remote: String, remote_branch: String, changed_count: usize, ahead: Option<usize>, behind: Option<usize>, target_exists: bool, ready: bool, message: String }

#[derive(Serialize)]
struct SearchMatch {
    path: String,
    kind: String,
    excerpt: String,
    line: usize,
    column: usize,
    length: usize,
    excerpt_offset: usize,
    query: String,
}

#[derive(Serialize)]
struct SearchResults { matches: Vec<SearchMatch>, truncated: bool, skipped: Vec<String> }

#[derive(Serialize)]
struct ImportedImage { path: String }

#[derive(Serialize, Clone)]
struct ProjectInfo { unity_root: String, git_root: String, documents_path: Option<String>, has_documents: bool, worktree_root: Option<String>, development_branch: Option<String> }

#[derive(Serialize, Clone, PartialEq, Eq)]
struct SetupEntry { path: String, kind: String }

#[derive(Serialize)]
struct SetupPreview { entries: Vec<SetupEntry>, changed_files: Vec<String> }

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

static OPERATION_LOCK: Mutex<()> = Mutex::new(());
fn operation_guard() -> Result<std::sync::MutexGuard<'static, ()>, String> {
    OPERATION_LOCK.try_lock().map_err(|_| "保存または Git 操作を実行中です。完了後に再試行してください".into())
}

fn repository(state: &State<'_, RepoState>) -> Result<Repository, String> {
    let repo = state.0.try_lock().map_err(|_| "同期処理中です。完了後に再試行してください".to_string())?.clone()
        .ok_or_else(|| "先に Git リポジトリを開いてください".to_string())?;
    if let Some(source) = &repo.source_root { workspace::validate_target(source, &repo.git_root)?; }
    Ok(repo)
}

fn repo_root(state: &State<'_, RepoState>) -> Result<PathBuf, String> { Ok(repository(state)?.git_root) }

fn git_command(root: &Path, args: &[&str]) -> Command {
    let mut command = Command::new("git");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // Tauri is a GUI process; prevent every Git CLI invocation (including polling) from opening a console.
        command.creation_flags(0x08000000);
    }
    command.args(args).current_dir(root).env("GIT_TERMINAL_PROMPT", "0").env("GCM_INTERACTIVE", "Never").env("GIT_OPTIONAL_LOCKS", "0");
    command
}

fn git(root: &Path, args: &[&str]) -> Result<Output, String> {
    git_command(root, args).output()
        .map_err(|_| "Git を起動できません。Git for Windows のインストールを確認してください。".to_string())
}

fn git_text(root: &Path, args: &[&str]) -> Result<String, String> {
    let output = git(root, args)?;
    if !output.status.success() {
        return Err("Git 操作に失敗しました。リポジトリの状態と Git 設定を確認してください。".into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn safe_git_output(output: &Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let text = match (stdout.trim().is_empty(), stderr.trim().is_empty()) {
        (true, true) => String::new(),
        (false, true) => stdout.trim().to_string(),
        (true, false) => stderr.trim().to_string(),
        (false, false) => format!("{}\n{}", stdout.trim(), stderr.trim()),
    };
    redact_url_credentials(&text)
}

fn redact_url_credentials(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(scheme) = rest.find("://") {
        let after_scheme = scheme + 3;
        result.push_str(&rest[..after_scheme]);
        rest = &rest[after_scheme..];
        let authority_end = rest.find(['/', '?', '#', '\r', '\n', ' ']).unwrap_or(rest.len());
        let authority = &rest[..authority_end];
        if let Some(at) = authority.rfind('@') { result.push_str("***@"); result.push_str(&authority[at + 1..]); }
        else { result.push_str(authority); }
        rest = &rest[authority_end..];
    }
    result.push_str(rest);
    redact_query_credentials(&result)
}

fn redact_query_credentials(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(query) = rest.find('?') {
        let query_start = query + 1;
        result.push_str(&rest[..query_start]);
        rest = &rest[query_start..];
        let query_end = rest.find(|character: char| matches!(character, '#' | ' ' | '\r' | '\n' | '\t')).unwrap_or(rest.len());
        let query_text = &rest[..query_end];
        for (index, parameter) in query_text.split('&').enumerate() {
            if index > 0 { result.push('&'); }
            if let Some((key, _value)) = parameter.split_once('=') {
                if key.is_empty() { result.push_str("***"); } else { result.push_str(key); result.push_str("=***"); }
            } else if !parameter.is_empty() { result.push_str("***"); }
        }
        rest = &rest[query_end..];
    }
    result.push_str(rest);
    result
}

fn git_command_result(root: &Path, args: &[&str], action: &str) -> Result<String, String> {
    let output = git(root, args)?;
    if output.status.success() {
        let result = safe_git_output(&output);
        Ok(if result.is_empty() { format!("{action}が完了しました") } else { result })
    }
    else {
        let detail = safe_git_output(&output);
        Err(if detail.is_empty() { format!("{action}に失敗しました (終了コード {})", output.status.code().map_or_else(|| "不明".into(), |code| code.to_string())) } else { detail })
    }
}

fn safe_relative(path: &str) -> Result<PathBuf, String> {
    let relative = Path::new(path);
    if relative.as_os_str().is_empty() || relative.is_absolute() || relative.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err("無効なファイルパスです".into());
    }
    Ok(relative.to_path_buf())
}

fn safe_existing(repo: &Repository, path: &str) -> Result<PathBuf, String> {
    if !repo.has_documents { return Err("設計書ディレクトリはまだ作成されていません".into()); }
    let relative = safe_document_relative(repo, path)?;
    let full = repo.root.join(relative).canonicalize().map_err(|_| "ファイルを見つけられません".to_string())?;
    if !full.starts_with(&repo.documents) || !full.is_file() { return Err("選択した設計書ディレクトリ外のファイルにはアクセスできません".into()); }
    let ext = full.extension().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
    if !["md", "markdown", "mmd", "json"].contains(&ext.as_str()) { return Err("Markdown または JSON ファイルのみ操作できます".into()); }
    Ok(full)
}

fn scan_document_tree(repo: &Repository) -> Result<DocumentTree, String> {
    if !repo.has_documents { return Ok(DocumentTree { files: Vec::new(), directories: Vec::new() }); }
    let root = repo.documents.canonicalize().map_err(|_| "設計書ディレクトリを読み取れません".to_string())?;
    if !root.starts_with(&repo.root) || !root.is_dir() { return Err("設計書ディレクトリが許可範囲外です".into()); }

    let mut files = Vec::new();
    let mut directories = Vec::new();
    let mut pending = vec![root.clone()];
    while let Some(directory) = pending.pop() {
        let canonical_directory = directory.canonicalize().map_err(|_| format!("設計書フォルダーを読み取れません: {}", directory.display()))?;
        if !canonical_directory.starts_with(&root) { return Err("設計書フォルダーが許可範囲外を指しています".into()); }
        let relative_directory = canonical_directory.strip_prefix(&root).map_err(|_| "設計書フォルダーの相対パスを確認できません".to_string())?;
        let exposed_directory = Path::new(&repo.documents_relative).join(relative_directory);
        directories.push(exposed_directory.to_string_lossy().replace('\\', "/"));

        let entries = fs::read_dir(&canonical_directory).map_err(|_| format!("設計書フォルダーの一覧を読み取れません: {}", canonical_directory.display()))?;
        for entry in entries {
            let entry = entry.map_err(|_| format!("設計書フォルダー内の項目を読み取れません: {}", canonical_directory.display()))?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path).map_err(|_| format!("設計書項目の情報を読み取れません: {}", path.display()))?;
            let file_type = metadata.file_type();
            if file_type.is_symlink() { continue; }
            let name = entry.file_name().to_string_lossy().to_string();
            if file_type.is_dir() {
                // Never descend into a nested Git metadata directory.
                if name.eq_ignore_ascii_case(".git") { continue; }
                pending.push(path);
                continue;
            }
            if !file_type.is_file() { continue; }
            let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
            let kind = match ext.as_str() { "md" | "markdown" | "mmd" => "markdown", "json" => "json", _ => continue };
            let canonical_file = path.canonicalize().map_err(|_| format!("設計書ファイルを確認できません: {}", path.display()))?;
            if !canonical_file.starts_with(&root) || !canonical_file.is_file() { continue; }
            let relative_file = canonical_file.strip_prefix(&root).map_err(|_| "設計書ファイルの相対パスを確認できません".to_string())?;
            let exposed_file = Path::new(&repo.documents_relative).join(relative_file);
            files.push(DocumentFile { path: exposed_file.to_string_lossy().replace('\\', "/"), kind: kind.into() });
        }
    }
    files.sort_by(|a, b| a.path.to_lowercase().cmp(&b.path.to_lowercase()));
    directories.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
    Ok(DocumentTree { files, directories })
}

fn document_files(repo: &Repository) -> Result<Vec<DocumentFile>, String> {
    Ok(scan_document_tree(repo)?.files)
}

fn validate_json(content: &str) -> Result<(), String> {
    let value: Value = serde_json::from_str(content).map_err(|e| format!("JSON の形式が正しくありません: {e}"))?;
    if value.get("fields").is_some() && !value.get("fields").is_some_and(Value::is_array) { return Err("fields は配列である必要があります".into()); }
    let fields = value.get("fields").and_then(Value::as_array).or_else(|| value.as_array());
    if let Some(fields) = fields {
        let mut ids = std::collections::HashSet::new();
        for (index, field) in fields.iter().enumerate() {
            if !field.is_object() { return Err(format!("フィールド {} はオブジェクトである必要があります", index + 1)); }
            let id = field.get("id").and_then(Value::as_str).filter(|v| !v.trim().is_empty()).ok_or_else(|| format!("フィールド {} に id が必要です", index + 1))?;
            if !ids.insert(id) { return Err(format!("フィールド ID が重複しています: {id}")); }
            if field.get("name").and_then(Value::as_str).filter(|v| !v.trim().is_empty()).is_none() { return Err(format!("フィールド {} に name が必要です", index + 1)); }
            let kind = field.get("type").and_then(Value::as_str).ok_or_else(|| format!("フィールド {} に type が必要です", index + 1))?;
            if !["string", "number", "integer", "boolean", "array", "object"].contains(&kind) { return Err(format!("フィールド {} の type がサポートされていません", index + 1)); }
            if let Some(default) = field.get("defaultValue") {
                let valid = match kind { "string" => default.is_string(), "number" => default.as_f64().is_some(), "integer" => default.as_i64().is_some() || default.as_u64().is_some(), "boolean" => default.is_boolean(), "array" => default.is_array(), "object" => default.is_object(), _ => false };
                if !valid { return Err(format!("フィールド {} の defaultValue が type と一致しません", index + 1)); }
            }
        }
    }
    Ok(())
}

fn validate_document_content(repo: &Repository, path: &str, content: &str) -> Vec<String> {
    if content.len() > 10_000_000 { return vec!["JSON ファイルが大きすぎます (上限 10 MB)".into()]; }
    let mut issues = Vec::new();
    let value: Value = match serde_json::from_str(content) {
        Ok(value) => value,
        Err(error) => return vec![format!("JSON 構文エラー: {error}")],
    };
    if let Err(error) = validate_json(content) { issues.push(error); }
    let relative = match safe_document_relative(repo, path) { Ok(p) => p, Err(e) => return vec![e] };
    let json_path = repo.root.join(relative);
    let mut candidates = Vec::new();
    let mut stem_schema = json_path.clone();
    stem_schema.set_extension("schema.json");
    candidates.push(stem_schema);
    let filename_schema = PathBuf::from(format!("{}.schema.json", json_path.to_string_lossy()));
    candidates.push(filename_schema);
    let schema_path = if path.to_ascii_lowercase().ends_with(".schema.json") { None } else { candidates.into_iter().find(|candidate| candidate.is_file()) };
    if let Some(schema_path) = schema_path {
        let canonical = match schema_path.canonicalize() {
            Ok(path) if path.starts_with(&repo.documents) => path,
            _ => return vec!["JSON Schema が設計書ディレクトリ外を参照しています".into()],
        };
        let schema: Value = match fs::read_to_string(canonical).ok().and_then(|s| serde_json::from_str(&s).ok()) {
            Some(schema) => schema,
            None => return vec!["JSON Schema ファイルを読み取れないか、JSON として不正です".into()],
        };
        let validator = match validator_for(&schema) {
            Ok(validator) => validator,
            Err(error) => return vec![format!("JSON Schema をコンパイルできません: {error}")],
        };
        issues.extend(validator.iter_errors(&value).map(|error| format!("{}: {error}", error.instance_path())));
    }
    issues
}

fn discover_documents(root: &Path) -> Result<Option<(PathBuf, String)>, String> {
    for name in ["DesignDocs", "Documentation", "DesignDocuments", "Design Documents", "design-docs", "docs/DesignDocs", "docs/design", "docs"] {
        let candidate = root.join(name);
        if fs::symlink_metadata(&candidate).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
            return Err(format!("設計書ディレクトリがシンボリックリンクです。安全のため開けません: {name}"));
        }
        if candidate.is_dir() {
            let canonical = candidate.canonicalize().map_err(|_| "設計書ディレクトリを開けません".to_string())?;
            if !canonical.starts_with(root) { return Err("設計書ディレクトリがリポジトリ外を指しています".into()); }
            let relative = canonical.strip_prefix(root).map_err(|_| "設計書ディレクトリが不正です".to_string())?.to_string_lossy().replace('\\', "/");
            return Ok(Some((canonical, relative)));
        }
    }
    Ok(None)
}

fn validate_unity_root(root: &Path) -> Result<(), String> {
    if !root.join("Assets").is_dir() || !root.join("ProjectSettings/ProjectVersion.txt").is_file() {
        return Err("Assets と ProjectSettings/ProjectVersion.txt が見つかりません。Unity プロジェクトのルートを選択してください".into());
    }
    Ok(())
}

fn discover_git_root(unity_root: &Path) -> Result<PathBuf, String> {
    let output = git(unity_root, &["rev-parse", "--show-toplevel"])?;
    if !output.status.success() { return Err("Unity プロジェクトが Git リポジトリ内にありません".into()); }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let git_root = PathBuf::from(text).canonicalize().map_err(|_| "Git リポジトリのルートを確認できません".to_string())?;
    if !unity_root.starts_with(&git_root) { return Err("検出した Git リポジトリが Unity プロジェクトを含んでいません".into()); }
    Ok(git_root)
}

fn git_relative_path(repo: &Repository, unity_relative: &Path) -> Result<PathBuf, String> {
    repo.root.join(unity_relative).strip_prefix(&repo.git_root).map(Path::to_path_buf)
        .map_err(|_| "設計書のパスが Git リポジトリ外です".into())
}

fn build_repository(root: PathBuf) -> Result<Repository, String> {
    validate_unity_root(&root)?;
    let git_root = discover_git_root(&root)?;
    let discovered = discover_documents(&root)?;
    let (documents, documents_relative, has_documents) = match discovered {
        Some((path, relative)) => (path, relative, true),
        None => (root.join("DesignDocs"), "DesignDocs".into(), false),
    };
    let git_documents_relative = documents.strip_prefix(&git_root).map_err(|_| "設計書ディレクトリが Git リポジトリ外です".to_string())?.to_string_lossy().replace('\\', "/");
    Ok(Repository { source_root: None, root, git_root, documents, documents_relative, git_documents_relative, has_documents })
}

fn project_info(repo: &Repository) -> ProjectInfo {
    ProjectInfo {
        unity_root: repo.source_root.as_ref().unwrap_or(&repo.root).to_string_lossy().to_string(),
        worktree_root: repo.source_root.as_ref().map(|_| repo.git_root.to_string_lossy().to_string()),
        development_branch: repo.source_root.as_ref().and_then(|root| git_text(root, &["symbolic-ref", "--short", "HEAD"]).ok()),
        git_root: repo.git_root.to_string_lossy().to_string(),
        documents_path: repo.has_documents.then(|| repo.documents.to_string_lossy().to_string()),
        has_documents: repo.has_documents,
    }
}

fn setup_entries() -> Vec<SetupEntry> {
    let directories = [
        "DesignDocs",
        "DesignDocs/00_総合設計",
        "DesignDocs/01_画面イメージ",
        "DesignDocs/02_処理仕様",
        "DesignDocs/03_項目仕様",
        "DesignDocs/04_API_LDI",
        "DesignDocs/05_世界観・ゲームルール",
        "DesignDocs/06_開発規約",
        "DesignDocs/images",
    ];
    let mut entries: Vec<SetupEntry> = directories.iter().map(|path| SetupEntry { path: (*path).into(), kind: "directory".into() }).collect();
    entries.push(SetupEntry { path: "DesignDocs/README.md".into(), kind: "file".into() });
    entries
}

fn validate_setup_plan(repo: &Repository, entries: &[SetupEntry]) -> Result<(), String> {
    if repo.has_documents { return Err("設計書ディレクトリはすでに存在します。既存ファイルをそのまま開いてください".into()); }
    let expected = setup_entries();
    if entries != expected { return Err("初期化項目が現在のプレビューと一致しません。プレビューを更新してください".into()); }
    for entry in entries {
        let relative = safe_relative(&entry.path)?;
        if !relative.starts_with("DesignDocs") { return Err("初期化パスが許可範囲外です".into()); }
        let target = repo.root.join(relative);
        if !target.starts_with(&repo.root) { return Err("初期化パスが Unity プロジェクト外です".into()); }
        match fs::symlink_metadata(&target) {
            Ok(_) => return Err(format!("作成先に既存のファイル、ディレクトリ、またはシンボリックリンクがあります: {}", entry.path)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {},
            Err(_) => return Err(format!("作成先を安全に確認できません: {}", entry.path)),
        }
    }
    Ok(())
}

fn preview_setup_impl(repo: &Repository) -> Result<SetupPreview, String> {
    let entries = setup_entries();
    validate_setup_plan(repo, &entries)?;
    let status = git_status_impl(&repo.git_root)?;
    Ok(SetupPreview { entries, changed_files: status.changed })
}

const INITIAL_README: &str = "# DesignDocs\n\nこのディレクトリは Unity プロジェクトの設計書を管理します。Markdown を本文の正本として扱い、別のデータ複製は作りません。\n\n## ディレクトリ\n\n- `00_総合設計`: 全体設計\n- `01_画面イメージ`: 画面イメージ\n- `02_処理仕様`: 処理仕様\n- `03_項目仕様`: 構造化データと項目仕様\n- `04_API_LDI`: API / LDI 仕様\n- `05_世界観・ゲームルール`: 世界観とルール\n- `06_開発規約`: 開発・文書規約\n- `images`: Markdown から参照する画像\n\n## 文書ルール\n\nMarkdown を通常の設計書の正本とします。JSON の構造化フィールドには変更後も維持する一意な `id` を付けます。画像は相対パスで参照してください。\n";

fn initialize_setup_impl(repo: &Repository) -> Result<(), String> {
    let entries = setup_entries();
    validate_setup_plan(repo, &entries)?;
    // This is a read-only preflight. Dirty files are preserved and reported to the caller.
    let _status_before_create = git_status_impl(&repo.git_root)?;
    let mut created_directories: Vec<PathBuf> = Vec::new();
    let mut created_readme: Option<PathBuf> = None;
    let result = (|| -> Result<(), String> {
        for entry in entries.iter().filter(|entry| entry.kind == "directory") {
            let target = repo.root.join(safe_relative(&entry.path)?);
            fs::create_dir(&target).map_err(|_| format!("ディレクトリを作成できません。競合があるため停止しました: {}", entry.path))?;
            created_directories.push(target.clone());
            let metadata = fs::symlink_metadata(&target).map_err(|_| format!("作成したディレクトリを確認できません: {}", entry.path))?;
            let canonical = target.canonicalize().map_err(|_| format!("作成したディレクトリを確認できません: {}", entry.path))?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() || !canonical.starts_with(&repo.root) {
                return Err(format!("作成先がシンボリックリンクまたは許可範囲外に変わりました: {}", entry.path));
            }
        }
        let readme_entry = entries.iter().find(|entry| entry.path == "DesignDocs/README.md").ok_or_else(|| "README の初期化項目がありません".to_string())?;
        let readme = repo.root.join(safe_relative(&readme_entry.path)?);
        let mut file = fs::OpenOptions::new().write(true).create_new(true).open(&readme)
            .map_err(|_| "README.md がすでに存在するか、安全に作成できません。既存ファイルは変更していません".to_string())?;
        created_readme = Some(readme);
        file.write_all(INITIAL_README.as_bytes()).and_then(|_| file.sync_all()).map_err(|_| "README.md の書き込みに失敗しました".to_string())?;
        Ok(())
    })();
    if let Err(error) = result {
        if let Some(path) = created_readme { let _ = fs::remove_file(path); }
        for path in created_directories.iter().rev() { let _ = fs::remove_dir(path); }
        return Err(error);
    }
    Ok(())
}

fn initialize_setup_after_confirmation(repo: &Repository, confirmed: bool) -> Result<(), String> {
    if !confirmed { return Err("初期化はユーザーが確認した後にのみ実行できます".into()); }
    initialize_setup_impl(repo)
}

fn validate_setup_git_snapshot(repo: &Repository, expected_changed_files: &[String]) -> Result<(), String> {
    let mut expected = expected_changed_files.to_vec();
    expected.sort();
    let mut current = git_status_impl(&repo.git_root)?.changed;
    current.sort();
    if current != expected { return Err("プレビュー後に Git の状態が変わりました。初期化プレビューを更新してください".into()); }
    Ok(())
}

fn safe_document_relative(repo: &Repository, path: &str) -> Result<PathBuf, String> {
    if !repo.has_documents { return Err("設計書ディレクトリはまだ作成されていません".into()); }
    let relative = safe_relative(path)?;
    let scope = Path::new(&repo.documents_relative);
    if !relative.starts_with(scope) || relative == scope { return Err("選択した設計書ディレクトリ外のパスです".into()); }
    Ok(relative)
}

fn safe_document_directory(repo: &Repository, path: &str) -> Result<PathBuf, String> {
    if !repo.has_documents { return Err("設計書ディレクトリはまだ作成されていません".into()); }
    let directory = mutation_entry(repo, path, true)?;
    if !directory.is_dir() { return Err("保存先は実在するフォルダーを選択してください".into()); }
    Ok(directory)
}

fn validate_entry_name(name: &str) -> Result<(), String> {
    if name.eq_ignore_ascii_case(".git") { return Err("Git 管理項目は操作できません".into()); }
    if name.trim().is_empty() || name.trim() != name || name == "." || name == ".." {
        return Err("名前を入力してください。前後の空白や . / .. は使用できません".into());
    }
    if name.chars().any(|ch| ch.is_control() || "<>:\"/\\|?*".contains(ch)) || name.ends_with('.') || name.ends_with(' ') {
        return Err("名前に使用できない文字が含まれています".into());
    }
    let stem = name.split('.').next().unwrap_or("").trim_end_matches([' ', '.']).to_ascii_uppercase();
    let reserved = ["CON", "PRN", "AUX", "NUL"];
    if reserved.contains(&stem.as_str()) || (1..=9).any(|n| stem == format!("COM{n}") || stem == format!("LPT{n}")) {
        return Err("Windows で予約されている名前は使用できません".into());
    }
    Ok(())
}

fn create_document_impl(repo: &Repository, parent_path: &str, name: &str, format: &str) -> Result<String, String> {
    validate_entry_name(name)?;
    let extension = match format { "markdown" => "md", "json" => "json", _ => return Err("対応していない文書形式です".into()) };
    let parent = safe_document_directory(repo, parent_path)?;
    let destination = parent.join(format!("{name}.{extension}"));
    let initial = if format == "json" { "{\n  \"fields\": []\n}\n".to_string() } else { format!("# {name}\n") };
    let mut file = fs::OpenOptions::new().write(true).create_new(true).open(&destination)
        .map_err(|error| -> String { if error.kind() == std::io::ErrorKind::AlreadyExists { "同名のファイルがすでに存在します".into() } else { "新しい文書を安全に作成できません".into() } })?;
    file.write_all(initial.as_bytes()).map_err(|_| "新しい文書を書き込めません".to_string())?;
    file.sync_all().map_err(|_| "新しい文書をディスクに保存できません".to_string())?;
    let relative = destination.canonicalize().map_err(|_| "作成した文書を確認できません".to_string())?
        .strip_prefix(&repo.root).map_err(|_| "作成した文書が Unity プロジェクト外です".to_string())?.to_path_buf();
    Ok(relative.to_string_lossy().replace('\\', "/"))
}

fn create_folder_impl(repo: &Repository, parent_path: &str, name: &str) -> Result<String, String> {
    validate_entry_name(name)?;
    let parent = safe_document_directory(repo, parent_path)?;
    let destination = parent.join(name);
    fs::create_dir(&destination).map_err(|error| -> String { if error.kind() == std::io::ErrorKind::AlreadyExists { "同名のファイルまたはフォルダーがすでに存在します".into() } else { "新しいフォルダーを安全に作成できません".into() } })?;
    let canonical = destination.canonicalize().map_err(|_| "作成したフォルダーを確認できません".to_string())?;
    if !canonical.starts_with(&repo.documents) { return Err("作成したフォルダーが設計書ディレクトリ外です".into()); }
    let relative = canonical.strip_prefix(&repo.root).map_err(|_| "作成したフォルダーが Unity プロジェクト外です".to_string())?.to_path_buf();
    Ok(relative.to_string_lossy().replace('\\', "/"))
}

fn reject_link(metadata: &fs::Metadata) -> Result<(), String> {
    #[cfg(windows)] {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 { return Err("リンク・ジャンクションは操作できません".into()); }
    }
    if metadata.file_type().is_symlink() { return Err("シンボリックリンクは操作できません".into()); }
    Ok(())
}

fn mutation_entry(repo: &Repository, path: &str, allow_root: bool) -> Result<PathBuf, String> {
    let relative = if allow_root && path == repo.documents_relative { safe_relative(path)? } else { safe_document_relative(repo, path)? };
    let mut candidate = repo.root.clone();
    for component in relative.components() {
        let name = component.as_os_str();
        validate_entry_name(&name.to_string_lossy())?;
        candidate.push(name);
        reject_link(&fs::symlink_metadata(&candidate).map_err(|_| "項目が存在しないか、アクセスできません")?)?;
    }
    let full = candidate.canonicalize().map_err(|_| "項目のパスを確認できません")?;
    if !full.starts_with(&repo.documents) { return Err("設計書の許可範囲外です".into()); }
    Ok(full)
}

fn validate_mutation_tree(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path).map_err(|_| "項目を確認できません")?;
    reject_link(&metadata)?;
    if metadata.is_dir() {
        for entry in fs::read_dir(path).map_err(|_| "フォルダーを読み取れません")? {
            let entry = entry.map_err(|_| "フォルダー内の項目を読み取れません")?;
            if entry.file_name().to_string_lossy().eq_ignore_ascii_case(".git") { return Err("Git 管理項目を含むフォルダーは操作できません".into()); }
            validate_mutation_tree(&entry.path())?;
        }
    } else if !metadata.is_file() { return Err("通常のファイルとフォルダーのみ操作できます".into()); }
    Ok(())
}

fn rename_without_replace(source: &Path, destination: &Path) -> Result<(), String> {
    if fs::symlink_metadata(destination).is_ok() { return Err("移動先に同名の項目があります。上書きしません".into()); }
    #[cfg(windows)] {
        use std::os::windows::ffi::OsStrExt;
        #[link(name = "Kernel32")]
        extern "system" { fn MoveFileExW(existing: *const u16, new: *const u16, flags: u32) -> i32; }
        let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
        let destination: Vec<u16> = destination.as_os_str().encode_wide().chain(Some(0)).collect();
        // No REPLACE_EXISTING flag: a concurrent destination creation cannot be overwritten.
        if unsafe { MoveFileExW(source.as_ptr(), destination.as_ptr(), 0) } == 0 {
            return Err("移動・名前変更に失敗しました。重名やファイルの使用状態を確認してください".into());
        }
    }
    #[cfg(not(windows))]
    fs::rename(source, destination).map_err(|_| "移動・名前変更に失敗しました")?;
    Ok(())
}

fn move_entry_impl(repo: &Repository, path: &str, parent_path: &str, name: &str) -> Result<String, String> {
    validate_entry_name(name)?;
    let source = mutation_entry(repo, path, false)?;
    let parent = mutation_entry(repo, parent_path, true)?;
    if !parent.is_dir() { return Err("移動先はフォルダーを選択してください".into()); }
    if parent.starts_with(&source) { return Err("フォルダーを自身や子孫へ移動できません".into()); }
    if source.is_file() {
        let extension = Path::new(name).extension().and_then(|ext| ext.to_str()).unwrap_or("").to_ascii_lowercase();
        if !["md", "markdown", "mmd", "json"].contains(&extension.as_str()) { return Err("文書の拡張子は md / markdown / mmd / json を指定してください".into()); }
    }
    validate_mutation_tree(&source)?;
    let destination = parent.join(name);
    rename_without_replace(&source, &destination)?;
    Ok(destination.strip_prefix(&repo.root).map_err(|_| "移動先が許可範囲外です")?.to_string_lossy().replace('\\', "/"))
}

fn delete_entry_impl(repo: &Repository, path: &str, confirmed: bool) -> Result<(), String> {
    if !confirmed { return Err("削除には確認が必要です".into()); }
    let source = mutation_entry(repo, path, false)?;
    validate_mutation_tree(&source)?;
    if source.is_dir() { fs::remove_dir_all(source) } else { fs::remove_file(source) }
        .map_err(|_| "削除を完了できませんでした。フォルダー内の一部が削除された可能性があります。一覧とアクセス権・使用状態を確認してください".into())
}

#[tauri::command]
fn move_document_entry(path: String, parent_path: String, name: String, state: State<'_, RepoState>) -> Result<String, String> {
    let _operation = operation_guard()?;
    move_entry_impl(&repository(&state)?, &path, &parent_path, &name)
}

#[tauri::command]
fn delete_document_entry(path: String, confirmed: bool, state: State<'_, RepoState>) -> Result<(), String> {
    let _operation = operation_guard()?;
    delete_entry_impl(&repository(&state)?, &path, confirmed)
}

fn resolve_image_reference(repo: &Repository, document_path: &str, reference: &str) -> Result<PathBuf, String> {
    let doc = safe_document_relative(repo, document_path)?;
    let ref_path = Path::new(reference);
    if ref_path.is_absolute() || reference.contains(':') || reference.starts_with('\\') { return Err("画像参照は設計書内の相対パスにしてください".into()); }
    let scope_depth = Path::new(&repo.documents_relative).components().count();
    let mut parts: Vec<std::ffi::OsString> = doc.parent().unwrap_or(Path::new("")).components().filter_map(|c| match c { Component::Normal(v) => Some(v.to_os_string()), _ => None }).collect();
    for component in ref_path.components() {
        match component {
            Component::Normal(v) => parts.push(v.to_os_string()),
            Component::CurDir => {},
            Component::ParentDir if parts.len() > scope_depth => { parts.pop(); },
            _ => return Err("画像参照のパスが設計書ディレクトリ外です".into()),
        }
    }
    if parts.len() <= scope_depth { return Err("画像参照のパスが設計書ディレクトリ外です".into()); }
    let mut resolved = PathBuf::new();
    for part in parts { resolved.push(part); }
    Ok(resolved)
}

fn atomic_write(destination: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = destination.parent().ok_or_else(|| "保存先ディレクトリが不正です".to_string())?;
    let mut temp_path = None;
    let mut file = None;
    for _ in 0..16 {
        let nonce = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
          let candidate = parent.join(format!(".designbook-{}.{}.tmp", std::process::id(), nonce));
        match fs::OpenOptions::new().write(true).create_new(true).open(&candidate) {
            Ok(opened) => { temp_path = Some(candidate); file = Some(opened); break; },
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return Err("一時ファイルを安全に作成できません".into()),
        }
    }
    let temp_path = temp_path.ok_or_else(|| "一時ファイル名を確保できません".to_string())?;
    let mut file = file.ok_or_else(|| "一時ファイルを開けません".to_string())?;
    if file.write_all(bytes).and_then(|_| file.sync_all()).is_err() { let _ = fs::remove_file(&temp_path); return Err("一時ファイルへの書き込みに失敗しました".into()); }
    drop(file);
    if fs::rename(&temp_path, destination).is_err() {
        let _ = fs::remove_file(&temp_path);
        return Err("安全な置換に失敗しました。元ファイルは保持されています".into());
    }
    Ok(())
}

#[tauri::command]
fn open_repository(path: String, state: State<'_, RepoState>) -> Result<ProjectInfo, String> {
    let _operation = operation_guard()?;
    // 過去の選択履歴に関係なく、独立した設計書の正本だけを開く。
    let _ = path;
    let root = PathBuf::from(r"D:\UnityProjects\Xiuxian_DesignBook").canonicalize().map_err(|_| "独立設計書リポジトリがありません".to_string())?;
    let repo = workspace::open(&root)?;
    let info = project_info(&repo);
    *state.0.lock().map_err(|_| "リポジトリ状態を更新できません".to_string())? = Some(repo);
    Ok(info)
}

#[tauri::command]
fn project_context(state: State<'_, RepoState>) -> Result<ProjectInfo, String> {
    Ok(project_info(&repository(&state)?))
}

#[tauri::command]
fn close_repository(state: State<'_, RepoState>) -> Result<(), String> {
    let _operation = operation_guard()?;
    *state.0.lock().map_err(|_| "リポジトリ状態を更新できません".to_string())? = None;
    Ok(())
}

#[tauri::command]
fn preview_document_setup(state: State<'_, RepoState>) -> Result<SetupPreview, String> {
    preview_setup_impl(&repository(&state)?)
}

#[tauri::command]
fn create_document_setup(confirmed: bool, expected_changed_files: Vec<String>, state: State<'_, RepoState>) -> Result<ProjectInfo, String> {
    let _operation = operation_guard()?;
    let repo = repository(&state)?;
    if !confirmed { return Err("初期化はユーザーが確認した後にのみ実行できます".into()); }
    validate_setup_git_snapshot(&repo, &expected_changed_files)?;
    initialize_setup_after_confirmation(&repo, confirmed)?;
    let updated = if let Some(source) = &repo.source_root { workspace::open(source)? } else { build_repository(repo.root)? };
    let info = project_info(&updated);
    *state.0.lock().map_err(|_| "リポジトリ状態を更新できません".to_string())? = Some(updated);
    Ok(info)
}

#[tauri::command]
fn list_documents(state: State<'_, RepoState>) -> Result<Vec<DocumentFile>, String> { document_files(&repository(&state)?) }

#[tauri::command]
fn list_document_directories(state: State<'_, RepoState>) -> Result<Vec<String>, String> {
    Ok(scan_document_tree(&repository(&state)?)?.directories)
}

#[tauri::command]
fn create_document(parent_path: String, name: String, format: String, state: State<'_, RepoState>) -> Result<String, String> {
    let _operation = operation_guard()?;
    create_document_impl(&repository(&state)?, &parent_path, &name, &format)
}

#[tauri::command]
fn create_document_folder(parent_path: String, name: String, state: State<'_, RepoState>) -> Result<String, String> {
    let _operation = operation_guard()?;
    create_folder_impl(&repository(&state)?, &parent_path, &name)
}

#[tauri::command]
fn validate_document(path: String, content: String, state: State<'_, RepoState>) -> Result<Vec<String>, String> {
    let repo = repository(&state)?;
    if !path.to_ascii_lowercase().ends_with(".json") { return Ok(Vec::new()); }
    Ok(validate_document_content(&repo, &path, &content))
}

#[tauri::command]
fn check_image_references(document_path: String, references: Vec<String>, state: State<'_, RepoState>) -> Result<Vec<String>, String> {
    let repo = repository(&state)?;
    check_image_references_impl(&repo, &document_path, references)
}

fn check_image_references_impl(repo: &Repository, document_path: &str, references: Vec<String>) -> Result<Vec<String>, String> {
    let mut missing = Vec::new();
    for reference in references {
        let lower = reference.to_ascii_lowercase();
        if lower.starts_with("http:") || lower.starts_with("https:") || lower.starts_with("data:") || reference.starts_with('#') { continue; }
        let local_path = reference.split(|c| c == '?' || c == '#').next().unwrap_or(&reference).to_string();
        let resolved = match resolve_image_reference(repo, document_path, &local_path) { Ok(p) => p, Err(_) => { missing.push(reference); continue; } };
        let full = repo.root.join(resolved);
        match full.canonicalize() {
            Ok(path) if path.starts_with(&repo.documents) && path.is_file() => {},
            _ => missing.push(reference),
        }
    }
    Ok(missing)
}

#[tauri::command]
fn import_image(document_path: String, source_path: String, replace_path: Option<String>, state: State<'_, RepoState>) -> Result<ImportedImage, String> {
    let _operation = operation_guard()?;
    let repo = repository(&state)?;
    import_image_impl(&repo, &document_path, &source_path, replace_path.as_deref())
}

fn import_image_impl(repo: &Repository, document_path: &str, source_path: &str, replace_path: Option<&str>) -> Result<ImportedImage, String> {
    let _doc = safe_document_relative(repo, document_path)?;
    let source = PathBuf::from(source_path).canonicalize().map_err(|_| "選択した画像を開けません".to_string())?;
    if !source.is_file() { return Err("選択した画像ファイルが不正です".into()); }
    let ext = source.extension().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
    if !["png", "jpg", "jpeg", "svg"].contains(&ext.as_str()) { return Err("PNG、JPG、JPEG、SVG の画像を選択してください".into()); }
    let metadata = fs::metadata(&source).map_err(|_| "画像情報を読み取れません".to_string())?;
    if metadata.len() == 0 || metadata.len() > 30_000_000 { return Err("画像サイズは 1 byte 以上 30 MB 以下にしてください".into()); }
    let destination = if let Some(replacement) = replace_path {
        let relative = resolve_image_reference(repo, document_path, replacement)?;
        let target = repo.root.join(relative);
        let canonical = target.canonicalize().map_err(|_| "置き換える画像が見つかりません".to_string())?;
        if !canonical.starts_with(&repo.documents) || !canonical.is_file() { return Err("設計書ディレクトリ外の画像は置き換えできません".into()); }
        let target_ext = canonical.extension().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
        if target_ext != ext { return Err("置き換える画像と同じ拡張子のファイルを選んでください".into()); }
        canonical
    } else {
        let images = repo.documents.join("images");
        fs::create_dir_all(&images).map_err(|_| "images ディレクトリを作成できません".to_string())?;
        let canonical_images = images.canonicalize().map_err(|_| "images ディレクトリを開けません".to_string())?;
        if !canonical_images.starts_with(&repo.documents) { return Err("images ディレクトリが設計書の外を指しています".into()); }
        let stem = source.file_stem().unwrap_or_default().to_string_lossy();
        let mut candidate = canonical_images.join(format!("{stem}.{ext}"));
        let mut suffix = 2;
        while candidate.exists() { candidate = canonical_images.join(format!("{stem}-{suffix}.{ext}")); suffix += 1; }
        candidate
    };
    let bytes = fs::read(&source).map_err(|_| "画像を読み取れません".to_string())?;
    atomic_write(&destination, &bytes)?;
    let relative = destination.strip_prefix(&repo.root).map_err(|_| "画像保存先がリポジトリ外です".to_string())?;
    let path = relative.to_string_lossy().replace('\\', "/");
    Ok(ImportedImage { path })
}

#[tauri::command]
async fn search_documents(query: String, state: State<'_, RepoState>) -> Result<SearchResults, String> {
    let repo = repository(&state)?;
    tauri::async_runtime::spawn_blocking(move || search_documents_impl(&repo, &query, 1000)).await
        .map_err(|_| "検索処理を完了できませんでした".to_string())?
}

fn search_documents_impl(repo: &Repository, query: &str, limit: usize) -> Result<SearchResults, String> {
    let query = query.trim();
    let needle: Vec<char> = query.chars().collect();
    let mut result = SearchResults { matches: Vec::new(), truncated: false, skipped: Vec::new() };
    if needle.is_empty() { return Ok(result); }
    if needle.len() > 200 { return Err("検索語は200文字以内で入力してください".into()); }
    for file in document_files(repo)? {
        let text = safe_existing(repo, &file.path).ok().and_then(|full| {
            fs::metadata(&full).ok().filter(|metadata| metadata.len() <= 5_000_000)?;
            fs::read_to_string(full).ok()
        });
        let mut content_found = false;
        if let Some(text) = text {
            for (line_index, line) in text.lines().enumerate() {
                let chars: Vec<char> = line.chars().collect();
                let mut start = 0;
                while start + needle.len() <= chars.len() {
                    if !chars[start..start + needle.len()].iter().zip(&needle).all(|(a, b)| a.to_lowercase().eq(b.to_lowercase())) { start += 1; continue; }
                    if result.matches.len() >= limit { result.truncated = true; return Ok(result); }
                    content_found = true;
                    let excerpt_start = start.saturating_sub(45);
                    result.matches.push(SearchMatch {
                        path: file.path.clone(), kind: file.kind.clone(), query: query.into(),
                        line: line_index + 1,
                        // UTF-16 columns align with browser selection offsets, including emoji.
                        column: chars[..start].iter().map(|ch| ch.len_utf16()).sum::<usize>() + 1,
                        length: chars[start..start + needle.len()].iter().map(|ch| ch.len_utf16()).sum(),
                        excerpt_offset: chars[excerpt_start..start].iter().map(|ch| ch.len_utf16()).sum(),
                        excerpt: chars[excerpt_start..].iter().take((needle.len() + 90).max(180)).collect(),
                    });
                    start += needle.len();
                }
            }
        } else { result.skipped.push(file.path.clone()); }
        if !content_found && file.path.to_lowercase().contains(&query.to_lowercase()) {
            if result.matches.len() >= limit { result.truncated = true; return Ok(result); }
            result.matches.push(SearchMatch { path: file.path, kind: file.kind, excerpt: String::new(), line: 0, column: 0, length: 0, excerpt_offset: 0, query: query.into() });
        }
    }
    Ok(result)
}

#[tauri::command]
fn read_document(path: String, state: State<'_, RepoState>) -> Result<String, String> {
    let repo = repository(&state)?;
    let full = safe_existing(&repo, &path)?;
    fs::read_to_string(full).map_err(|_| "ファイルをテキストとして読み込めません".into())
}

#[tauri::command]
fn save_document(path: String, content: String, state: State<'_, RepoState>) -> Result<(), String> {
    let _operation = operation_guard()?;
    save_document_impl(&repository(&state)?, &path, &content)
}

fn save_document_impl(repo: &Repository, path: &str, content: &str) -> Result<(), String> {
    let full = safe_existing(repo, path)?;
    let ext = full.extension().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
    if ext == "json" {
        let issues = validate_document_content(repo, path, content);
        if !issues.is_empty() { return Err(issues.join("\n")); }
    }
    if content.len() > 10_000_000 { return Err("ファイルが大きすぎます".into()); }
    atomic_write(&full, content.as_bytes())
}

fn git_status_impl(root: &Path) -> Result<GitState, String> {
    let branch = git_text(root, &["branch", "--show-current"]).unwrap_or_else(|_| "detached".into());
    let status = git(root, &["status", "--porcelain=v1", "-z", "--untracked-files=all"])?;
    if !status.status.success() { return Err("Git の作業状態を読み取れませんでした".into()); }
    let changes = parse_git_changes(&status.stdout)?;
    let changed: Vec<String> = changes.iter().map(|change| change.path.clone()).collect();
    // Resolve the upstream from the current local branch ref instead of relying on
    // the context-sensitive `@{upstream}` revision shorthand. The full ref from
    // for-each-ref is stable across shells, Git versions, and localized output.
    let upstream_ref = if branch == "detached" || branch.is_empty() {
        None
    } else {
        let local_ref = format!("refs/heads/{branch}");
        let upstream = git(root, &["for-each-ref", "--format=%(upstream)", &local_ref])?;
        if !upstream.status.success() { return Err("現在のブランチの追跡先を確認できませんでした".into()); }
        String::from_utf8_lossy(&upstream.stdout).lines().next().map(str::trim).filter(|value| !value.is_empty()).map(str::to_string)
    };
    let has_upstream = upstream_ref.is_some();
    let mut tracking_error = None;
    let (ahead, behind) = if has_upstream {
        let upstream_ref = upstream_ref.as_deref().expect("upstream_ref checked above");
        let range = format!("HEAD...{upstream_ref}");
        match git_text(root, &["rev-list", "--left-right", "--count", &range]) {
            Ok(counts) => {
                let values: Vec<usize> = counts.split_whitespace().filter_map(|value| value.parse().ok()).collect();
                if values.len() == 2 { (values[0], values[1]) }
                else { return Err("Git ahead/behind の出力形式が不正です".into()); }
            }
            Err(_) => {
                tracking_error = Some("upstream は設定済みですが、追跡先の履歴を読み取れません。remote と対象ブランチを確認してください。必要な Fetch はユーザー操作で実行してください。".into());
                (0, 0)
            }
        }
    } else { (0, 0) };
    Ok(GitState { branch, clean: changed.is_empty(), changed, changes, document_changed: Vec::new(), other_changed: Vec::new(), ahead, behind, has_upstream, tracking_error })
}

#[tauri::command]
fn git_status(state: State<'_, RepoState>) -> Result<GitState, String> {
    let repo = repository(&state)?;
    Ok(scope_git_status(git_status_impl(&repo.git_root)?, &repo.git_documents_relative))
}

fn git_branches_impl(root: &Path) -> Result<Vec<GitBranch>, String> {
    let current = git_text(root, &["branch", "--show-current"]).unwrap_or_default();
    let format = "--format=%(refname:short)%09%(upstream:short)";
    let locals = git_command_result(root, &["for-each-ref", format, "refs/heads"], "ローカルブランチ一覧")?;
    let remotes = git_command_result(root, &["for-each-ref", format, "refs/remotes"], "リモートブランチ一覧")?;
    let mut branches = Vec::new();
    for (text, is_remote) in [(locals, false), (remotes, true)] {
        for line in text.lines().filter(|line| !line.is_empty()) {
            let mut parts = line.splitn(2, '\t');
            let name = parts.next().unwrap_or_default().to_string();
            if name.ends_with("/HEAD") { continue; }
            let upstream = parts.next().filter(|value| !value.is_empty()).map(str::to_string);
            let (ahead, behind) = if !is_remote {
                if let Some(ref target) = upstream {
                    let range = format!("refs/heads/{name}...{target}");
                    let counts = git_text(root, &["rev-list", "--left-right", "--count", &range])?;
                    let mut counts = counts.split_whitespace();
                    (counts.next().and_then(|value| value.parse().ok()).unwrap_or(0), counts.next().and_then(|value| value.parse().ok()).unwrap_or(0))
                } else { (0, 0) }
            } else { (0, 0) };
            branches.push(GitBranch { current: !is_remote && name == current, name, remote: is_remote, upstream, ahead, behind });
        }
    }
    Ok(branches)
}

fn git_remotes_impl(root: &Path) -> Result<Vec<GitRemote>, String> {
    let names = git_command_result(root, &["remote"], "リモート一覧")?;
    names.lines().filter(|name| !name.is_empty()).map(|name| {
        let url = git_command_result(root, &["remote", "get-url", name], "リモート URL の取得")?;
        Ok(GitRemote { name: name.into(), url })
    }).collect()
}

fn require_remote(root: &Path, remote: &str) -> Result<(), String> {
    if git_remotes_impl(root)?.iter().any(|item| item.name == remote) { Ok(()) }
    else { Err(format!("リモートが存在しません: {remote}")) }
}

fn validate_branch_name(root: &Path, name: &str) -> Result<(), String> {
    if name.trim().is_empty() { return Err("ブランチ名を入力してください".into()); }
    git_command_result(root, &["check-ref-format", "--branch", name], "ブランチ名の検証").map(|_| ())
}

fn git_create_branch_impl(root: &Path, name: &str, start_point: &str) -> Result<String, String> {
    validate_branch_name(root, name)?;
    let branches = git_branches_impl(root)?;
    if !branches.iter().any(|branch| branch.name == start_point) { return Err(format!("開始元ブランチが存在しません: {start_point}")); }
    git_command_result(root, &["branch", name, start_point], "ブランチ作成")
}

fn git_switch_branch_impl(root: &Path, name: &str) -> Result<String, String> {
    let status = git_status_impl(root)?;
    if !status.clean { return Err(format!("未コミット変更が {} 件あります。切替前に保存またはコミットしてください", status.changed.len())); }
    if !git_branches_impl(root)?.iter().any(|branch| !branch.remote && branch.name == name) { return Err(format!("ローカルブランチが存在しません: {name}")); }
    git_command_result(root, &["switch", "--", name], "ブランチ切替")
}

fn git_remote_change_impl(root: &Path, action: &str, name: &str, url: Option<&str>) -> Result<String, String> {
    if name.trim().is_empty() || name.starts_with('-') || name.contains('/') { return Err("無効なリモート名です".into()); }
    match action {
        "add" => {
            if git_remotes_impl(root)?.iter().any(|remote| remote.name == name) { return Err(format!("リモート名がすでに存在します: {name}")); }
            let url = url.filter(|value| !value.trim().is_empty()).ok_or_else(|| "リモート URL を入力してください".to_string())?;
            git_command_result(root, &["remote", "add", name, url], "リモート追加")
        }
        "set-url" => {
            require_remote(root, name)?;
            let url = url.filter(|value| !value.trim().is_empty()).ok_or_else(|| "リモート URL を入力してください".to_string())?;
            git_command_result(root, &["remote", "set-url", name, url], "リモート URL 更新")
        }
        "remove" => {
            require_remote(root, name)?;
            git_command_result(root, &["remote", "remove", name], "リモート削除")
        }
        _ => Err("不明なリモート操作です".into()),
    }
}

fn git_set_upstream_impl(root: &Path, remote: &str, branch: &str) -> Result<String, String> {
    require_remote(root, remote)?;
    let upstream = format!("{remote}/{branch}");
    if !git_branches_impl(root)?.iter().any(|item| item.remote && item.name == upstream) { return Err(format!("リモートブランチが存在しません: {upstream}。既存 upstream の設定ではなく、初回 Push を選択してください")); }
    git_command_result(root, &["branch", "--set-upstream-to", &upstream], "upstream 設定")
}

fn git_fetch_impl(root: &Path, remote: Option<&str>) -> Result<String, String> {
    if let Some(remote) = remote { require_remote(root, remote)?; git_command_result(root, &["fetch", remote], "Fetch") }
    else { git_command_result(root, &["fetch", "--all"], "Fetch") }
}

fn git_push_preview_impl(root: &Path, remote: &str, branch: &str) -> Result<GitPushPreview, String> {
    require_remote(root, remote)?;
    validate_branch_name(root, branch)?;
    let status = git_status_impl(root)?;
    let make = |ahead, behind, target_exists, ready, message: String| GitPushPreview { current_branch: status.branch.clone(), remote: remote.into(), remote_branch: branch.into(), changed_count: status.changed.len(), ahead, behind, target_exists, ready, message };
    if status.branch.is_empty() { return Ok(make(None, None, false, false, "detached HEAD からは Push できません。ローカルブランチへ切り替えてください".into())); }
    if !git(root, &["rev-parse", "--verify", "HEAD"])?.status.success() {
        return Ok(make(Some(0), Some(0), false, false, "Push できるローカルコミットがありません。先に Commit してください".into()));
    }
    let remote_result = git(root, &["ls-remote", "--heads", remote, &format!("refs/heads/{branch}")])?;
    if !remote_result.status.success() { return Err(safe_git_output(&remote_result)); }
    let remote_text = String::from_utf8_lossy(&remote_result.stdout);
    let remote_oid = remote_text.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        let oid = parts.next()?;
        let refname = parts.next()?;
        (refname == format!("refs/heads/{branch}")).then(|| oid.to_string())
    });
    if let Some(remote_oid) = remote_oid {
        if git(root, &["cat-file", "-e", &format!("{remote_oid}^{{commit}}")])?.status.success() {
            if !git(root, &["merge-base", "--is-ancestor", &remote_oid, "HEAD"])?.status.success() {
                let remote_ahead = git(root, &["merge-base", "--is-ancestor", "HEAD", &remote_oid])?.status.success();
                let counts = git_text(root, &["rev-list", "--left-right", "--count", &format!("HEAD...{remote_oid}")]).ok();
                let mut parts = counts.as_deref().unwrap_or("").split_whitespace();
                let ahead = parts.next().and_then(|value| value.parse().ok());
                let behind = parts.next().and_then(|value| value.parse().ok());
                let message = if remote_ahead { format!("Push 先がローカルより {} 件進んでいます。Fetch/Pull を先に実行してください", behind.unwrap_or(0)) } else { "Push 先とローカルの履歴が分岐しています。Fetch して履歴を確認してください".into() };
                return Ok(make(ahead, behind, true, false, message));
            }
            let local_ahead = git_text(root, &["rev-list", "--count", &format!("{remote_oid}..HEAD")])?.parse::<usize>().unwrap_or(0);
            if local_ahead == 0 { return Ok(make(Some(0), Some(0), true, false, format!("Push 先 {remote}/{branch} に送る新しいコミットがありません"))); }
            return Ok(make(Some(local_ahead), Some(0), true, true, format!("{local_ahead} 件の既存コミットを Push できます")));
        } else {
            return Ok(make(None, None, true, false, format!("Push 先 {remote}/{branch} の履歴がローカルにありません。先に Fetch してください")));
        }
    }
    let commits = git_text(root, &["rev-list", "--count", "HEAD"])?.parse::<usize>().unwrap_or(0);
    if commits == 0 { return Ok(make(Some(0), Some(0), false, false, "Push できるローカルコミットがありません".into())); }
    Ok(make(Some(commits), Some(0), false, true, format!("リモートブランチを新規作成し、{commits} 件の既存コミットを Push できます")))
}

fn git_push_target_impl(root: &Path, remote: &str, branch: &str, set_upstream: bool) -> Result<String, String> {
    let preview = git_push_preview_impl(root, remote, branch)?;
    if !preview.ready { return Err(preview.message); }
    let refspec = format!("HEAD:refs/heads/{branch}");
    let mut args = vec!["push"];
    if set_upstream { args.push("--set-upstream"); }
    args.extend([remote, &refspec]);
    git_command_result(root, &args, "Push")
}

fn git_pull_ff_impl(root: &Path, remote: &str, branch: &str) -> Result<String, String> {
    require_remote(root, remote)?;
    validate_branch_name(root, branch)?;
    let status = git_status_impl(root)?;
    if !status.clean { return Err(format!("未コミット変更が {} 件あります。Pull の前に保存またはコミットしてください", status.changed.len())); }
    if status.branch.is_empty() { return Err("detached HEAD からは Pull できません".into()); }
    let fetch = git_command_result(root, &["fetch", remote, branch], "Pull 前の Fetch")?;
    let merge = git(root, &["merge", "--ff-only", "FETCH_HEAD"])?;
    if !merge.status.success() {
        let detail = safe_git_output(&merge);
        return Err(if detail.is_empty() { "Fast-forward できません。履歴が分岐している可能性があります。Merge は実行していません".into() } else { detail });
    }
    let result = safe_git_output(&merge);
    Ok(if result.is_empty() { fetch } else { result })
}

#[tauri::command]
fn git_branches(state: State<'_, RepoState>) -> Result<Vec<GitBranch>, String> { git_branches_impl(&repo_root(&state)?) }

#[tauri::command]
fn git_remotes(state: State<'_, RepoState>) -> Result<Vec<GitRemote>, String> { git_remotes_impl(&repo_root(&state)?) }

#[tauri::command]
fn git_create_branch(name: String, start_point: String, state: State<'_, RepoState>) -> Result<String, String> {
    let _operation = operation_guard()?;
    if repository(&state)?.source_root.is_some() { return Err("設計書は main 固定です".into()); }
    git_create_branch_impl(&repo_root(&state)?, &name, &start_point) }

#[tauri::command]
fn git_switch_branch(name: String, state: State<'_, RepoState>) -> Result<String, String> {
    let _operation = operation_guard()?;
    let repo = repository(&state)?;
    if repo.source_root.is_some() { return Err("設計書 Worktree は main 固定です。Unity 開発ブランチは変更しません。".into()); }
    git_switch_branch_impl(&repo.git_root, &name)
}

#[tauri::command]
fn git_remote_change(action: String, name: String, url: Option<String>, state: State<'_, RepoState>) -> Result<String, String> {
    let _operation = operation_guard()?;
    if repository(&state)?.source_root.is_some() { return Err("設計書の origin は移行時に固定します".into()); }
    git_remote_change_impl(&repo_root(&state)?, &action, &name, url.as_deref()) }

#[tauri::command]
fn git_set_upstream(remote: String, branch: String, state: State<'_, RepoState>) -> Result<String, String> {
    let _operation = operation_guard()?;
    if repository(&state)?.source_root.is_some() && (remote != "origin" || branch != "main") { return Err("origin/main のみ使用できます".into()); }
    git_set_upstream_impl(&repo_root(&state)?, &remote, &branch) }

#[tauri::command]
fn git_fetch(remote: Option<String>, state: State<'_, RepoState>) -> Result<String, String> {
    let _operation = operation_guard()?; git_fetch_impl(&repo_root(&state)?, remote.as_deref()) }

#[tauri::command]
fn git_push_target(remote: String, branch: String, set_upstream: bool, expected_source: String, state: State<'_, RepoState>) -> Result<String, String> {
    let _operation = operation_guard()?;
    let repo = repository(&state)?;
    if repo.source_root.is_some() && (remote != "origin" || branch != "main") { return Err("設計書の送信先ブランチは main に限定しています".into()); }
    let root = repo.git_root;
    if expected_source.is_empty() || git_status_impl(&root)?.branch != expected_source {
        return Err("送信元ブランチが変更されました。状態を再読み込みしてください".into());
    }
    git_push_target_impl(&root, &remote, &branch, set_upstream)
}

#[tauri::command]
fn git_push_preview(remote: String, branch: String, state: State<'_, RepoState>) -> Result<GitPushPreview, String> {
    let repo = repository(&state)?;
    if repo.source_root.is_some() && (remote != "origin" || branch != "main") { return Err("設計書の送信先ブランチは main に限定しています".into()); }
    git_push_preview_impl(&repo.git_root, &remote, &branch)
}

#[tauri::command]
fn git_pull_ff(remote: String, branch: String, state: State<'_, RepoState>) -> Result<String, String> {
    let _operation = operation_guard()?;
    let repo = repository(&state)?;
    if repo.source_root.is_some() { return Err("設計書の更新は更新履歴パネルの『更新を確認』から実行してください。main の安全な早送り更新と履歴保存を行います。".into()); }
    git_pull_ff_impl(&repo.git_root, &remote, &branch)
}

#[tauri::command]
fn git_diff(state: State<'_, RepoState>) -> Result<String, String> {
    let repo = repository(&state)?;
    if !repo.has_documents { return Err("設計書ディレクトリがまだありません。先にプレビューから作成してください".into()); }
    let root = &repo.git_root;
    let scope = repo.git_documents_relative.as_str();
    let output = git_text(root, &["diff", "--", scope]).and_then(|unstaged| {
        let staged = git_text(root, &["diff", "--cached", "--", scope])?;
        Ok(if staged.is_empty() { unstaged } else if unstaged.is_empty() { staged } else { format!("{staged}\n{unstaged}") })
    })?;
    let untracked = git(root, &["ls-files", "--others", "--exclude-standard", "-z", "--", scope])?;
    if !untracked.status.success() { return Err("未追跡ファイルの一覧を取得できません".into()); }
    let mut additions = Vec::new();
    for name in String::from_utf8_lossy(&untracked.stdout).split('\0').filter(|name| !name.is_empty()).take(300) {
        if name.contains('\r') || name.contains('\n') { continue; }
        let file = match safe_existing(&repo, name) { Ok(file) => file, Err(_) => continue };
        let metadata = match fs::metadata(&file) { Ok(metadata) if metadata.len() <= 2_000_000 => metadata, _ => continue };
        if metadata.len() > 2_000_000 { continue; }
        let text = match fs::read_to_string(file) { Ok(text) => text, Err(_) => continue };
        let lines: Vec<&str> = text.lines().collect();
        let body = lines.iter().map(|line| format!("+{line}")).collect::<Vec<_>>().join("\n");
        additions.push(format!("diff --git a/{name} b/{name}\nnew file mode 100644\n--- /dev/null\n+++ b/{name}\n@@ -0,0 +1,{} @@\n{body}", lines.len()));
    }
    if additions.is_empty() { Ok(output) } else if output.is_empty() { Ok(additions.join("\n")) } else { Ok(format!("{output}\n{}", additions.join("\n"))) }
}

#[cfg(test)]
fn git_sync_impl(root: &Path) -> Result<String, String> {
    let before = git_status_impl(root)?;
    if !before.clean { return Err("未コミットの変更があります。変更を保存・コミットしてから同期してください".into()); }
    if !before.has_upstream { return Err("現在のブランチに追跡リモートがありません。Git で upstream を設定してください".into()); }
    let fetch = git(root, &["fetch", "--prune"])?;
    if !fetch.status.success() { return Err("リモートの更新を取得できませんでした。認証とネットワークを確認してください".into()); }
    let after = git_status_impl(root)?;
    if after.ahead > 0 && after.behind > 0 { return Err("ローカルとリモートの履歴が分岐しています。安全のため同期を中止しました".into()); }
    if after.behind == 0 { return Ok("リモートに新しいコミットはありません".into()); }
    let pull = git(root, &["merge", "--ff-only", "@{upstream}"])?;
    if !pull.status.success() { return Err("Fast-forward 同期に失敗しました。ローカルの内容はリセットしていません".into()); }
    Ok(format!("リモートから {} 件のコミットを Fast-forward で同期しました", after.behind))
}

#[tauri::command]
fn git_commit(message: String, state: State<'_, RepoState>) -> Result<String, String> {
    let _operation = operation_guard()?;
    if message.trim().is_empty() { return Err("コミットメッセージを入力してください".into()); }
    let repo = repository(&state)?;
    git_commit_impl(&repo, &message)
}

fn git_commit_impl(repo: &Repository, message: &str) -> Result<String, String> {
    if !repo.has_documents { return Err("設計書ディレクトリがまだありません。先に作成してください".into()); }
    let root = &repo.git_root;
    let scope = repo.git_documents_relative.as_str();
    if let Some(source) = &repo.source_root {
        workspace::validate_target(source, root)?;
        let staged = git(root, &["diff", "--cached", "--name-only", "--no-renames", "-z"])?;
        if !staged.status.success() { return Err("ステージ済みファイルを確認できません".into()); }
        let outside: Vec<_> = staged.stdout.split(|b| *b == 0).filter(|path| !path.is_empty())
            .map(|path| String::from_utf8_lossy(path).into_owned())
            .filter(|path| path != scope && !path.strip_prefix(scope).is_some_and(|rest| rest.starts_with('/'))).collect();
        if !outside.is_empty() { return Err(format!("DesignDocs 範囲外のステージ済み変更があります。index を変更せず Commit を停止しました: {}", outside.join(", "))); }
    }
    for document in document_files(repo)?.into_iter().filter(|file| file.kind == "json") {
        let full = safe_existing(repo, &document.path)?;
        let content = fs::read_to_string(full).map_err(|_| format!("JSON を読み込めません: {}", document.path))?;
        let issues = validate_document_content(repo, &document.path, &content);
        if !issues.is_empty() { return Err(format!("コミット前の JSON 検証に失敗しました: {}\n{}", document.path, issues.join("\n"))); }
    }
    let add = git(root, &["add", "-A", "--", scope])?;
    if !add.status.success() { return Err("設計書ディレクトリを stage できませんでした".into()); }
    let staged = git_text(root, &["diff", "--cached", "--name-only", "--", scope])?;
    if staged.is_empty() { return Err("選択した設計書ディレクトリにコミットする変更がありません".into()); }
    // `--only` constrains this commit to the selected docs path, leaving unrelated staged work alone.
    let previous = git_text(root, &["rev-parse", "--verify", "HEAD"]).ok();
    let result = git_command_result(root, &["commit", "--only", "-m", message, "--", scope], "Commit")?;
    let hash = git_text(root, &["rev-parse", "--verify", "HEAD"])?;
    if previous.as_deref() == Some(hash.as_str()) { return Err("Commit 後に新しいコミットを確認できませんでした".into()); }
    Ok(format!("Commit 成功: {hash}\n{result}"))
}

#[cfg(test)]
fn git_push_impl(root: &Path) -> Result<String, String> {
    let fetch = git(root, &["fetch"])?;
    if !fetch.status.success() { return Err("プッシュ前のリモート確認に失敗しました".into()); }
    let status = git_status_impl(root)?;
    let remote = validate_push_preflight(&status)?;
    let push = git(root, &["push"])?;
    if !push.status.success() { return Err("プッシュできませんでした。リモートの状態と認証を確認してください".into()); }
    Ok(format!("ブランチ {} の既存コミットを Push しました。{}。", status.branch, remote))
}

#[cfg(test)]
fn validate_push_preflight(status: &GitState) -> Result<String, String> {
    let remote = if status.has_upstream { format!("リモートとの差分: ahead {} / behind {}", status.ahead, status.behind) } else { "リモート追跡先: 未設定".into() };
    if !status.has_upstream { return Err(format!("ブランチ {}。{}。Git で upstream を設定してください。Push は既存コミットのみを送信します。", status.branch, remote)); }
    if status.behind > 0 { return Err(format!("ブランチ {} はリモートより {} 件遅れています。{}。先に安全な同期を行ってください。", status.branch, status.behind, remote)); }
    if status.ahead == 0 { return Err(format!("ブランチ {} に Push する既存コミットがありません。{}。", status.branch, remote)); }
    Ok(remote)
}

#[tauri::command]
fn git_history(path: String, state: State<'_, RepoState>) -> Result<Vec<String>, String> {
    let repo = repository(&state)?;
    let rel = safe_document_relative(&repo, &path)?;
    let git_rel = git_relative_path(&repo, &rel)?;
    let rel_text = git_rel.to_string_lossy().replace('\\', "/");
    let output = git(&repo.git_root, &["log", "-20", "--date=format:%Y-%m-%d %H:%M:%S", "--format=%h%x1f%ad%x1f%an%x1f%s", "--", &rel_text])?;
    if !output.status.success() { return Err("ファイル履歴を取得できませんでした".into()); }
    Ok(String::from_utf8_lossy(&output.stdout).lines().map(|line| line.replace('\u{1f}', "  ·  ")).collect())
}

fn field_map(value: &Value) -> std::collections::HashMap<String, Value> {
    let fields = value.get("fields").and_then(Value::as_array).or_else(|| value.as_array());
    fields.into_iter().flatten().filter_map(|field| {
        let id = field.get("id")?.as_str()?.to_string();
        Some((id, field.clone()))
    }).collect()
}

fn field_changes(label: &str, before_fields: &std::collections::HashMap<String, Value>, after_fields: &std::collections::HashMap<String, Value>) -> Vec<String> {
    let mut events = Vec::new();
    for (id, after) in after_fields {
        let before = before_fields.get(id);
        if before != Some(after) {
            let name = after.get("name").and_then(Value::as_str).unwrap_or("(name unavailable)");
            events.push(format!("{label}\n{id} · {name}\n{} → {}", before.map(|v| v.to_string()).unwrap_or_else(|| "(new field)".into()), after));
        }
    }
    for (id, before) in before_fields {
        if !after_fields.contains_key(id) { events.push(format!("{label}\n{id} · removed\n{before} → (removed)")); }
    }
    events
}

#[tauri::command]
fn git_field_history(path: String, state: State<'_, RepoState>) -> Result<Vec<String>, String> {
    let repo = repository(&state)?;
    let rel = safe_document_relative(&repo, &path)?;
    let git_rel = git_relative_path(&repo, &rel)?;
    let rel_text = git_rel.to_string_lossy().replace('\\', "/");
    let log = git(&repo.git_root, &["log", "-30", "--date=format:%Y-%m-%d %H:%M:%S", "--format=%H%x1f%h%x1f%ad%x1f%an%x1f%s", "--", &rel_text])?;
    if !log.status.success() { return Err("フィールド履歴を取得できませんでした".into()); }
    let mut revisions: Vec<(String, String)> = String::from_utf8_lossy(&log.stdout).lines().filter_map(|line| {
        let mut fields = line.splitn(5, '\u{1f}');
        let hash = fields.next()?.to_string();
        let label = fields.collect::<Vec<_>>().join("  ·  ");
        Some((hash, label))
    }).collect();
    let mut previous = std::collections::HashMap::new();
    let mut events = Vec::new();
    for (hash, label) in revisions.drain(..).rev() {
        let spec = format!("{hash}:{rel_text}");
        let snapshot = git(&repo.git_root, &["show", &spec])?;
        if !snapshot.status.success() { continue; }
        let current: Value = match serde_json::from_slice(&snapshot.stdout) { Ok(v) => v, Err(_) => continue };
        let current_fields = field_map(&current);
        events.extend(field_changes(&label, &previous, &current_fields));
        previous = current_fields;
    }
    events.reverse();
    Ok(events)
}

fn main() {
    let builder = tauri::Builder::default();
    #[cfg(all(windows, not(debug_assertions)))]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
        use tauri::Manager;
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.unminimize();
            let _ = window.set_focus();
        }
    }));

    builder
        .plugin(tauri_plugin_dialog::init())
        .manage(RepoState(Mutex::new(None)))
        .invoke_handler(tauri::generate_handler![workspace::preview_workspace, workspace::create_workspace, workspace::sync_workspace, workspace::update_records, workspace::mark_updates_read, workspace::locate_update, open_repository, project_context, close_repository, preview_document_setup, create_document_setup, list_documents, list_document_directories, create_document, create_document_folder, move_document_entry, delete_document_entry, search_documents, read_document, save_document, validate_document, check_image_references, import_image, git_status, git_diff, git_commit, git_branches, git_remotes, git_create_branch, git_switch_branch, git_remote_change, git_set_upstream, git_fetch, git_push_target, git_push_preview, git_pull_ff, git_history, git_field_history])
        .run(tauri::generate_context!())
        .expect("DesignBook Studio could not start");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn full_text_search_returns_each_occurrence_with_unicode_line_columns_and_filename_fallback() {
        let (root, repo) = test_repo();
        fs::create_dir_all(repo.documents.join("deep/folder")).unwrap();
        let source = "# 概要\r\n😀 修炼属性 与 修炼属性\r\n还有修炼属性\n";
        fs::write(repo.documents.join("deep/folder/a.md"), source).unwrap();
        fs::write(repo.documents.join("schema.json"), "{\"description\":\"修炼属性\"}").unwrap();
        fs::write(repo.documents.join("修炼属性.md"), "filename match only").unwrap();
        fs::write(root.join("game.md"), "修炼属性 outside").unwrap();
        let results = search_documents_impl(&repo, "修炼属性", 1000).unwrap();
        assert_eq!(results.matches.len(), 5);
        let content: Vec<_> = results.matches.iter().filter(|hit| hit.path.ends_with("a.md")).collect();
        assert_eq!(content.len(), 3);
        assert_eq!((content[0].line, content[0].column, content[0].length), (2, 4, 4));
        assert_eq!((content[1].line, content[1].column), (2, 11));
        assert_eq!((content[2].line, content[2].column), (3, 3));
        assert!(results.matches.iter().any(|hit| hit.kind == "json" && hit.line == 1));
        assert!(results.matches.iter().any(|hit| hit.line == 0 && hit.path.ends_with("修炼属性.md")));
        assert!(results.matches.iter().all(|hit| hit.path.starts_with("DesignDocs/")));
        assert_eq!(fs::read_to_string(repo.documents.join("deep/folder/a.md")).unwrap(), source);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn full_text_search_is_literal_case_insensitive_and_reports_limits_and_skips() {
        let (root, repo) = test_repo();
        fs::write(repo.documents.join("a.md"), "A.B a.b\nA.B").unwrap();
        let results = search_documents_impl(&repo, "a.b", 2).unwrap();
        assert_eq!(results.matches.len(), 2);
        assert!(results.truncated);
        assert_eq!(results.matches[1].column, 5);
        fs::write(repo.documents.join("a.md"), "{} [x] axb").unwrap();
        assert!(search_documents_impl(&repo, "a.b", 100).unwrap().matches.is_empty());
        assert_eq!(search_documents_impl(&repo, "[x]", 100).unwrap().matches.len(), 1);
        fs::write(repo.documents.join("bad.md"), [0xff, 0xfe]).unwrap();
        fs::write(repo.documents.join("big.md"), vec![b'x'; 5_000_001]).unwrap();
        let results = search_documents_impl(&repo, "unknown", 100).unwrap();
        assert_eq!(results.skipped, vec!["DesignDocs/bad.md", "DesignDocs/big.md"]);
        assert!(search_documents_impl(&repo, " ", 100).unwrap().matches.is_empty());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn full_text_search_excerpt_contains_late_match_on_long_lines() {
        let (root, repo) = test_repo();
        fs::write(repo.documents.join("a.md"), format!("{}修炼属性", "前".repeat(400))).unwrap();
        let results = search_documents_impl(&repo, "修炼属性", 100).unwrap();
        let hit = &results.matches[0];
        assert_eq!(hit.column, 401);
        assert_eq!(hit.excerpt_offset, 45);
        assert!(hit.excerpt.ends_with("修炼属性"));
        assert!(search_documents_impl(&repo, &"a".repeat(201), 100).is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn tree_move_and_rename_preserve_content_and_reject_collisions_and_escape() {
        let (root, repo) = test_repo();
        fs::create_dir_all(repo.documents.join("A/sub")).unwrap();
        fs::create_dir_all(repo.documents.join("B")).unwrap();
        fs::write(repo.documents.join("A/a.md"), "# original\n").unwrap();
        fs::write(repo.documents.join("B/a.md"), "existing destination").unwrap();
        assert!(move_entry_impl(&repo, "DesignDocs/A/a.md", "DesignDocs/B", "a.md").is_err());
        assert_eq!(fs::read_to_string(repo.documents.join("B/a.md")).unwrap(), "existing destination");
        assert_eq!(move_entry_impl(&repo, "DesignDocs/A/a.md", "DesignDocs/B", "renamed.md").unwrap(), "DesignDocs/B/renamed.md");
        assert_eq!(fs::read_to_string(repo.documents.join("B/renamed.md")).unwrap(), "# original\n");
        assert!(move_entry_impl(&repo, "DesignDocs/A", "DesignDocs/A/sub", "A").is_err());
        assert!(move_entry_impl(&repo, "DesignDocs", "DesignDocs/B", "root").is_err());
        assert!(move_entry_impl(&repo, "DesignDocs/B/renamed.md", "DesignDocs", "../escape.md").is_err());
        assert!(move_entry_impl(&repo, "DesignDocs/B/renamed.md", "outside", "escape.md").is_err());
        assert!(move_entry_impl(&repo, "DesignDocs/B/renamed.md", "DesignDocs", ".git").is_err());
        fs::write(repo.documents.join("A/sub/child.json"), "{}").unwrap();
        assert_eq!(move_entry_impl(&repo, "DesignDocs/A", "DesignDocs/B", "moved").unwrap(), "DesignDocs/B/moved");
        assert_eq!(fs::read_to_string(repo.documents.join("B/moved/sub/child.json")).unwrap(), "{}");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn tree_delete_requires_confirmation_and_only_removes_selected_subtree() {
        let (root, repo) = test_repo();
        fs::create_dir_all(repo.documents.join("A/sub")).unwrap();
        fs::write(repo.documents.join("A/sub/a.md"), "remove").unwrap();
        fs::write(repo.documents.join("keep.md"), "keep").unwrap();
        fs::write(root.join("game.asset"), "game").unwrap();
        assert!(delete_entry_impl(&repo, "DesignDocs/A", false).is_err());
        assert!(repo.documents.join("A/sub/a.md").exists());
        assert!(delete_entry_impl(&repo, "DesignDocs", true).is_err());
        assert!(delete_entry_impl(&repo, "DesignDocs/../game.asset", true).is_err());
        delete_entry_impl(&repo, "DesignDocs/A", true).unwrap();
        assert!(!repo.documents.join("A").exists());
        assert_eq!(fs::read_to_string(repo.documents.join("keep.md")).unwrap(), "keep");
        assert_eq!(fs::read_to_string(root.join("game.asset")).unwrap(), "game");
        delete_entry_impl(&repo, "DesignDocs/keep.md", true).unwrap();
        assert!(!repo.documents.join("keep.md").exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn tree_mutations_reject_nested_git_metadata_before_changing_anything() {
        let (root, repo) = test_repo();
        fs::create_dir_all(repo.documents.join("nested/.git")).unwrap();
        fs::write(repo.documents.join("nested/page.md"), "keep").unwrap();
        assert!(delete_entry_impl(&repo, "DesignDocs/nested", true).is_err());
        assert!(move_entry_impl(&repo, "DesignDocs/nested", "DesignDocs", "renamed").is_err());
        assert!(create_folder_impl(&repo, "DesignDocs", ".git").is_err());
        assert_eq!(fs::read_to_string(repo.documents.join("nested/page.md")).unwrap(), "keep");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn tree_mutations_reject_symlink_ancestors_and_subtrees() {
        let (root, repo) = test_repo();
        fs::create_dir_all(root.join("outside")).unwrap();
        fs::write(root.join("outside/keep.md"), "outside").unwrap();
        fs::create_dir_all(repo.documents.join("A")).unwrap();
        let link = repo.documents.join("A/link");
        #[cfg(windows)] std::os::windows::fs::symlink_dir(root.join("outside"), &link).unwrap();
        #[cfg(unix)] std::os::unix::fs::symlink(root.join("outside"), &link).unwrap();
        assert!(delete_entry_impl(&repo, "DesignDocs/A", true).is_err());
        assert!(delete_entry_impl(&repo, "DesignDocs/A/link/keep.md", true).is_err());
        assert!(move_entry_impl(&repo, "DesignDocs/A", "DesignDocs", "renamed").is_err());
        assert!(create_document_impl(&repo, "DesignDocs/A/link", "new", "markdown").is_err());
        assert_eq!(fs::read_to_string(root.join("outside/keep.md")).unwrap(), "outside");
        #[cfg(windows)] fs::remove_dir(link).unwrap();
        #[cfg(unix)] fs::remove_file(link).unwrap();
        let _ = fs::remove_dir_all(root);
    }

    fn temp_dir(label: &str) -> PathBuf {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("designbook-{label}-{}-{stamp}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn test_repo() -> (PathBuf, Repository) {
        let root = temp_dir("schema").canonicalize().unwrap();
        let documents = root.join("DesignDocs");
        fs::create_dir_all(&documents).unwrap();
        (root.clone(), Repository { source_root: None, root: root.clone(), git_root: root.clone(), documents, documents_relative: "DesignDocs".into(), git_documents_relative: "DesignDocs".into(), has_documents: true })
    }

    fn empty_unity_repo(label: &str) -> (PathBuf, Repository) {
        let root = temp_dir(label);
        fs::create_dir_all(root.join("Assets")).unwrap();
        fs::create_dir_all(root.join("ProjectSettings")).unwrap();
        fs::write(root.join("ProjectSettings/ProjectVersion.txt"), "m_EditorVersion: 6000.0.0f1\n").unwrap();
        run_git(&root, &["init"]);
        let repo = build_repository(root.canonicalize().unwrap()).unwrap();
        (root, repo)
    }

    #[test]
    fn valid_unity_project_without_design_docs_opens_without_creating_files() {
        let (root, repo) = empty_unity_repo("unity-no-design-docs");
        assert!(!repo.has_documents);
        assert_eq!(document_files(&repo).unwrap().len(), 0);
        assert!(!root.join("DesignDocs").exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn refusing_initialization_does_not_write_anything() {
        let (root, repo) = empty_unity_repo("setup-refused");
        assert!(initialize_setup_after_confirmation(&repo, false).is_err());
        assert!(!root.join("DesignDocs").exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn setup_preview_lists_only_required_directories_and_one_readme_without_writes() {
        let (root, repo) = empty_unity_repo("setup-preview");
        let preview = preview_setup_impl(&repo).unwrap();
        assert_eq!(preview.entries.iter().filter(|entry| entry.kind == "directory").count(), 9);
        assert_eq!(preview.entries.iter().filter(|entry| entry.kind == "file").map(|entry| entry.path.as_str()).collect::<Vec<_>>(), vec!["DesignDocs/README.md"]);
        assert!(!root.join("DesignDocs").exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn initialization_stops_if_git_status_changes_after_preview() {
        let (root, repo) = empty_unity_repo("setup-stale-preview");
        let preview = preview_setup_impl(&repo).unwrap();
        fs::write(root.join("Assets/added-after-preview.asset"), "keep").unwrap();
        assert!(validate_setup_git_snapshot(&repo, &preview.changed_files).is_err());
        assert!(!root.join("DesignDocs").exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn confirmed_setup_creates_only_preview_items_and_preserves_unrelated_files() {
        let (root, repo) = empty_unity_repo("setup-created");
        let unrelated = root.join("Assets/existing.asset");
        fs::write(&unrelated, "untouched").unwrap();
        let preview = preview_setup_impl(&repo).unwrap();
        initialize_setup_after_confirmation(&repo, true).unwrap();
        for entry in &preview.entries {
            let path = root.join(&entry.path);
            if entry.kind == "directory" { assert!(path.is_dir(), "missing {}", entry.path); }
            else { assert!(path.is_file(), "missing {}", entry.path); }
        }
        assert_eq!(fs::read_to_string(unrelated).unwrap(), "untouched");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn existing_design_docs_file_is_never_overwritten() {
        let (root, repo) = empty_unity_repo("setup-conflict");
        let target = root.join("DesignDocs");
        fs::write(&target, "user data").unwrap();
        assert!(preview_setup_impl(&repo).is_err());
        assert_eq!(fs::read_to_string(&target).unwrap(), "user data");
        assert!(initialize_setup_impl(&repo).is_err());
        assert_eq!(fs::read_to_string(&target).unwrap(), "user data");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn existing_readme_is_never_overwritten_by_setup() {
        let (root, repo) = empty_unity_repo("setup-existing-readme");
        let design_docs = root.join("DesignDocs");
        fs::create_dir(&design_docs).unwrap();
        let readme = design_docs.join("README.md");
        fs::write(&readme, "existing user README\n").unwrap();
        assert!(initialize_setup_impl(&repo).is_err());
        assert_eq!(fs::read_to_string(readme).unwrap(), "existing user README\n");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn design_docs_symlink_outside_unity_root_is_rejected() {
        let (root, _repo) = empty_unity_repo("setup-symlink");
        let outside = temp_dir("setup-symlink-target");
        let link = root.join("DesignDocs");
        if std::os::windows::fs::symlink_dir(&outside, &link).is_err() {
            let _ = fs::remove_dir_all(root);
            let _ = fs::remove_dir_all(outside);
            return;
        }
        assert!(build_repository(root.canonicalize().unwrap()).is_err());
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(outside);
    }

    #[test]
    fn setup_path_traversal_is_rejected() {
        assert!(safe_relative("DesignDocs/../Assets/overwrite.asset").is_err());
        assert!(safe_relative("DesignDocs\\..\\Assets\\overwrite.asset").is_err());
    }

    #[test]
    fn opening_unity_subdirectory_resolves_parent_git_root_and_scopes_docs() {
        let git_root = temp_dir("parent-git-root");
        let unity = git_root.join("Projects/Xiuxian");
        fs::create_dir_all(unity.join("Assets")).unwrap();
        fs::create_dir_all(unity.join("ProjectSettings")).unwrap();
        fs::write(unity.join("ProjectSettings/ProjectVersion.txt"), "m_EditorVersion: 6000.0.0f1\n").unwrap();
        fs::create_dir_all(unity.join("DesignDocs")).unwrap();
        run_git(&git_root, &["init"]);
        let repo = build_repository(unity.canonicalize().unwrap()).unwrap();
        assert_eq!(repo.root, unity.canonicalize().unwrap());
        assert_eq!(repo.git_root, git_root.canonicalize().unwrap());
        assert_eq!(repo.documents_relative, "DesignDocs");
        assert_eq!(repo.git_documents_relative, "Projects/Xiuxian/DesignDocs");
        assert!(safe_document_relative(&repo, "DesignDocs/page.md").is_ok());
        assert!(safe_document_relative(&repo, "Assets/game.asset").is_err());
        assert_eq!(git_relative_path(&repo, Path::new("DesignDocs/page.md")).unwrap(), PathBuf::from("Projects/Xiuxian/DesignDocs/page.md"));
        assert!(validate_unity_root(&git_root).is_err());
        let _ = fs::remove_dir_all(git_root);
    }

    fn run_git(root: &Path, args: &[&str]) {
        let output = git_command(root, args).output().unwrap();
        assert!(output.status.success(), "git {:?} failed: {}", args, String::from_utf8_lossy(&output.stderr));
    }

    fn setup_git_remote(label: &str) -> (PathBuf, PathBuf, PathBuf) {
        let base = temp_dir(label);
        let bare = base.join("remote.git");
        let local = base.join("local");
        let writer = base.join("writer");
        fs::create_dir_all(&local).unwrap();
        run_git(&base, &["init", "--bare", "--initial-branch=main", bare.to_str().unwrap()]);
        run_git(&local, &["init", "--initial-branch=main"]);
        run_git(&local, &["config", "core.autocrlf", "false"]);
        run_git(&local, &["config", "user.name", "DesignBook Test"]);
        run_git(&local, &["config", "user.email", "designbook@example.invalid"]);
        fs::create_dir_all(local.join("DesignDocs")).unwrap();
        fs::write(local.join("DesignDocs/page.md"), "initial\n").unwrap();
        fs::write(local.join("game.txt"), "game\n").unwrap();
        run_git(&local, &["add", "."]);
        run_git(&local, &["commit", "-m", "initial"]);
        run_git(&local, &["remote", "add", "origin", bare.to_str().unwrap()]);
        run_git(&local, &["push", "-u", "origin", "main"]);
        run_git(&base, &["-c", "core.autocrlf=false", "clone", bare.to_str().unwrap(), writer.to_str().unwrap()]);
        run_git(&writer, &["config", "user.name", "Remote Writer"]);
        run_git(&writer, &["config", "user.email", "writer@example.invalid"]);
        (base, local, writer)
    }

    fn remote_commit(writer: &Path, value: &str, message: &str) {
        fs::write(writer.join("DesignDocs/page.md"), value).unwrap();
        run_git(writer, &["add", "DesignDocs/page.md"]);
        run_git(writer, &["commit", "-m", message]);
        run_git(writer, &["push", "origin", "main"]);
    }

    #[test]
    fn schema_errors_include_instance_location_and_reason() {
        let (root, repo) = test_repo();
        fs::write(repo.documents.join("params.schema.json"), r#"{"type":"object","properties":{"count":{"type":"integer","minimum":2}},"required":["count"]}"#).unwrap();
        let errors = validate_document_content(&repo, "DesignDocs/params.json", r#"{"count":1}"#);
        assert!(!errors.is_empty());
        assert!(errors[0].contains("/count"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn duplicate_field_ids_are_rejected() {
        let data = r#"{"fields":[{"id":"X","name":"one","type":"string"},{"id":"X","name":"two","type":"string"}]}"#;
        assert!(validate_json(data).unwrap_err().contains("重複"));
    }

    #[test]
    fn paths_cannot_escape_selected_document_directory() {
        let (_root, repo) = test_repo();
        assert!(safe_document_relative(&repo, "DesignDocs/../Assets/config.json").is_err());
        assert!(safe_document_relative(&repo, "Packages/manifest.json").is_err());
    }

    #[test]
    fn document_scan_includes_deep_and_named_directories_without_symlink_traversal() {
        let (root, repo) = test_repo();
        let mut nested = repo.documents.clone();
        for part in ["Library", "Temp", "Logs", "obj", "bin", "target", ".draft", "level-8", "level-9", "level-10"] {
            nested.push(part);
        }
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("deep.MD"), "# Deep document\n").unwrap();
        fs::write(repo.documents.join("README.md"), "# Root\n").unwrap();
        let tree = scan_document_tree(&repo).unwrap();
        assert!(tree.files.iter().any(|file| file.path.ends_with("deep.MD")));
        assert!(tree.files.iter().any(|file| file.path == "DesignDocs/README.md"));
        assert!(tree.directories.iter().any(|path| path.ends_with("level-10")));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn new_documents_and_folders_are_scoped_and_never_overwrite() {
        let (root, repo) = test_repo();
        fs::create_dir_all(repo.documents.join("nested")).unwrap();
        assert_eq!(create_document_impl(&repo, "DesignDocs/nested", "概要", "markdown").unwrap(), "DesignDocs/nested/概要.md");
        assert_eq!(fs::read_to_string(repo.documents.join("nested/概要.md")).unwrap(), "# 概要\n");
        assert!(create_document_impl(&repo, "DesignDocs/nested", "概要", "markdown").unwrap_err().contains("同名"));
        assert_eq!(fs::read_to_string(repo.documents.join("nested/概要.md")).unwrap(), "# 概要\n");
        assert!(create_document_impl(&repo, "DesignDocs", "../escape", "markdown").is_err());
        assert!(create_document_impl(&repo, "DesignDocs/../Assets", "escape", "markdown").is_err());
        assert_eq!(create_folder_impl(&repo, "DesignDocs", "新規フォルダー").unwrap(), "DesignDocs/新規フォルダー");
        assert!(create_folder_impl(&repo, "DesignDocs", "新規フォルダー").unwrap_err().contains("同名"));
        let tree = scan_document_tree(&repo).unwrap();
        assert!(tree.files.iter().any(|file| file.path == "DesignDocs/nested/概要.md"));
        assert!(tree.directories.iter().any(|path| path == "DesignDocs/新規フォルダー"));
        assert!(!root.join("Assets/escape.md").exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn failed_atomic_replace_keeps_existing_path_untouched() {
        let root = temp_dir("atomic");
        let destination = root.join("existing-directory");
        fs::create_dir_all(&destination).unwrap();
        fs::write(destination.join("sentinel.txt"), "keep me").unwrap();
        assert!(atomic_write(&destination, b"replacement").is_err());
        assert_eq!(fs::read_to_string(destination.join("sentinel.txt")).unwrap(), "keep me");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn field_history_tracks_rename_by_stable_id_and_records_first_commit() {
        let before = std::collections::HashMap::new();
        let initial = field_map(&serde_json::from_str(r#"{"fields":[{"id":"FLD-1","name":"old","type":"string"}]}"#).unwrap());
        let first = field_changes("initial commit", &before, &initial);
        assert_eq!(first.len(), 1);
        let renamed = field_map(&serde_json::from_str(r#"{"fields":[{"id":"FLD-1","name":"new","type":"string"}]}"#).unwrap());
        let update = field_changes("rename commit", &initial, &renamed);
        assert_eq!(update.len(), 1);
        assert!(update[0].contains("FLD-1 · new"));
        assert!(update[0].contains("old"));
        assert!(field_changes("delete commit", &renamed, &before).iter().any(|event| event.contains("removed")));
    }

    #[test]
    fn image_import_inserts_without_overwriting_and_replacement_keeps_path() {
        let (root, repo) = test_repo();
        let source = root.join("new.png");
        fs::write(&source, b"new-image").unwrap();
        fs::create_dir_all(repo.documents.join("images")).unwrap();
        fs::write(repo.documents.join("images/new.png"), b"original-image").unwrap();
        let inserted = import_image_impl(&repo, "DesignDocs/page.md", source.to_str().unwrap(), None).unwrap();
        assert!(inserted.path.ends_with("images/new-2.png"));
        assert_eq!(fs::read(repo.documents.join("images/new.png")).unwrap().as_slice(), b"original-image");
        let replaced = import_image_impl(&repo, "DesignDocs/page.md", source.to_str().unwrap(), Some("images/new.png")).unwrap();
        assert!(replaced.path.ends_with("images/new.png"));
        assert_eq!(fs::read(repo.documents.join("images/new.png")).unwrap().as_slice(), b"new-image");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn image_reference_check_reports_broken_relative_links() {
        let (root, repo) = test_repo();
        fs::write(repo.documents.join("existing.png"), b"image").unwrap();
        let missing = check_image_references_impl(&repo, "DesignDocs/page.md", vec!["existing.png".into(), "missing.png".into(), "../../Packages/manifest.json".into()]).unwrap();
        assert_eq!(missing, vec!["missing.png", "../../Packages/manifest.json"]);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn git_errors_do_not_return_raw_command_output() {
        let root = temp_dir("git-error-redaction");
        let error = git_text(&root, &["--codex-secret-canary"]).unwrap_err();
        assert!(!error.contains("canary"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn sync_refuses_dirty_worktree_without_changing_local_file() {
        let (base, local, writer) = setup_git_remote("dirty-sync");
        fs::write(local.join("DesignDocs/page.md"), "local unsaved\n").unwrap();
        remote_commit(&writer, "remote update\n", "remote update");
        assert!(git_sync_impl(&local).is_err());
        assert_eq!(fs::read_to_string(local.join("DesignDocs/page.md")).unwrap(), "local unsaved\n");
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn sync_fast_forwards_clean_worktree() {
        let (base, local, writer) = setup_git_remote("ff-sync");
        remote_commit(&writer, "remote fast-forward\n", "fast-forward");
        assert!(git_sync_impl(&local).unwrap().contains("Fast-forward"));
        assert_eq!(fs::read_to_string(local.join("DesignDocs/page.md")).unwrap(), "remote fast-forward\n");
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn sync_stops_on_divergence_without_overwriting_local_commit() {
        let (base, local, writer) = setup_git_remote("diverged-sync");
        fs::write(local.join("DesignDocs/page.md"), "local commit\n").unwrap();
        run_git(&local, &["add", "DesignDocs/page.md"]);
        run_git(&local, &["commit", "-m", "local commit"]);
        remote_commit(&writer, "remote commit\n", "remote commit");
        assert!(git_sync_impl(&local).unwrap_err().contains("分岐"));
        assert_eq!(fs::read_to_string(local.join("DesignDocs/page.md")).unwrap(), "local commit\n");
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn push_refuses_remote_updates_after_fetch() {
        let (base, local, writer) = setup_git_remote("push-stale");
        fs::write(local.join("DesignDocs/page.md"), "local commit\n").unwrap();
        run_git(&local, &["add", "DesignDocs/page.md"]);
        run_git(&local, &["commit", "-m", "local commit"]);
        remote_commit(&writer, "remote commit\n", "remote commit");
        assert!(git_push_impl(&local).unwrap_err().contains("behind 1"));
        assert_eq!(git_text(&local, &["rev-parse", "origin/main"]).unwrap(), git_text(&writer, &["rev-parse", "HEAD"]).unwrap());
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn save_only_writes_file_and_does_not_create_a_commit() {
        let (base, local, _writer) = setup_git_remote("save-no-commit");
        let canonical_local = local.canonicalize().unwrap();
        let repo = Repository { source_root: None, root: canonical_local.clone(), git_root: canonical_local.clone(), documents: canonical_local.join("DesignDocs").canonicalize().unwrap(), documents_relative: "DesignDocs".into(), git_documents_relative: "DesignDocs".into(), has_documents: true };
        let head_before = git_text(&local, &["rev-parse", "HEAD"]).unwrap();
        save_document_impl(&repo, "DesignDocs/page.md", "saved only\n").unwrap();
        assert_eq!(fs::read_to_string(local.join("DesignDocs/page.md")).unwrap(), "saved only\n");
        assert_eq!(git_text(&local, &["rev-parse", "HEAD"]).unwrap(), head_before);
        let status = git_status_impl(&local).unwrap();
        assert!(!status.clean);
        assert!(status.changed.iter().any(|path| path == "DesignDocs/page.md"));
        assert_eq!(status.ahead, 0);
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn push_with_dirty_worktree_still_stops_when_there_are_no_commits_to_send() {
        let (base, local, _writer) = setup_git_remote("push-dirty");
        fs::write(local.join("DesignDocs/page.md"), "unsaved local change\n").unwrap();
        let head_before = git_text(&local, &["rev-parse", "HEAD"]).unwrap();
        let error = git_push_impl(&local).unwrap_err();
        assert!(error.contains("main"));
        assert!(error.contains("Push する既存コミットがありません"));
        assert!(error.contains("ahead 0 / behind 0"));
        assert_eq!(git_text(&local, &["rev-parse", "HEAD"]).unwrap(), head_before);
        assert_eq!(fs::read_to_string(local.join("DesignDocs/page.md")).unwrap(), "unsaved local change\n");
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn upstream_detection_reads_main_tracking_origin_main_and_ahead_count() {
        let (base, local, _writer) = setup_git_remote("upstream-detection");
        let status = git_status_impl(&local).unwrap();
        assert_eq!(status.branch, "main");
        assert!(status.has_upstream);
        assert_eq!((status.ahead, status.behind), (0, 0));

        fs::write(local.join("DesignDocs/page.md"), "local pending commit\n").unwrap();
        run_git(&local, &["add", "DesignDocs/page.md"]);
        run_git(&local, &["commit", "-m", "local ahead commit"]);
        let status = git_status_impl(&local).unwrap();
        assert!(status.has_upstream);
        assert_eq!((status.ahead, status.behind), (1, 0));
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn upstream_detection_reports_missing_tracking_branch() {
        let (base, local, _writer) = setup_git_remote("no-upstream-detection");
        run_git(&local, &["branch", "--unset-upstream"]);
        let status = git_status_impl(&local).unwrap();
        assert_eq!(status.branch, "main");
        assert!(!status.has_upstream);
        assert_eq!((status.ahead, status.behind), (0, 0));
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn push_preflight_recognizes_ahead_commit_without_pushing() {
        let (base, local, _writer) = setup_git_remote("push-ahead-preflight");
        fs::write(local.join("DesignDocs/page.md"), "local ahead commit\n").unwrap();
        run_git(&local, &["add", "DesignDocs/page.md"]);
        run_git(&local, &["commit", "-m", "local ahead commit"]);
        let status = git_status_impl(&local).unwrap();
        assert!(status.has_upstream);
        assert_eq!(status.ahead, 1);
        assert!(status.clean);
        let ready = validate_push_preflight(&status).unwrap();
        assert!(ready.contains("ahead 1 / behind 0"));
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn branch_list_uses_git_refs_and_branch_switch_refuses_dirty_worktree() {
        let (base, local, _writer) = setup_git_remote("branch-management");
        let branches = git_branches_impl(&local).unwrap();
        assert!(branches.iter().any(|branch| !branch.remote && branch.name == "main" && branch.current && branch.upstream.as_deref() == Some("origin/main")));
        assert!(branches.iter().any(|branch| branch.remote && branch.name == "origin/main"));
        git_create_branch_impl(&local, "feature/ui", "origin/main").unwrap();
        assert!(git_branches_impl(&local).unwrap().iter().any(|branch| !branch.remote && branch.name == "feature/ui"));
        fs::write(local.join("DesignDocs/page.md"), "dirty before switch\n").unwrap();
        assert!(git_switch_branch_impl(&local, "feature/ui").unwrap_err().contains("未コミット変更"));
        assert_eq!(git_text(&local, &["branch", "--show-current"]).unwrap(), "main");
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn remote_management_lists_masks_credentials_and_supports_add_update_remove() {
        let (base, local, _writer) = setup_git_remote("remote-management");
        git_remote_change_impl(&local, "add", "private", Some("https://alice:secret-token@example.invalid/repo.git")).unwrap();
        let remotes = git_remotes_impl(&local).unwrap();
        let private = remotes.iter().find(|remote| remote.name == "private").unwrap();
        assert_eq!(private.url, "https://***@example.invalid/repo.git");
        assert!(!private.url.contains("secret-token"));
        git_remote_change_impl(&local, "set-url", "private", Some("https://bob:another-secret@example.invalid/renamed.git")).unwrap();
        assert_eq!(git_remotes_impl(&local).unwrap().iter().find(|remote| remote.name == "private").unwrap().url, "https://***@example.invalid/renamed.git");
        git_remote_change_impl(&local, "remove", "private", None).unwrap();
        assert!(!git_remotes_impl(&local).unwrap().iter().any(|remote| remote.name == "private"));
        assert!(git_remote_change_impl(&local, "remove", "missing", None).is_err());
        git_remote_change_impl(&local, "remove", "origin", None).unwrap();
        assert!(!git_status_impl(&local).unwrap().has_upstream);
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn upstream_requires_an_existing_remote_branch_and_can_be_set() {
        let (base, local, _writer) = setup_git_remote("set-upstream");
        git_create_branch_impl(&local, "feature/upstream", "main").unwrap();
        run_git(&local, &["switch", "feature/upstream"]);
        assert!(git_set_upstream_impl(&local, "origin", "not-created").unwrap_err().contains("初回 Push"));
        git_set_upstream_impl(&local, "origin", "main").unwrap();
        let status = git_status_impl(&local).unwrap();
        assert!(status.has_upstream);
        assert_eq!(git_branches_impl(&local).unwrap().iter().find(|branch| branch.current).unwrap().upstream.as_deref(), Some("origin/main"));
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn explicit_push_validates_target_and_can_create_branch_and_upstream() {
        let (base, local, _writer) = setup_git_remote("push-target");
        fs::write(local.join("DesignDocs/page.md"), "new local commit\n").unwrap();
        run_git(&local, &["add", "DesignDocs/page.md"]);
        run_git(&local, &["commit", "-m", "push target commit"]);
        let result = git_push_target_impl(&local, "origin", "feature/publish", true).unwrap();
        assert!(!result.trim().is_empty());
        assert!(!result.contains("secret"));
        assert_eq!(git_text(&local, &["rev-parse", "refs/remotes/origin/feature/publish"]).unwrap(), git_text(&local, &["rev-parse", "HEAD"]).unwrap());
        assert_eq!(git_text(&local, &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{upstream}"]).unwrap(), "origin/feature/publish");
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn explicit_push_rejects_remote_ahead_even_with_dirty_changes() {
        let (base, local, writer) = setup_git_remote("push-protection");
        remote_commit(&writer, "remote ahead\n", "remote ahead");
        let error = git_push_target_impl(&local, "origin", "main", false).unwrap_err();
        assert!(error.contains("Fetch") || error.contains("ahead"));
        assert_eq!(fs::read_to_string(local.join("DesignDocs/page.md")).unwrap(), "initial\n");
        fs::write(local.join("DesignDocs/page.md"), "dirty\n").unwrap();
        let error = git_push_target_impl(&local, "origin", "main", false).unwrap_err();
        assert!(error.contains("Fetch") || error.contains("ahead"));
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn push_sends_only_commits_and_preserves_staged_unstaged_and_untracked_content() {
        let (base, local, _writer) = setup_git_remote("push-dirty-preservation");
        fs::write(local.join("DesignDocs/page.md"), "committed design\n").unwrap();
        run_git(&local, &["add", "DesignDocs/page.md"]);
        run_git(&local, &["commit", "-m", "design changes"]);
        let head = git_text(&local, &["rev-parse", "HEAD"]).unwrap();
        fs::write(local.join("DesignDocs/page.md"), "uncommitted design draft\n").unwrap();
        fs::write(local.join("game.txt"), "staged game edit\n").unwrap();
        run_git(&local, &["add", "game.txt"]);
        fs::write(local.join("game.txt"), "unstaged game edit\n").unwrap();
        fs::write(local.join("untracked.asset"), "untracked game asset\n").unwrap();
        let before_status = git(&local, &["status", "--porcelain=v1", "-z", "--untracked-files=all"]).unwrap().stdout;
        let before_index = git(&local, &["diff", "--cached", "--binary"]).unwrap().stdout;
        let preview = git_push_preview_impl(&local, "origin", "main").unwrap();
        assert!(preview.ready);
        assert_eq!(preview.ahead, Some(1));
        assert_eq!(preview.changed_count, 3);
        git_push_target_impl(&local, "origin", "main", false).unwrap();
        assert_eq!(git_text(&local, &["rev-parse", "HEAD"]).unwrap(), head);
        assert_eq!(git_text(&local, &["rev-parse", "refs/remotes/origin/main"]).unwrap(), head);
        let bare_path = git_text(&local, &["remote", "get-url", "origin"]).unwrap();
        let bare = Path::new(&bare_path);
        assert_eq!(git_text(bare, &["show", "main:DesignDocs/page.md"]).unwrap(), "committed design");
        assert_eq!(git_text(bare, &["show", "main:game.txt"]).unwrap(), "game");
        assert!(!git(bare, &["cat-file", "-e", "main:untracked.asset"]).unwrap().status.success());
        assert_eq!(git(&local, &["status", "--porcelain=v1", "-z", "--untracked-files=all"]).unwrap().stdout, before_status);
        assert_eq!(git(&local, &["diff", "--cached", "--binary"]).unwrap().stdout, before_index);
        assert_eq!(fs::read_to_string(local.join("DesignDocs/page.md")).unwrap(), "uncommitted design draft\n");
        assert_eq!(fs::read_to_string(local.join("game.txt")).unwrap(), "unstaged game edit\n");
        assert_eq!(fs::read_to_string(local.join("untracked.asset")).unwrap(), "untracked game asset\n");
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn push_preview_reports_target_counts_and_blocks_diverged_history() {
        let (base, local, writer) = setup_git_remote("push-preview-diverged");
        remote_commit(&writer, "remote preview commit\n", "remote preview");
        run_git(&local, &["fetch", "origin"]);
        fs::write(local.join("DesignDocs/page.md"), "local preview commit\n").unwrap();
        run_git(&local, &["add", "DesignDocs/page.md"]);
        run_git(&local, &["commit", "-m", "local preview"]);
        let preview = git_push_preview_impl(&local, "origin", "main").unwrap();
        assert_eq!((preview.ahead, preview.behind), (Some(1), Some(1)));
        assert!(preview.target_exists);
        assert!(!preview.ready);
        assert!(preview.message.contains("分岐"));
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn fetch_changes_refs_only_and_pull_refuses_dirty_worktree() {
        let (base, local, writer) = setup_git_remote("fetch-pull-safety");
        remote_commit(&writer, "fetched remote content\n", "remote update");
        git_fetch_impl(&local, Some("origin")).unwrap();
        assert_eq!(fs::read_to_string(local.join("DesignDocs/page.md")).unwrap(), "initial\n");
        git_pull_ff_impl(&local, "origin", "main").unwrap();
        assert_eq!(fs::read_to_string(local.join("DesignDocs/page.md")).unwrap(), "fetched remote content\n");
        fs::write(local.join("DesignDocs/page.md"), "dirty local edit\n").unwrap();
        assert!(git_pull_ff_impl(&local, "origin", "main").unwrap_err().contains("未コミット"));
        assert_eq!(fs::read_to_string(local.join("DesignDocs/page.md")).unwrap(), "dirty local edit\n");
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn credentials_are_redacted_from_git_operation_output() {
        assert_eq!(redact_url_credentials("fatal: https://name:token@example.invalid/repo.git denied"), "fatal: https://***@example.invalid/repo.git denied");
        assert_eq!(redact_url_credentials("https://example.invalid/repo?access_token=secret-value&custom-key=private&view=1"), "https://example.invalid/repo?access_token=***&custom-key=***&view=***");
    }

    #[test]
    fn commit_only_commits_document_scope_and_keeps_other_staged_changes() {
        let base = temp_dir("commit-scope");
        let local = base.join("repo");
        fs::create_dir_all(local.join("DesignDocs")).unwrap();
        run_git(&local, &["init", "--initial-branch=main"]);
        let local = local.canonicalize().unwrap();
        run_git(&local, &["config", "user.name", "DesignBook Test"]);
        run_git(&local, &["config", "user.email", "designbook@example.invalid"]);
        fs::write(local.join("DesignDocs/page.md"), "initial\n").unwrap();
        fs::write(local.join("game.txt"), "initial game\n").unwrap();
        run_git(&local, &["add", "."]);
        run_git(&local, &["commit", "-m", "initial"]);
        fs::write(local.join("game.txt"), "staged unrelated\n").unwrap();
        run_git(&local, &["add", "game.txt"]);
        fs::write(local.join("DesignDocs/page.md"), "documentation change\n").unwrap();
        let repo = Repository { source_root: None, root: local.clone(), git_root: local.clone(), documents: local.join("DesignDocs").canonicalize().unwrap(), documents_relative: "DesignDocs".into(), git_documents_relative: "DesignDocs".into(), has_documents: true };
        let result = git_commit_impl(&repo, "docs only").unwrap();
        assert!(result.contains(&git_text(&local, &["rev-parse", "HEAD"]).unwrap()));
        assert_eq!(git_text(&local, &["show", "--format=", "--name-only", "HEAD"]).unwrap(), "DesignDocs/page.md");
        assert_eq!(git_text(&local, &["diff", "--cached", "--name-only"]).unwrap(), "game.txt");
        // Read again independently, as a restarted application would.
        let status = scope_git_status(git_status_impl(&local).unwrap(), "DesignDocs");
        assert!(status.document_changed.is_empty());
        assert_eq!(status.other_changed, vec!["game.txt"]);
        assert!(!status.clean);
        assert!(git_commit_impl(&repo, "nothing to commit").unwrap_err().contains("変更がありません"));
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn porcelain_nul_handles_rename_unicode_newlines_and_deleted_files() {
        let changes = parse_git_changes("R  DesignDocs/新しい 名前.md\0DesignDocs/old.md\0 D DesignDocs/deleted.md\0?? Assets/untracked\nfile.asset\0".as_bytes()).unwrap();
        assert_eq!(changes.len(), 3);
        assert_eq!(changes[0].path, "DesignDocs/新しい 名前.md");
        assert_eq!(changes[0].previous_path.as_deref(), Some("DesignDocs/old.md"));
        assert_eq!(changes[1].status, " D");
        assert_eq!(changes[2].path, "Assets/untracked\nfile.asset");
        assert!(parse_git_changes(b"R  new\0").is_err());
    }

    #[test]
    fn scope_classification_checks_path_boundary_and_rename_source() {
        let changes = parse_git_changes(b"R  Assets/moved.md\0DesignDocs/old.md\0?? DesignDocs-other/file.md\0?? DesignDocs/new.md\0").unwrap();
        let status = GitState { branch: "main".into(), changed: changes.iter().map(|c| c.path.clone()).collect(), changes, document_changed: vec![], other_changed: vec![], ahead: 0, behind: 0, clean: false, has_upstream: false, tracking_error: None };
        let status = scope_git_status(status, "DesignDocs");
        assert_eq!(status.document_changed, vec!["Assets/moved.md", "DesignDocs/new.md"]);
        assert_eq!(status.other_changed, vec!["DesignDocs-other/file.md"]);
    }

    #[test]
    fn commit_failure_from_hook_keeps_head_and_returns_error() {
        let (base, local, _writer) = setup_git_remote("commit-failure");
        let local = local.canonicalize().unwrap();
        fs::write(local.join("DesignDocs/page.md"), "pending content\n").unwrap();
        let hooks = local.join("test-hooks");
        fs::create_dir_all(&hooks).unwrap();
        fs::write(hooks.join("pre-commit"), "#!/bin/sh\necho test-hook-rejected >&2\nexit 1\n").unwrap();
        #[cfg(unix)] {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(hooks.join("pre-commit"), fs::Permissions::from_mode(0o755)).unwrap();
        }
        run_git(&local, &["config", "core.hooksPath", "test-hooks"]);
        let before = git_text(&local, &["rev-parse", "HEAD"]).unwrap();
        let repo = Repository { source_root: None, root: local.clone(), git_root: local.clone(), documents: local.join("DesignDocs").canonicalize().unwrap(), documents_relative: "DesignDocs".into(), git_documents_relative: "DesignDocs".into(), has_documents: true };
        let error = git_commit_impl(&repo, "rejected").unwrap_err();
        assert!(error.contains("test-hook-rejected"), "{error}");
        assert_eq!(git_text(&local, &["rev-parse", "HEAD"]).unwrap(), before);
        assert_eq!(scope_git_status(git_status_impl(&local).unwrap(), "DesignDocs").document_changed.len(), 1);
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn push_preview_explains_unborn_branch_without_contacting_remote() {
        let base = temp_dir("push-no-commits");
        run_git(&base, &["init", "--initial-branch=main"]);
        run_git(&base, &["remote", "add", "local-test", "unused-local-remote"]);
        let preview = git_push_preview_impl(&base, "local-test", "first").unwrap();
        assert!(!preview.ready);
        assert_eq!(preview.ahead, Some(0));
        assert!(preview.message.contains("コミットがありません"));
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn first_push_without_upstream_refreshes_status_and_honors_tracking_choice() {
        for set_upstream in [false, true] {
            let (base, local, _writer) = setup_git_remote("first-push-no-upstream");
            git_create_branch_impl(&local, "new-local", "main").unwrap();
            git_switch_branch_impl(&local, "new-local").unwrap();
            assert!(!git_status_impl(&local).unwrap().has_upstream);
            let preview = git_push_preview_impl(&local, "origin", "new-published").unwrap();
            assert!(preview.ready);
            assert!(!preview.target_exists);
            git_push_target_impl(&local, "origin", "new-published", set_upstream).unwrap();
            let refreshed = git_status_impl(&local).unwrap();
            assert_eq!(refreshed.has_upstream, set_upstream);
            assert_eq!((refreshed.ahead, refreshed.behind), (0, 0));
            let preview = git_push_preview_impl(&local, "origin", "new-published").unwrap();
            assert!(!preview.ready);
            assert_eq!(preview.ahead, Some(0));
            assert!(preview.message.contains("新しいコミットがありません"));
            let _ = fs::remove_dir_all(base);
        }
    }

    #[test]
    fn unavailable_upstream_history_preserves_workspace_state_and_reports_unknown_counts() {
        let (base, local, _writer) = setup_git_remote("missing-upstream-history");
        run_git(&local, &["update-ref", "-d", "refs/remotes/origin/main"]);
        fs::write(local.join("DesignDocs/page.md"), "uncommitted docs\n").unwrap();
        let status = scope_git_status(git_status_impl(&local).unwrap(), "DesignDocs");
        assert!(status.has_upstream);
        assert!(status.tracking_error.as_deref().unwrap().contains("追跡先の履歴"));
        assert_eq!(status.document_changed, vec!["DesignDocs/page.md"]);
        assert!(!status.clean);
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn commit_rejects_invalid_json_before_staging_documents() {
        let base = temp_dir("invalid-json-commit");
        let local = base.join("repo");
        fs::create_dir_all(local.join("DesignDocs")).unwrap();
        run_git(&local, &["init", "--initial-branch=main"]);
        let local = local.canonicalize().unwrap();
        run_git(&local, &["config", "user.name", "DesignBook Test"]);
        run_git(&local, &["config", "user.email", "designbook@example.invalid"]);
        fs::write(local.join("DesignDocs/page.md"), "initial\n").unwrap();
        run_git(&local, &["add", "."]);
        run_git(&local, &["commit", "-m", "initial"]);
        let original_head = git_text(&local, &["rev-parse", "HEAD"]).unwrap();
        fs::write(local.join("DesignDocs/bad.json"), "{broken").unwrap();
        let repo = Repository { source_root: None, root: local.clone(), git_root: local.clone(), documents: local.join("DesignDocs").canonicalize().unwrap(), documents_relative: "DesignDocs".into(), git_documents_relative: "DesignDocs".into(), has_documents: true };
        assert!(git_commit_impl(&repo, "should fail").unwrap_err().contains("JSON 構文エラー"));
        assert!(git_text(&local, &["diff", "--cached", "--name-only"]).unwrap().is_empty());
        assert_eq!(git_text(&local, &["rev-parse", "HEAD"]).unwrap(), original_head);
        let _ = fs::remove_dir_all(base);
    }
}
