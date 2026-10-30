> 現在は独立リポジトリ main 専用です。[移行状況と現行仕様](INDEPENDENT_REPOSITORY.md)を参照してください。以下の共有 Worktree の説明は旧仕様です。

# Git workflow / Git 操作

## 日本語

すべての操作は、選択した Unity プロジェクトから解決した実際の Git ルートで実行します。アプリ起動・プロジェクトを開く処理では fetch、pull、commit、push を行いません。

- **ブランチ**: ローカルブランチと Fetch 済みのリモート追跡ブランチを Git の ref から表示します。作成元を選びローカルブランチを作成できます。切替は未コミット変更とエディター内の未保存変更がある場合に止めます。
- **リモート**: URL の表示では URL 内の user-info と query の全パラメーター値を伏せます。追加・URL 変更・削除の前に確認を表示します。削除はローカル Git 設定から remote と追跡設定を取り除きますが、サーバー上のデータは削除しません。
- **upstream**: 既存 upstream は Fetch 済みのリモートブランチだけ選択できます。存在しないブランチを upstream に設定することはできません。新規リモートブランチは Push で作成し、成功時に tracking を設定できます。
- **Commit**: 設計書ディレクトリの変更を対象に、ユーザーが明示してローカル commit を作成します。Push は行いません。
- **Push**: remote と宛先ブランチを明示します。送信先を確認する操作は `ls-remote` による読み取りです。未コミット変更は警告として表示し送信しません。remote 側の履歴が未取得、remote 側が先行、分岐、送信コミットなしの場合は停止します。通常の non-fast-forward 保護を維持し、force push は実装していません。Push 成功後の upstream 設定は別途チェックを入れた場合だけ行います。
- **Fetch**: remote の参照だけを更新し、作業ツリーのファイルを変更しません。
- **Pull**: ユーザー確認後に指定 branch を Fetch し、fast-forward のみで統合します。未コミット変更や fast-forward できない履歴では停止します。自動 merge commit や conflict 解決は行いません。
- **認証情報**: 保存 URL の user-info/query 値と Git の URL 出力を伏せます。URL 入力欄は password 型です。資格情報をアプリ独自ログに記録しません。Git credential helper / SSH agent の認証画面は Git 側の機能です。

送信先の状態は Push 直前に確認しますが、確認からサーバー処理までの間に remote が変化する可能性があります。その場合も通常の Git push が non-fast-forward を拒否します。認証・ネットワークエラーは Git が返したサニタイズ済みのエラーを表示します。

## 中文

所有操作都在从所选 Unity 项目解析出的真实 Git 仓库根目录执行。启动应用或打开项目时不会自动 fetch、pull、commit 或 push。

- **分支**：从 Git refs 显示本地分支和已 Fetch 的远程跟踪分支。可以选择来源创建本地分支。存在未提交修改或编辑器未保存内容时，切换会被阻止。
- **远程**：显示 URL 时会隐藏其中的 user-info 和 query 参数值。添加、修改 URL、删除前都会要求确认。删除会从本地 Git 配置中移除远程和跟踪设置，不会删除服务器数据。
- **Upstream**：只能选择已 Fetch 且实际存在的远程分支。不存在的远程分支不能设置为已有 upstream；新远程分支应通过 Push 创建，并可选择在成功后建立跟踪关系。
- **Commit**：用户明确操作后，只对设计书目录的变更创建本地提交，不会 Push。
- **Push**：必须明确选择远程和目标分支。确认目标时使用只读 `ls-remote`。未提交修改仅提示且不会被发送。远程历史尚未获取、远程领先、历史分叉或没有可发送提交时会停止。保留 Git 默认的 non-fast-forward 保护，不实现 force push。只有明确勾选后，Push 成功才设置 upstream。
- **Fetch**：只更新远程引用，不修改工作区文件。
- **Pull**：用户确认后 Fetch 指定分支，并仅以 fast-forward 方式合并。存在未提交修改或无法 fast-forward 时停止，不自动创建 merge commit，也不自动解决冲突。
- **凭据**：保存 URL 中的 user-info/query 参数值和 Git URL 输出会被隐藏；URL 输入框使用密码类型。软件不把凭据写入自有日志。Git credential helper / SSH agent 的认证由 Git 处理。

Push 目标会在操作前读取检查，但检查结束到服务器处理期间远程仍可能变化；这种情况下 Git 的常规 non-fast-forward 检查会拒绝 Push。认证或网络错误显示经过凭据脱敏的 Git 错误信息。

## vNext：専用 Worktree の運用（2026-10-30）

設計書の操作対象は独立した main Worktree 内の DesignDocs。開発用 Unity Worktree の HEAD・index・ファイルは変更しない。起動時は専用 Worktree で origin/main を取得し、未保存編集・未コミット変更・分岐がなく早送りできる場合だけ更新する。手動 Pull も更新記録を保存する同期経路を使う。Commit と Push は明示操作のままで、自動実行しない。詳細は [vNext 実装記録](VNEXT.md) を参照。

## Sparse Worktree の運用（2026-10-31）

専用 Worktree は DesignDocs のみを展開する。非表示のゲームファイルは index の skip-worktree で維持し、削除として扱わない。Commit は範囲付き add と commit --only を使用する。専用側に範囲外のステージ済み変更がある場合は index を変更する前に停止する。Push の既定先は origin/main。実移行時に Unity 側 main の解放が必要な場合は、既存開発ブランチへの切替についてユーザーの明示確認を待つ。詳細は [Sparse Worktree 記録](SPARSE_WORKTREE.md) を参照。
