# Test plan and latest results / テスト計画と最新結果

Date: 2026-10-29

## 2026-10-29 回帰確認 / 回归验证

- 阅读态搜索定位修正：默认使用正常 Markdown 阅读排版，按原文行上下文区分重复命中及表格行，并通过 DOM Range 高亮，不向设计书插入标记。增加正文渲染后定位及表格/代码异步挂载处理。`scripts/verify-reading-search.mjs` 使用独立 Edge 无头浏览器及模拟 Tauri API 检查显式编辑、只读搜索跳转、正常表格渲染、第二处命中的单元格和页面异常；浏览器交互脚本已实际通过，并验证只读点击输入不改变文档、从原文返回排版阅读、切回阅读时保留最后输入。它不访问 Unity 项目，不等同于实际桌面窗口手动验收。脚本需要 Playwright，可用 `DESIGNBOOK_PLAYWRIGHT_MODULE` 指定已有模块路径。

- 阅读/搜索回归：编辑回调拒绝只读操作和初始化规范化更新；只读命中视图不生成可编辑文本框，显式进入编辑后才提供输入。临时文档覆盖 Markdown/JSON 正文、同一行与多行的多处命中、名称匹配、大小写、字面特殊字符、CRLF、Emoji 的 UTF-16 列号、长行摘要、限制提示及非法 UTF-8/超大文件跳过。前端检查命中区间、过期结果拒绝、原文高亮及 HTML 转义。搜索在后台线程读取原文件，不写入文件。桌面实际点击“编辑”和搜索结果跳转尚未手动验收。
- 文件树回归：临时目录验证文件/目录移动和重命名、内容保持、重名不覆盖、禁止根目录/自身子目录/路径穿越、拒绝未确认删除、递归删除只影响所选子树，以及含符号链接或嵌套 Git 元数据时在修改前拒绝。前端覆盖移动后路径映射、目录边界、无效拖放和未保存编辑保护；Windows WebView 拖拽配置纳入检查。原生右键/拖拽交互尚未手动验收。
- Push 策略修正：独立临时仓库同时存在设计书未提交草稿、Unity 已暂存/未暂存修改及未跟踪文件时，验证 Push 只发送原有提交。断言远端只包含已提交文档，本地 HEAD、工作区状态、暂存差异及文件内容均保持一致；远端领先/分叉保护仍生效。

- 実行結果 / 实际结果：`cargo test --manifest-path src-tauri/Cargo.toml` 53/53；`pnpm test` 45/45；`pnpm lint` 无警告；`pnpm build` 成功；`pnpm tauri build --no-bundle` 成功，独立程序另存为项目根目录的 `DesignBookStudio.exe`。构建仍有大于 500 kB 的分块体积提示。以下旧章节的次数为历史记录，以本节结果为准。
- 追加回帰 / 补充回归：恢复测试通过与 App 共用的加载函数验证 Unity 根验证、目录树、Git 状态和首个文档重新读取；路径验证失败时只调用打开验证，不创建任何文件。Push 检查未执行、目标变化、无新提交、分叉、历史未知及认证失败均保持 Push 禁用并显示原始安全检查原因。临时仓库删除本地远程跟踪引用后仍可读取工作区，并明确报告 upstream 已设置但计数未知。
- 自動復元 / 自动恢复：成功打开路径的保存、下次会话恢复调用、首次启动不读取项目、路径不可访问时保留设置并返回错误。前端使用与手动打开一致的加载流程，不缓存 Git 状态。
- Commit：独立临时仓库中验证设计书范围提交、新 Hash 返回、提交后再次读取状态、其他已暂存游戏文件保留，以及失败 hook 返回错误且 HEAD 不变。
- Git status：使用 porcelain v1 `-z --untracked-files=all`，覆盖新增、删除、重命名、日文与空格/换行文件名、重命名原路径和目录边界分类。
- Push：独立临时仓库和 bare remote 中验证无 upstream 首次推送、是否设置跟踪的两个选择、成功后重读状态、无提交、目标无新提交、远程领先/分叉保护。前端覆盖目标缺失、未保存编辑的禁用原因及未提交文件路径的非阻塞警告，以及 Commit/Push 成功后刷新与失败传播。
- 桌面手動確認 / 桌面手动验证：本轮未执行真实窗口的重启恢复及点击 Commit/Push 验收。自动化通过不代表手动验收通过。真实游戏项目不参与写入测试。

## 日本語: 実装・確認状況

- **実装済み**: UI からローカル/既知リモートブランチ一覧、現在ブランチ、各ローカル upstream と ahead/behind を確認できます。作成元を選ぶローカルブランチ作成、未コミット状態を拒否する切替、リモート追加/URL 更新/削除、既存 upstream 設定、明示 remote/branch Push、Fetch、Fast-forward のみの Pull を実装しました。
- **安全策**: Commit はユーザー操作時に設計書範囲のローカル commit のみ。Push は未コミット変更を送信せず警告し、remote の実状態を `ls-remote` で確認、履歴不明・remote 先行・分岐・送信コミットなしを拒否します。force push はありません。Pull は dirty worktree を拒否し、fast-forward できない場合は merge しません。Fetch は worktree を変更しません。
- **認証情報**: remote URL の user-info とURL query の全パラメーター値を伏せ、URL 入力を password 欄にしました。Git エラーも URL をサニタイズして表示します。
- **実行済み**: Rust 39 件成功。Vitest 11 件成功。`cargo check`、`pnpm build` 成功。`pnpm lint` は終了コード 0。
- **注意**: build は 500 kB を超える chunk の警告あり。lint は生成された Tauri API ファイルに no-unused-expression 警告あり。アプリを実際に起動して画面操作するデスクトップ実機確認はこのタスクでは未実施です。
- **テスト境界**: Rust Git テストは独立した一時リポジトリとローカル bare remote のみを使用。ゲームプロジェクトには Git 操作を行っていません。`src-tauri/Cargo.lock` は保持しています。

### 実行コマンド

Cargo は通常の PATH にはありませんでしたが、既存の Rustup stable toolchain と Cargo、および Visual Studio 2022 Build Tools の Developer Command Prompt が確認できました。既存環境を使って実行しました。

```powershell
cmd.exe /d /s /c '"C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat" -arch=amd64 -host_arch=amd64 && "C:\Users\Administrator\.cargo\bin\cargo.exe" test --manifest-path src-tauri\Cargo.toml'
pnpm test
pnpm lint
pnpm build
```

## 中文：实现与验证状态

- **已实现**：在 UI 中查看全部本地/已知远程分支、当前分支、每个本地分支的 upstream 与 ahead/behind；选择来源创建本地分支；工作区有未提交或编辑器未保存内容时阻止切换；添加/修改/删除远程；设置已有 upstream；明确选择远程和目标分支 Push；独立 Fetch；仅 fast-forward 的 Pull。
- **安全措施**：Commit 只在用户明确操作时创建设计书范围内的本地提交。Push 对未提交修改显示警告而不发送这些文件，并通过 `ls-remote` 检查远程实时状态；远程历史无法判断、远程领先、历史分叉或没有可发送提交时都会停止。没有 force push。Pull 拒绝 dirty 工作区，无法 fast-forward 时不合并。Fetch 不修改工作区文件。
- **认证信息**：隐藏 remote URL 中的 user-info 和URL query 中的全部参数值；URL 输入框使用密码类型。Git 错误会在显示前对 URL 脱敏。
- **已执行**：Rust 39 项通过；Vitest 11 项通过；`cargo check`、`pnpm build` 成功；`pnpm lint` 退出码 0。
- **提示**：build 有超过 500 kB chunk 的警告；lint 对生成的 Tauri API 文件报告 no-unused-expression 警告。此次没有实际启动应用并手动操作桌面窗口。
- **测试范围**：Rust Git 测试只使用独立临时仓库和本地 bare remote，没有对游戏项目执行 Git 操作。保留了 `src-tauri/Cargo.lock`。

### 执行命令

Cargo 不在普通 PATH 中，但检查确认现有 Rustup stable 工具链、Cargo 和 Visual Studio 2022 Build Tools Developer Command Prompt 均可用；以下使用的是现有环境，没有安装组件。

```powershell
cmd.exe /d /s /c '"C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat" -arch=amd64 -host_arch=amd64 && "C:\Users\Administrator\.cargo\bin\cargo.exe" test --manifest-path src-tauri\Cargo.toml'
pnpm test
pnpm lint
pnpm build
```

### 重点测试

Rust temporary-repository tests cover branch listing/current/upstream, branch creation and dirty-switch refusal, credential-safe remote listing and add/update/remove, existing-only upstream selection, explicit Push to a new branch with optional tracking, dirty-worktree preservation and remote-ahead/divergence protections, target ahead/behind preview, Fetch worktree preservation, and clean fast-forward Pull. Frontend unit tests cover remote branch filtering, Push button safeguards, and confirmation text contents.

## vNext：独立 Worktree と GitHub 更新通知（2026-10-30）

最新の動作・安全条件は [vNext 実装記録](VNEXT.md) を参照。本節は旧来の「Unity 作業ツリーを直接操作」「起動時は Fetch しない」という記述を置き換える。

追加した回帰テスト：

- 確認拒否時の無書き込み、Worktree の初回作成と再利用、main 占有時の停止。
- 元の未コミット設計書、未共有の設計書コミット、パス競合での移行停止。
- 開発側の HEAD・ブランチ・index・変更ファイルを維持したまま、専用 Worktree の設計書だけを Commit。
- 既存 Worktree の未コミット変更を保持し、誤ったブランチを拒否。
- 更新なし、追加・変更・削除・名前変更、JSON と Markdown の複数行変更。
- 未保存編集・未コミット変更・分岐・接続失敗時の停止と再試行。
- 設計書以外だけの早送りでは通知しない。
- 記録の永続化、既読保存、重複通知の抑止、途中終了後の記録回復、破損記録の保護。
- 正しい変更行への移動、削除差分、現在ファイルが変わった場合の安全な差分表示。
- ブラウザーでのプロジェクト復元、開発/設計書ブランチの表示、更新通知、本文移動、未保存編集の保護。

実ゲームに対する Git 書き込みテストは行わない。詳細結果と未検証範囲は VNEXT.md に記録する。

実行結果：Rust 65 件、フロントエンド 51 件が成功。Cargo check・Lint・フロントエンド build・Tauri の単体実行ファイル build が成功。検索の既存ブラウザー回帰と、更新通知・移動・未保存保護・既読復元の新規ブラウザー回帰が成功。実 Unity の移行と GitHub 実通信、新ビルドのネイティブ UI 手動確認は未実施。理由と実行ファイルの場所は VNEXT.md を参照。

## Sparse Checkout 回帰（2026-10-31）

[最新の設計・検証記録](SPARSE_WORKTREE.md) を参照。新規に以下を追加した。

- `.git` と DesignDocs だけの実展開、skip-worktree の保持、Unity 側設定の維持。
- 全展開 Worktree の確認付き変換と、無視ファイルを消さない停止。
- Sparse 状態での設計書のみの Commit・再オープン・origin/main への初回 Push。
- 範囲外のステージ済み削除を検知し、元の index を維持して停止。
- リモートのゲームファイル更新後も Assets を展開しないこと。
- 無視されたローカル設計書とリモート更新の衝突で上書きしないこと。
- フロントエンドでの既存 Worktree 変換の確認拒否・承認。

実ゲームでの移行は、開発側 main を切り替える確認が必要なため保留する。実デスクトップ UI の操作確認は、自動テストと区別して記録する。

今回の実行結果：Rust 71 件、フロントエンド 52 件が成功。Cargo check、Lint、フロントエンド build、Tauri --no-bundle build、既存と新規のブラウザー回帰が成功。独立 WebView プロファイルで新 EXE を実起動し、OS ウィンドウ生成と応答まで確認した。実画面の操作確認と実ゲームの移行は未完了。詳細・確認待ちの内容は SPARSE_WORKTREE.md を参照。

ユーザー承認後の実移行（2026-10-31）：Unity 側を既存 feature/shared-foundation に通常切替し、main の専用 Sparse Worktree を作成済み。両方クリーン、元の index と設計書14ファイルのハッシュを維持。専用側は .git と DesignDocs のみ。実 Commit・Push・Fetch・Pull は未実施。前の「実移行は確認待ち」という状態は完了に更新する。実デスクトップ UI 操作の未検証範囲は変わらない。
