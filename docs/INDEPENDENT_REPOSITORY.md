# 独立設計書リポジトリへの移行（2026-10-31）

現在の正本は `D:\UnityProjects\Xiuxian_DesignBook\DesignDocs`、独立 Git の `main` です。
GitHub は `Ron0609/XiuxianGame_DesignDocs`。GitHub の作成・初回 Push は承認済みで完了しました。
ゲーム側の旧 DesignDocs は、リモート検証とユーザー承認を経て削除済みです。

アプリはこのパスだけを開きます。旧 Unity の保存パスも新しい正本へ案内します。
起動時および60秒ごとに origin/main を確認し、未保存編集・未コミット変更・履歴分岐がある場合は更新を停止します。
自動 Commit / Push は行いません。更新通知と右側の履歴から差分・本文位置を確認できます。
main 以外への切替・作成、任意のリモート変更は使用できません。

## バックアップ

`D:\UnityProjects\Xiuxian_DesignDocs_Backup_20261031_010206` にゲーム全履歴、ソフト全履歴、未コミットのソフトファイル、両方の DesignDocs、独立リポジトリの bundle を保存しました。
旧 linked worktree は `OriginalLinkedWorktree` に Git の正式な move で保持しました。
設計書14ファイルを SHA-256 で照合し、一致を確認しました。
独立側は元コミット ac4bf10 からのスナップショットとして初期コミット0481bd9を作成しました。過去の全履歴はゲームの bundle に保存しています。

## 承認後の順序

1. 公開範囲を確認し、Ron0609/XiuxianGame_DesignDocs を作成する。
2. origin を設定し、main を初回 Push する。
3. 別の検証ディレクトリへ clone し、コミットIDと14ファイルのハッシュを再照合する。
4. アプリの実リモート同期を確認する。
5. ここまで成功してから、ゲーム側 DesignDocs の削除について別途確認する。
6. 削除を承認された場合、ゲーム側 AGENTS.md の参照先も独立設計書へ更新する。

旧 SPARSE_WORKTREE.md / VNEXT.md に記載された共有 Git 方式は旧仕様です。

## 最終検証

フロントエンド52件、Rust72件のテストが成功しました。ブラウザーで更新通知・本文位置・差分・未保存保護を確認し、release exe を更新しました。
初回のテストで混入した生成ファイルとコミットは TestAffectedIndependentRepo に隔離し、検証済み bundle から独立側を再構築しました。最終 HEAD は0481bd9、クリーンです。14ファイルを再確認し、ゲーム側との差は Git の改行変換だけです。SHA256-verified.json に最終ファイルのハッシュを保存しました。

## GitHub 移行完了

ユーザー承認後、私有リポジトリ https://github.com/Ron0609/XiuxianGame_DesignDocs を作成し、main を Push しました。
origin/main を追跡し、HEAD は 0481bd903e77d8897862220ec73f4e7acbafaadf です。
バックアップ内 GitHubVerificationClone へ実際に clone し、14ファイルすべての SHA-256 とコミットIDを照合しました。
実リモートからの再 Fetch も成功し、ahead/behind は0/0です。
ネイティブ画面の追加検証は、この環境で computer-use の @oai/sky が読み込めず実施できていません。
ゲーム側 DesignDocs の削除と AGENTS.md の参照更新は、承認後に完了しました。

## ゲーム側の旧設計書を整理済み

ユーザー承認後、バックアップと新リポジトリを再検証し、ゲーム側 DesignDocs の14ファイルを削除しました。
AGENTS.md の設計書入口を独立リポジトリへ更新し、リンク先の存在を確認しました。
Assets / Packages / ProjectSettings に旧 DesignDocs への参照はありません。
ゲーム側の整理は feature/shared-foundation、ソフトの更新は main にコミットしてそれぞれ origin へ Push します。独立設計書側は main で管理します。
