> 現在は独立リポジトリ main 専用です。[移行状況と現行仕様](INDEPENDENT_REPOSITORY.md)を参照してください。以下の共有 Worktree の説明は旧仕様です。

# 独立 Worktree と Sparse Checkout（2026-10-31）

本書は、従来の全ファイルを展開する専用 Worktree 方式を更新する。前回の結果は VNEXT.md に履歴として残している。

## 実プロジェクトの移行前確認

- Unity: `D:\UnityProjects\Xiuxian`。Git ルートも同じ。
- 現在のブランチ: `main`。
- HEAD: `ac4bf10501c3ee508252b22c8b8634c483a38306`。
- main と、ローカルに保存済みの origin/main は同じコミット。main の追跡先は origin/main。
- 既存の feature/shared-foundation も同じコミット。
- 未コミット・未追跡変更 0 件、ステージ済み 0 件、DesignDocs の変更 0 件。
- 保存済み origin/main と比較した未送信の設計書コミットはない。GitHub 最新状態を確認する Fetch は実行していない。
- Worktree は Unity 開発側だけ。main がそこで使用されている。
- `D:\UnityProjects\Xiuxian_DesignBook` は存在しない。
- DesignDocs 内の 14 ファイルについて SHA-256 を保存した。

main を専用側で使用するには、Unity 側を既存の開発ブランチへ切り替える必要がある。指示書の「自動切り替え禁止」に従い、feature/shared-foundation への通常切替と専用 Worktree 作成についてユーザーの確認を求めた。返答がない間は実移行を実行しない。

詳細はローカル監査ファイル `docs/SPARSE_MIGRATION_AUDIT.json` に保存する。このファイルには元のプロジェクト情報が含まれるため .gitignore で除外している。

## Sparse Checkout の方式

Git の cone モードは指定ディレクトリだけでなく、その祖先の直下にあるファイルも展開する。ルートの README やその他のファイルも含まれるため、今回の「DesignDocs と Git 管理情報だけ」という条件には一致しない。

このため、指示書の cone モード例をそのまま使用せず、単一の絶対位置指定パターンを使う非 cone モードを採用した。sparse index は使わず、通常の index と skip-worktree ビットを使用する。Git 2.50.1.windows.1 で検証している。

根拠: [Git 公式 sparse-checkout 資料](https://git-scm.com/docs/git-sparse-checkout)。特に set の祖先直下ファイルと worktree-specific config の説明を参照。

新規作成で専用 Worktree に対して行う処理は次のとおり。

```text
git worktree add --no-checkout <専用パス> main
git sparse-checkout set --no-cone --no-sparse-index /DesignDocs/
git read-tree -mu HEAD
```

最後のコマンドは新規作成直後の未初期化 index を HEAD から準備するためにだけ実行する。Sparse 設定後に実行するため、ゲームのファイルを全展開しない。既存の Worktree の変換には使用しない。reset・clean・stash・force checkout は使用しない。

Unity がリポジトリのサブディレクトリにある場合は、専用 Worktree 内の対応する `<相対位置>/DesignDocs` だけを指定する。作成先は Git ルートの兄弟ディレクトリとし、Unity 開発側の作業ツリー内部には作成しない。

## 既存 Worktree の扱い

- 同じ Git 共通ディレクトリに属する正規の linked Worktree、main ブランチ、リンク・ジャンクション不在を確認する。
- すでに正しい Sparse 設定ならそのまま再利用する。
- 全展開された既存 Worktree は、そのまま編集対象にはしない。変換の確認を表示し、承認後に再検査する。
- 未コミット・未追跡変更や進行中の Git 操作があれば変換を停止する。
- `git status` だけでなく、範囲外の実ファイル・ディレクトリを確認する。無視されたファイルや空の未追跡ディレクトリがある場合も、削除せず停止する。
- 変換対象は変更のない追跡済みファイルだけ。元の Unity ファイルやそのブランチを変更しない。
- Sparse 設定は worktree-specific config に保存する。共通の objects と remote は共有するが、Unity 側の Sparse 設定・skip-worktree 状態は変えない。
- 共有された旧式 Sparse 設定を Unity 側が使用している場合は、自動変更せず停止する。

## ソフトウェアの変更

`workspace.rs` が作成・検証・Sparse 変換を担当する。専用側に Assets や ProjectSettings を要求していた検証を取り除き、Unity の検証は元の開発ルートに限定した。

既存の Repository 経由で、一覧・新規作成・保存・削除・移動・検索・履歴・差分・Git 操作を専用 Worktree に限定する。毎回、同じリポジトリに属する main と正しい Sparse 範囲を検証する。

Commit は `git add -A -- <DesignDocs の範囲>` と範囲付きの `git commit --only` を使用する。管理中の専用 Worktree に範囲外のステージ済み変更があれば、設計書をステージする前に停止し、既存 index を保持する。実際の新規コミットハッシュを確認してから成功を表示する。

Push は main の既存コミットだけを送信し、自動 Commit はしない。通常の初期選択は origin/main とし、存在しない origin を勝手に作らない。追跡先なしの場合も、確認後の初回 Push と追跡先設定が可能。

画面上部に開発ブランチ・設計書 main・専用設計書ディレクトリを表示する。Commit の無効理由も表示する。

## 自動更新・通知・変更ナビゲーション

既存のバックグラウンド Fetch・早送り更新・通知記録・既読状態・右側の変更ナビゲーションを Sparse Worktree で継続する。

- 未保存編集・未コミット変更・履歴の分岐・Git 操作中は同期を停止する。
- Fetch は専用 Worktree から実行し、既存の Unity 作業ファイルは変更しない。
- 早送りは Sparse 設定を維持する。リモートでゲームファイルが更新されても専用側に Assets を展開しない。
- ignored ファイルは porcelain に現れないため、リモート差分とのパス衝突を別途検査する。衝突時はローカル内容を保持して停止する。
- 更新通知は DesignDocs の実コミット差分だけから作る。ゲームファイルだけの更新では通知しない。
- 更新履歴と既読状態はアプリデータに保存し、Git に含めない。
- 文書の現内容を照合してから変更行へ移動する。削除箇所や不一致は読み取り専用差分に戻す。

## テスト

一時リポジトリとローカル bare remote を使い、以下を検証する。実ゲームにテスト Commit・Push は行わない。

- Worktree 作成・再利用、main の占有による停止。
- 専用側のルートが `.git` と `DesignDocs` だけになること。
- Assets 等の index エントリが skip-worktree のまま保持されること。
- Unity 側のブランチ・ファイル・index・Sparse 設定が変わらないこと。
- 全展開 Worktree の確認付き変換、無視ファイルがある場合の停止。
- 設計書だけの Commit と再オープン後の正しい状態。
- 範囲外のステージ済み削除がある場合に、index を変更せず Commit を停止すること。
- origin/main への Push と、選択した場合だけの upstream 設定。
- Sparse を維持する Fetch・早送り、未コミット・分岐・接続失敗時の保護。
- ignored ファイルへの更新衝突を保護すること。
- 更新通知、記録・既読の永続化、変更箇所移動、未保存編集の保護、起動時復元。

検証コマンド:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml
cargo check --manifest-path src-tauri/Cargo.toml
pnpm test
pnpm lint
pnpm build
pnpm tauri build --no-bundle
node scripts/verify-reading-search.mjs
node scripts/verify-workspace-updates.mjs
```

ブラウザーテストは Playwright と Edge を使用する。必要に応じて DESIGNBOOK_PLAYWRIGHT_MODULE で既存モジュールの場所を指定する。Tauri API の模擬実装による検証であり、実デスクトップ操作の成功とは区別する。

## 今回の検証結果

| 実行内容 | 結果 |
| --- | --- |
| cargo test --manifest-path src-tauri/Cargo.toml | 71 件成功、失敗・除外 0 |
| cargo check --manifest-path src-tauri/Cargo.toml | 成功 |
| pnpm test | 52 件成功、9 ファイル |
| pnpm lint | 成功 |
| pnpm build | 成功。500 kB 超のチャンク警告は継続 |
| pnpm tauri build --no-bundle | 成功。Windows 実行ファイルを生成 |
| verify-reading-search.mjs | 成功。既存の閲覧・検索回帰 |
| verify-workspace-updates.mjs | 成功。通知・変更移動・未保存保護・既読復元。Tauri API は模擬実装 |

実行ファイルは `G:\DesignBookStudio\src-tauri\target\release\designbook-studio.exe`。

既存インスタンスが存在しないことを確認後、独立した WebView プロファイル `.sparse-ui-profile.local` で新実行ファイルを実際に起動した。起動 5 秒後にプロセス継続、タイトル `DesignBook Studio` の OS ウィンドウ生成、応答ありを確認した。実画面内容・ボタン操作・プロジェクト選択・ネイティブの保存/Commit/Push/Pull 操作は未検証。computer-use の `@oai/sky` 読み込みが `Module not found` で失敗しており、OS のプロセス/ウィンドウ情報だけで手動 UI 検証済みとは扱わない。

実プロジェクトは引き続き確認待ち。最終確認で HEAD・ブランチ・変更一覧・index エントリ・skip-worktree フラグ・Sparse 設定・設計書 14 ファイルのハッシュが監査時と一致した。専用ディレクトリは未作成。実 GitHub への Fetch・Push や、実 Commit は行っていない。既存の Cargo.lock、実行ファイル、ショートカット、前回の未コミット修正は維持した。ソフトウェアの GitHub リポジトリにも Commit・Push していない。

### 今回変更したファイル

- `src-tauri/src/workspace.rs`：Sparse 作成・変換・検証、同期保護、隔離 Git テスト。
- `src-tauri/src/main.rs`：Sparse 専用 Commit の範囲外ステージ検査。
- `src/App.tsx`：専用パス表示、Commit 無効理由、origin/main の初期選択。
- `src/lib/workspace.ts`、`src/lib/workspace.test.tsx`：新規作成・既存変換の確認と回帰テスト。
- `scripts/verify-reading-search.mjs`、`scripts/verify-workspace-updates.mjs`：Sparse 検査結果に対応したブラウザー検証。
- `.gitignore`：今回のローカル監査記録を除外。
- `README.md`、`docs/TECHNICAL_DESIGN.md`、`docs/GIT_WORKFLOW.md`、`docs/TEST_PLAN.md`、`docs/SPARSE_WORKTREE.md`：現行仕様と結果。
- `docs/SPARSE_MIGRATION_AUDIT.json`：Git 対象外の実プロジェクト監査記録。

必要な確認は「同じコミットを指す既存 feature/shared-foundation へ Unity 側を通常切替し、main の専用 Sparse Worktree を作成してよいか」。状態が変化した場合は実行せず再確認する。アプリ自体には Unity 側を自動切り替えする機能を追加していない。

## ユーザー承認後の実移行完了（2026-10-31）

ユーザーの「允许」を受け、直前に main・feature/shared-foundation・origin/main がすべて ac4bf10501c3ee508252b22c8b8634c483a38306、作業ツリーと index がクリーン、専用パス未作成であることを再確認して実施した。本節が、上記の「確認待ち」「実移行未完了」を更新する。

- Unity 側を既存 feature/shared-foundation へ通常切替。HEAD は変わらず、強制操作なし。切替フックはこの操作に限り無効化した。
- Xiuxian_DesignBook を main の linked Worktree として --no-checkout で作成。
- 専用側だけに `/DesignDocs/` の非 cone Sparse Checkout を設定し、初期 index を準備。
- 専用側のルートは `.git` と `DesignDocs` の2項目だけ。Assets・Packages・ProjectSettings は展開されていない。
- 専用 main の upstream は origin/main。両 Worktree の HEAD は ac4bf10、両方とも未コミット・未追跡変更なし。
- 同一の共通 Git ディレクトリ D:/UnityProjects/Xiuxian/.git を使用。独立 clone や新 GitHub リポジトリは作成していない。
- Unity 側では Sparse Checkout を有効化していない。専用側の Sparse 設定は worktree-specific config に保存。
- 元の index エントリ・skip-worktree フラグ・設計書14ファイルの SHA-256 を維持。

検証途中で新規チェックアウト側の3文書にバイト差を検出し、追加操作を停止して調査した。Git for Windows の既存設定 core.autocrlf=true による LF→CRLF のみであることを確認した。元ファイルは一切書き換えず、新規側も追加の改行変換・上書きを行っていない。残りのファイルは元の SHA-256 と一致する。

今回の実操作では Commit・Push・Fetch・Pull・reset・clean・stash・remote URL 変更は行っていない。既存のユーザーデータを削除・復元していない。共有 Git 管理情報には Worktree 登録と extensions.worktreeConfig が追加されている。

機械可読の結果はローカル専用 `docs/SPARSE_MIGRATION_RESULT.json` に保存し、Git 対象から除外した。コード変更はなく、直前の Rust 71件・フロントエンド52件と各ビルドの成功結果はそのまま。実際のデスクトップ UI によるプロジェクト選択・GitHub 同期の操作確認は引き続き未実施。

DesignBook Studio では元の Unity プロジェクト `D:\UnityProjects\Xiuxian` を選択する。アプリが対応する専用 Sparse Worktree を識別し、実際の編集先は `D:\UnityProjects\Xiuxian_DesignBook\DesignDocs` になる。
