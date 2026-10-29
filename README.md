# DesignBook Studio v0.1

閲覧・検索更新: 文書は既定で閲覧専用です。「編集」を押した場合だけ Markdown、JSON 原文、JSON フィールド、画像変更を操作できます。「閲覧に戻る」は未保存の編集を破棄しません。検索はファイル名と保存済みの本文を対象にし、ファイル別の一覧に各一致の行・列と前後の文章を表示します。結果をクリックすると通常の閲覧表示を維持したまま、対応する段落や表のセルへスクロールし、一致語を強調します。原文表示は「原文を表示」を明示的に押した場合だけ開きます。ファイルが検索後に変わった場合は再検索を案内します。検索は大文字小文字を区別しない文字列検索で、正規表現ではありません。先頭1000件まで、各文書5MBまで、検索語200文字まで。読み取れない・大きすぎる文書は一覧で通知します。

阅读和搜索更新：文档默认只读，点击“编辑”后才允许修改 Markdown、JSON 源文、JSON 字段或图片。“返回阅读”保留尚未保存的编辑。搜索同时覆盖文档名称和已保存的正文，按文件列出多处匹配、行列号与上下文；点击结果会直接打开正常排版的阅读页面，保留标题、表格等格式，滚动到对应段落或表格单元格并高亮匹配词。只有主动点击“原文を表示”才切换到源文视图。文件在搜索后变化时会提示重新搜索。采用不区分大小写的字面文本搜索，暂不支持正则表达式；最多显示1000项，单文档搜索上限5MB，搜索词最多200字符。跳过的文档会明确提示，不搜索尚未保存的编辑内容。

ファイルツリー更新: 右上の Git 入口は1つに統合しました。左のフォルダーを選択すると、その場所で文書・フォルダーを新規作成できます。保存先選択欄はありません。文書・フォルダーを別のフォルダーへドラッグして移動し、右クリックから名前変更・削除できます。設計書ルート・範囲外パス・リンク・重名上書き・自身の子孫への移動は拒否します。未保存の編集中文書やその親フォルダーは、保存・確認するまで移動・削除できません。削除は確認後の完全削除で、ごみ箱には入りません。移動・名前変更時に Markdown/JSON の内容や相対リンクは自動書換しません。

文件树更新：右上角统一为一个 Git 管理入口。点击左侧文件夹后，新建文档或文件夹直接使用该目录，不再选择保存位置；拖到其他文件夹上即可移动。右键提供重命名和删除。禁止操作设计书根目录、越界路径、符号链接或 Junction，禁止重名覆盖及将文件夹移入自身子目录。编辑中的文档若有未保存内容或未确认的外部修改，会阻止对它及其父目录的移动、重命名和删除。删除经确认后永久执行，不进入回收站；移动/重命名不会自动改写文档中的相对链接。

Windows 内部ドラッグ対応は Tauri の `dragDropEnabled: false` を使用します（[公式設定資料](https://v2.tauri.app/reference/config/#dragdropenabled)）。

追加修正: Push は「送信先を安全確認（読み取り専用）」で対象を検証した後に有効になります。対象変更・Commit・Push 後は再確認が必要です。実際の Push 直前にも再検証します。upstream が設定済みでも履歴を読めない場合は、未設定と区別し、ahead/behind を未確定と表示してプロジェクトの閲覧を継続できます。

补充修复：先点击“送信先を安全確認（読み取り専用）”检查目标，检查通过后 Push 才启用；目标变更、Commit 或 Push 后需要重新检查，真正推送前仍会再次验证。upstream 已配置但本地缺少对应历史时，会显示“历史未验证、ahead/behind 未确定”，项目仍可正常打开。

2026-10-29 更新: 正常に開いた Unity ルートのみをアプリのローカル設定に記憶し、次回起動時に同じ検証・読み込み処理で復元します。初回の更新後は一度プロジェクトを選択してください。復元先が存在しない、または読み取れない場合はエラーを表示して再選択できます。設計書の未コミット変更と Unity のその他の変更は別々に表示します。Commit は設計書だけを対象とし、実際の新規 Hash を確認した後に Git 状態を再読込します。Push は未コミット変更があっても既存コミットだけを送信でき、送信されないファイルを警告として表示します。upstream がなくても remote と対象ブランチを明示して初回 Push できます。Push 時のリモート確認は読み取り専用で、force push は使いません。

2026-10-29 更新：成功打开项目后保存 Unity 根路径；下次启动重新验证并读取文件树及真实 Git 状态。更新后需要先选择一次项目。路径丢失或权限不足会提示重新选择。界面分别列出设计书与 Unity 其他未提交文件。Commit 只提交设计书，确认新的真实 Hash 后刷新状态；其他 Unity 修改仍保留，不会被误算为设计书未提交。Push 允许在工作区有未提交修改时发送已有提交，并明确提示未提交文件不会发送；无 upstream 时可明确选择 remote/目标分支首次推送。远程历史检查不会自动 Fetch，也不会 force push。路径设置保存在此应用的 WebView 本地存储中，清除应用数据会丢失该设置。

DesignBook Studio は、Unity プロジェクト内の既存設計書をそのまま編集する Windows デスクトップアプリです。アプリ側に設計書の複製や別データストアは作りません。

## 主な機能

- Tauri 2 + React + TypeScript、Milkdown/Crepe Markdown エディター。
- Unity プロジェクトのルートを選択し、既存の `DesignDocs`、`Documentation`、`DesignDocuments`、`Design Documents`、`design-docs`、`docs/DesignDocs`、`docs/design` または `docs` をそのプロジェクト内から検出します。Git ルートは Git CLI で別途特定します。Unity プロジェクトがリポジトリのサブディレクトリにあっても開けます。
- Markdown / JSON の閲覧、編集、原子的な保存、外部変更検知、全文検索、Mermaid プレビュー、JSON フィールド編集と検証。
- 画像を設計書内の `images/` に追加し、相対リンクを挿入。ローカル画像の置換と参照切れ検知。
- Git ブランチ/リモート管理、upstream 選択、明示ターゲット付き Push、Fetch と Fast-forward Pull の個別操作。未コミット変更/分岐を検査し、force push は提供しません。

## データと Git

設計書の唯一の正本は選択した Unity プロジェクト内の既存ファイルです。ソフトウェアは別コピーを作りません。フォルダーを開いた時点ではファイルを列挙・読み込み、Git 状態を確認するだけです。自動 fetch / 同期、Git リポジトリの初期化、commit、push は行いません。保存は実ファイルに反映します。commit / push はそれぞれのボタンを押した場合だけ実行します。詳しくは `docs/GIT_WORKFLOW.md` を参照してください。

選択フォルダーには `Assets` と `ProjectSettings/ProjectVersion.txt` が必要です。設計書ディレクトリがないプロジェクトも開けます。その場合、作成プレビューには `DesignDocs`、既定の8カテゴリ/画像ディレクトリ、短い `README.md` のみを表示し、ユーザーが「確認して作成」を押すまで何も作成しません。作成前に Git 状態と全対象パスを再確認し、既存パスやシンボリックリンクがあれば中止します。設計書の読書きは Unity ルート内の検出済みディレクトリに限り、Git 操作は Git CLI で特定したリポジトリを対象にします。

## 開発と確認

```powershell
cd G:\DesignBookStudio
pnpm install
pnpm tauri dev
```

開発サーバーを必要としない実行ファイルは `pnpm tauri build --no-bundle` で作成します。`cargo build --release` だけでは本番用ページの組み込みを保証できません。配布時は Tauri ビルドの出力を使用してください。

独立运行、不依赖开发服务器的程序请使用 `pnpm tauri build --no-bundle` 构建。仅执行 `cargo build --release` 不能保证启用页面打包；交付时应使用 Tauri 构建生成的程序。

開発モードは Vite の 5173 番ポートを固定使用し、使用中なら新しいウィンドウを作らず起動を停止します。Windows の正式ビルドは単一起動プラグインを有効にし、再起動時に既存ウィンドウを前面に表示します。

フロントエンドのみを確認する場合は `pnpm dev` を実行します。この場合はサンプル表示で、実ファイル保存と Git 操作は利用できません。

```powershell
pnpm build
pnpm lint
pnpm test
cargo test --manifest-path src-tauri/Cargo.toml
```

Tauri の Windows ビルドには Rust stable、`x86_64-pc-windows-msvc` target、Visual Studio Build Tools の **Desktop development with C++** workload と WebView2 Runtime が必要です。現環境では Rust/Cargo と Visual Studio 2022 Build Tools が利用可能で、Developer Command Prompt 経由の Rust テストを実行できます。

## 既知の制限

外部 JSON Schema `$ref` は無効です。Schema 編集/生成、画像の crop/resize、Markdown のフォントサイズ/色拡張、設定/ヘルプ/属性画面の一部は未実装です。詳細は `docs/TECHNICAL_DESIGN.md` と `docs/TEST_PLAN.md` を参照してください。

---

# DesignBook Studio v0.1

DesignBook Studio 是一款 Windows 桌面应用，用于直接编辑 Unity 项目中已有的设计文档。应用不会复制设计书，也不会建立第二套数据存储。

## 主要功能

- Tauri 2 + React + TypeScript，Milkdown/Crepe Markdown 编辑器。
- 选择 Unity 项目根目录，并在该项目内检测已有的 `DesignDocs`、`Documentation`、`DesignDocuments`、`Design Documents`、`design-docs`、`docs/DesignDocs`、`docs/design` 或 `docs`。Git 根目录由 Git CLI 单独解析；Unity 项目位于仓库子目录时也可打开。
- 浏览、编辑并原子保存 Markdown / JSON；检测外部修改、全文搜索、Mermaid 预览、JSON 字段编辑和验证。
- 将图片添加到设计书目录中的 `images/`，插入相对链接；替换本地图片并检测失效引用。
- 在软件内管理 Git 分支/远程和 upstream；显式选择 Push 目标；Fetch 与 Fast-forward Pull 分开操作。检测未提交修改/历史分叉，不提供 force push。

## 数据与 Git

所选 Unity 项目中的现有文件是设计书唯一的数据源。软件不会创建副本。打开文件夹时只枚举和读取文件并检查 Git 状态，不会自动 fetch / 同步，不会初始化 Git 仓库，也不会 commit 或 push。保存会直接修改实际文件。只有点击相应按钮才会 commit / push。详情见 `docs/GIT_WORKFLOW.md`。

所选文件夹必须含有 `Assets` 和 `ProjectSettings/ProjectVersion.txt`。即使尚无设计书目录也可以打开项目。此时初始化预览只列出 `DesignDocs`、约定的 8 个分类/图片目录和简短 `README.md`；用户点击“确认并创建”前不会写入任何内容。创建前重新只读检查 Git 状态和全部目标路径，发现既有路径或符号链接即停止。设计书读写仅限 Unity 根目录内的已识别目录；Git 操作使用 Git CLI 解析出的真实仓库。

## 开发与检查

```powershell
cd G:\DesignBookStudio
pnpm install
pnpm tauri dev
```

开发模式固定使用 Vite 的 5173 端口；端口被占用时会停止启动，不会另开一个窗口。Windows 正式构建启用单实例插件，再次启动时会聚焦已有窗口。

仅查看前端时运行 `pnpm dev`。此时显示示例内容，不支持实际文件保存和 Git 操作。

```powershell
pnpm build
pnpm lint
pnpm test
cargo test --manifest-path src-tauri/Cargo.toml
```

构建 Tauri Windows 应用需要 Rust stable、`x86_64-pc-windows-msvc` target、Visual Studio Build Tools 的 **Desktop development with C++** workload 和 WebView2 Runtime。当前环境已检测到 Rust/Cargo 和 Visual Studio 2022 Build Tools，可通过 Developer Command Prompt 运行 Rust 测试。

## 已知限制

外部 JSON Schema `$ref` 已禁用。Schema 编辑/生成、图片裁剪/缩放、Markdown 字体大小/颜色扩展，以及部分设置/帮助/属性页面尚未实现。详情见 `docs/TECHNICAL_DESIGN.md` 和 `docs/TEST_PLAN.md`。


### Git の日常操作 / Git 日常操作

保存後、Commit → Push 画面でメッセージを入力して Commit し、送信元ローカルブランチと送信先リモート・ブランチを選んで Push します。別のローカルブランチは未コミット変更がない状態で明示的に切り替えます。送信先の検査は Push 時に自動実行し、最終確認後に送信します。ブランチ作成、リモート設定、upstream、Fetch / Pull は「詳細 Git 管理」を開いて操作できます。

保存后，在 Commit → Push 界面填写提交说明并 Commit，再选择本地分支、远程及目标分支进行 Push。选择其他本地分支时，需要先在没有未提交修改的状态下明确切换。Push 会自动检查目标并显示最终确认。分支创建、远程设置、upstream、Fetch / Pull 保留在可展开的“详细 Git 管理”中。
