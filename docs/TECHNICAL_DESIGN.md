# 技術設計

## 目的と構成

Windows 上で動作するローカルファーストの設計書ワークスペース。Tauri 2 をデスクトップ境界に置き、React + TypeScript の UI、Milkdown/Crepe の Markdown 編集、Rust のファイル/Git ブリッジで構成する。設計データは選択したリポジトリ内の Markdown/JSON のみを正本とする。アプリ固有の設定やキャッシュを Unity の `Assets`、`Packages`、`ProjectSettings` に書き込まない。

## モジュール

- UI: 左ナビゲーション、文書キャンバス、JSON フィールド表、Git/履歴パネル。
- Tauri commands: Unity ルート、Git ルート、設計書ルートを別々に扱う。設計書がない有効な Unity プロジェクトも開き、空状態を返す。初期化 preview は Git status と全予定パスを読み取り確認し、ユーザー確認後のみ、予定された8カテゴリ/画像ディレクトリと README を `create_new` 相当で作る。既存パスや symlink があれば中止する。ファイル I/O は Unity ルート内の設計書 scope、Git 操作は Git ルート基準の同じ scope に限る。開いただけでは fetch/sync しない。
- Git bridge: システム Git CLI を使い、状態、差分、履歴、fetch/Fast-forward、明示 commit/push を提供。
- Markdown: Markdown を本文の正本とし、Crepe による往復編集を行う。Mermaid フェンスはソースを保持し、別枠で図として描画する。
- JSON: フィールド配列を表形式で編集し、ID を維持する。保存前に JSON 構文と ID 重複を確認する。

## セキュリティ境界

Tauri のファイルアクセスは Rust command 経由に限定する。相対パスの `..`、絶対パス、選択済み設計書ディレクトリ外への canonical path、サポート外拡張子を拒否する。JSON は解析・field ID 検証・同階層 Schema 検証後、一意な一時ファイルへ全量を書いて原子的に置換する。置換失敗時は元ファイルを削除せず、一時ファイルを片付けて失敗を返す。Git の生のエラー出力を UI に返さず、認証情報が remote URL に含まれる場合の漏えいを避ける。Git 認証はシステム設定に任せる。

## Git 方針

同期は未コミット変更がない場合に限る。fetch 後に upstream との差分を数え、分岐時は停止する。behind のみなら `merge --ff-only` を実行する。commit と push は各ボタン操作からのみ呼び出す。プロジェクト選択時には Git を変更する操作を実行しない。選択したパスが Git ルートであることを確認し、そのルートを作業ディレクトリとして全 Git 操作を行う。commit は選択済み document scope のみを `git commit --only` で確定し、他の staged file を含めない。push 前に fetch し、behind があれば push を拒否する。diff は未追跡の Markdown/JSON も追加行として表示する。

## 依存関係

直接依存は React/TypeScript、Tauri API と dialog plugin、Milkdown Crepe、Mermaid、Lucide、および JSON Schema Draft 2020-12 対応の Rust `jsonschema` crate。crate は default features を無効にし、外部 `$ref` 解決を含めない。npm registry の直接依存メタデータで MIT、ISC、Apache-2.0/MIT を確認した。Cargo がないため Rust の解決済み依存ツリーとライセンス監査は未実施。

## 現状の制約

現環境に Rust/Cargo と MSVC compiler がないため native compile は未確認。JSON Schema ファイルは JSON 文書と同階層の `name.schema.json` または `name.json.schema.json` とする。外部 `$ref` は無効。設定/ヘルプ/属性パネルの一部は UI placeholder。画像は PNG/JPG/JPEG/SVG を扱い、変換はしない。

---

# 技术设计

## 目标与组成

这是一个运行于 Windows 的 Local-first 设计书工作台。桌面边界采用 Tauri 2，界面使用 React + TypeScript，Markdown 编辑使用 Milkdown/Crepe，文件与 Git 桥接由 Rust 实现。设计数据只以用户所选仓库中的 Markdown/JSON 为权威来源。应用设置和缓存不会写入 Unity 的 `Assets`、`Packages`、`ProjectSettings`。

## 模块

- UI：左侧导航、文档编辑区、JSON 字段表、Git/历史面板。
- Tauri commands：分别维护 Unity 根目录、Git 根目录和设计书目录。有效 Unity 项目即使没有设计书目录也能打开并显示空状态。初始化预览只读检查 Git 状态与所有目标路径；用户明确确认后，才创建预览中的 8 个分类/图片目录和 README。遇到已存在路径或符号链接则停止。文件读写限于 Unity 根目录内的设计书范围；Git 操作使用真实仓库根目录及对应路径。仅打开项目不 fetch/sync。
- Git bridge：调用系统 Git CLI，提供状态、差异、历史、fetch/Fast-forward，以及显式 commit/push。
- Markdown：Markdown 是正文权威来源，使用 Crepe 往返编辑。Mermaid 代码围栏保留原文，并在独立区域绘图。
- JSON：字段数组以表格编辑并保留 ID。保存前检查 JSON 语法和重复 ID。

## 安全边界

Tauri 文件访问仅通过 Rust command。拒绝包含 `..` 的相对路径、绝对路径、所选设计书目录之外的 canonical path 和不支持的扩展名。JSON 经解析、field ID 检查和同目录 Schema 验证后，写入独立命名的临时文件并原子替换。替换失败时不会先删除原文件。Git 原始错误输出不传给界面，避免 remote URL 含认证信息时泄漏。Git 认证使用系统配置。

## Git 规则

仅在工作区干净时同步。fetch 后检查与 upstream 的提交差；检测到分叉即停止。仅 behind 时执行 `merge --ff-only`。commit 和 push 只由对应按钮操作触发。选择项目时不会修改 Git；选择路径必须是 Git 根目录，所有 Git 命令均在该根目录执行。push 前再次 fetch；若远端领先则拒绝 push。

## 依赖

直接依赖包括 React/TypeScript、Tauri API 和 dialog plugin、Milkdown Crepe、Mermaid、Lucide，以及支持 JSON Schema Draft 2020-12 的 Rust `jsonschema` crate。禁用 crate 默认功能，不启用外部 `$ref` 解析。通过 npm registry 元数据确认直接依赖许可证为 MIT、ISC 或 Apache-2.0/MIT。由于缺少 Cargo，尚未检查 Rust 锁定依赖树和许可证。

## 当前限制

当前环境没有 Rust/Cargo 和 MSVC compiler，因此未验证原生编译。JSON Schema 文件应与 JSON 文档同目录，命名为 `name.schema.json` 或 `name.json.schema.json`；禁用外部 `$ref`。设置/帮助/属性面板仍有 UI placeholder。图片支持 PNG/JPG/JPEG/SVG，不做图像转换。
