> 現在は独立リポジトリ main 専用です。[移行状況と現行仕様](INDEPENDENT_REPOSITORY.md)を参照してください。以下の共有 Worktree の説明は旧仕様です。

# vNext：独立 Worktree・GitHub 更新通知

## 移行前の実プロジェクト確認（2026-10-30、日本時間）

- Unity: `D:\UnityProjects\Xiuxian`。
- Git ルート: `D:/UnityProjects/Xiuxian`。
- 開発ブランチ: `feature/shared-foundation`、HEAD: `5d0a25e4a5fdfb4d74587dcb2ce570dd3f7de341`。upstream は未設定。
- main: `42c7689`、追跡先は `origin/main`。ローカルに保存済みのリモート参照との比較は先行 0・遅延 0。実リモートへの Fetch は実行していないため、GitHub の最新状態を確認した結果ではない。
- Worktree は Unity 開発用の 1 件だけ。main を使用する Worktree はない。
- `D:\UnityProjects\Xiuxian_DesignBook` は存在しない。
- ステージ済み変更はない。ゲーム側の変更・未追跡ファイルが多数ある。
- `DesignDocs/06_開発規約/導入とGit運用.md` に未コミット変更がある（41 行追加・8 行削除）。
- 開発側には main に含まれない設計書変更を含むコミット `5d0a25e 初期作成` がある。main 自体には、保存済み origin/main に対する未送信の設計書コミットはない。

**移行は停止した。実 Worktree の作成、ブランチ切り替え、Commit、Push、Pull、Fetch、設定変更は実行していない。**

全変更一覧、ステージ済み一覧、追跡済み設計書、設計書差分、ブランチ・Worktree 一覧は [監査記録](VNEXT_MIGRATION_AUDIT.json) に保存している。これはローカルの監査資料であり、今回 GitHub に公開・Push していない。

### 移行の前に決めること

元の設計書の未コミット変更と、開発側だけにある設計書コミットをどの経路で共有するか、ユーザーの判断が必要。自動で main に移したりコミットしたりしない。内容の保護・共有が済んだ後、アプリで元の Unity プロジェクトを選択し、作成先とブランチを確認して作成する。未解決のままでは安全検査が停止する。

## 実装

### 開く・作成する

`preview_workspace` は Unity の有効性、実 Git ルート、HEAD・upstream、全未コミット変更、ステージ済み変更、DesignDocs の変更、未共有設計書コミット、Worktree 一覧、main の占有とパス競合を検査する。`create_workspace` は明示確認後に再検査し、`git worktree add` でのみ作成する。既存ディレクトリを上書きしない。

既存 Worktree は `.git` ファイル、共通 Git ディレクトリ、Git への登録、main ブランチ、リンク・ジャンクションの不在を確認する。未コミット変更がある既存 Worktree は読み込めるが、自動同期を停止する。

`Repository.root` は専用 Worktree 内の Unity ルート、`git_root` は専用 Worktree の Git ルート、`documents` はその DesignDocs。`source_root` に元の Unity ルートを保持する。すべてのファイル操作と Git 操作はこの Repository を使用し、操作ごとに Worktree の所属とブランチを再検査する。

Unity が Git ルートのサブディレクトリの場合も、リポジトリ内の相対位置を専用 Worktree に対応させる。作成先は Git ルートの兄弟ディレクトリに置き、元の開発ツリー内に Worktree を作らない。Git for Windows に渡す作成先から、Rust の正規化で付く Windows の拡張パス接頭辞を除去する。

### 同期

`sync_workspace` を非同期タスクとして実行する。Git の対話認証を無効にし、取得処理には 45 秒の待機上限を設ける。未保存編集・未コミット変更・進行中の Git 操作がある場合は取得・更新を停止する。

`origin/main` を明示的に取得し、実コミットの祖先関係を確認する。安全条件を満たす場合だけ `merge --ff-only` で同じ履歴上を早送りする。履歴統合や強制更新はしない。更新直前にも HEAD と作業ツリーを再検査する。Fetch の参照更新に付く `+` はリモート追跡参照の更新用で、Push や作業ツリーの強制上書きではない。

更新中は UI の編集・ファイル操作を無効にする。バックエンドのプロジェクト状態も保持し、他のコマンドは待機で UI を止めず「同期処理中」として拒否する。同期後に Repository とファイルツリー、Git 状態、現在の文書を再読込する。Git のフックと再帰的サブモジュール更新は自動同期時に無効にする。

接続に失敗しても既存ファイルを表示でき、更新パネルから再試行できる。既存の Commit は設計書範囲だけを対象とし、その他のステージ済み変更を巻き込まない。Push は既存コミットだけを main に送信する。Unity 開発ブランチの切り替えは行わない。

### 更新記録・通知・移動

Git のコミット間差分を `--name-status -z --find-renames` とパッチで取得する。追加・変更・削除・名前変更、変更行、Markdown の近傍見出し、JSON の変更、更新前後の完全ハッシュを保存する。ファイル変更時刻は判定に使わない。

保存先は Tauri のアプリデータディレクトリ内 `github-updates/<Worktree パスのハッシュ>.json`。既読状態も同じファイルに保存し、Git に追加しない。早送り前に未完了記録を書き、成功後に完了状態へ更新する。早送り後にアプリが終了した場合は、次回 HEAD を照合して記録を回復する。破損した記録は上書きせず停止する。

実際に設計書の差分が適用された場合だけ通知し、同じ同期の通常の再起動では再通知しない。「後で見る」は未読を維持する。「更新を見る」、個別既読、全既読に対応する。

右側にスクロール・折り畳み可能な履歴を表示する。変更項目を選ぶと読み取り専用の文書へ移動し、右側で変更前後のパッチを表示する。移動前に現在のファイルが記録後のコミット内容と一致するか確認し、一致しなければ差分だけに戻す。削除内容と特定できない位置は読み取り専用の差分表示にする。未保存編集を破棄して移動しない。

## 検証方法

```powershell
cargo test --manifest-path src-tauri/Cargo.toml
cargo check --manifest-path src-tauri/Cargo.toml
pnpm test
pnpm lint
pnpm build
pnpm tauri build --no-bundle
```

ブラウザー回帰テストは Playwright とインストール済み Edge を使用する。環境に Playwright がある場合は、必要に応じて `DESIGNBOOK_PLAYWRIGHT_MODULE` にそのモジュールの場所を指定する。

```powershell
node scripts/verify-reading-search.mjs
node scripts/verify-workspace-updates.mjs
```

ブラウザーテストの Tauri API は模擬実装であり、ネイティブデスクトップ UI の手動確認とは区別する。Git 書き込みを伴う Rust テストは、独立した一時リポジトリとローカル bare remote だけを使用する。

## 制限・未検証事項

- 実プロジェクトの移行と GitHub への実通信は安全上の理由で保留。
- Windows の実ウィンドウ操作は computer-use の `@oai/sky` を読み込めず、今回手動確認できていない。
- 古い実行ファイルが現在起動中のため、それを強制終了したり上書きしたりしない。新ビルドの起動前に既存ウィンドウを通常の手順で閉じる必要がある。
- 差分は最大 2 MB/ファイルまで保持する。超える場合は省略理由を表示し、位置移動を行わない。
- 記録後に文書が変わった場合は、内容を推測して移動せず差分表示に戻る。
- Git のリモート設定・ブランチ参照は両 Worktree で共有される。独立するのは HEAD・index・作業ファイルであり、別リポジトリではない。

## 今回の実行結果（2026-10-30、日本時間）

| 項目 | 実際の結果 |
| --- | --- |
| Rust tests | 65 件成功、失敗 0、除外 0 |
| Cargo check | 成功 |
| フロントエンド tests | 51 件成功、9 ファイル、失敗 0 |
| Lint | 成功 |
| フロントエンド build | 成功。500 kB 超のチャンク警告は残る |
| Tauri build | `pnpm tauri build --no-bundle` 成功 |
| 検索・閲覧ブラウザー回帰 | 成功 |
| Worktree・更新通知ブラウザー回帰 | 成功。Tauri API は模擬実装 |
| 実 Worktree 作成・実 GitHub 同期 | 保留。元の設計書の未コミット変更と開発側だけの設計書コミットを保護 |
| 新バージョンの実デスクトップ起動・手動操作 | 未実施。旧版が起動中で、デスクトップ操作モジュールの読み込みも失敗 |

新しい実行ファイル: `G:\DesignBookStudio\src-tauri\target\release\designbook-studio.exe`。
起動中の `G:\DesignBookStudio\DesignBookStudio.exe` は強制終了も上書きもしていない。

最終再確認では、実 Unity 側の HEAD・ブランチ・155 項目の変更状態（未追跡を展開）・対象設計書の SHA-256 が監査時と一致した。ステージ済みは 0 件で、専用 Worktree のディレクトリは未作成。GitHub への公開・Commit・Push は実行していない。`src-tauri/Cargo.lock` と既存のショートカットも維持している。

### 変更したソフトウェアファイル

- `.gitignore`：実ゲームの差分を含むローカル監査記録を公開対象から除外。
- `src-tauri/src/main.rs`：専用 Worktree の Repository、操作先の再検証、同期・保存・Git 書き込みの排他制御、IPC 接続。
- `src-tauri/src/workspace.rs`：Worktree 検査・作成・再利用、同期、通知記録、位置検証、隔離 Git テスト。
- `src/App.tsx`：起動時復元、作成確認、同期、更新後の再読込、開発/設計書ブランチの区別、編集保護。
- `src/App.css`：更新履歴・通知・差分の表示。
- `src/UpdatePanel.tsx`：通知、履歴、既読、変更箇所、差分、折り畳み。
- `src/lib/projectSession.ts`：プロジェクト情報の型拡張。
- `src/lib/workspace.ts`：Worktree 作成確認、変更集計、移動先・位置の検証。
- `src/lib/workspace.test.tsx`：フロントエンド回帰テスト。
- `scripts/verify-reading-search.mjs`：既存検証の新 IPC 対応。
- `scripts/verify-workspace-updates.mjs`：通知・移動・未保存編集・再起動のブラウザー回帰。
- `README.md`、`docs/TECHNICAL_DESIGN.md`、`docs/GIT_WORKFLOW.md`、`docs/TEST_PLAN.md`、`docs/VNEXT.md`：仕様・移行・検証結果。
- `docs/VNEXT_MIGRATION_AUDIT.json`：ローカル限定の詳細監査資料。Git の対象外。
