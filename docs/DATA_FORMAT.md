# データ形式

## Markdown

本文は UTF-8 Markdown ファイルを正本とする。見出し、段落、強調、リンク、リスト、表、引用、コードフェンス、相対画像リンクを Markdown として保持する。Mermaid は ```` ```mermaid ```` コードフェンスで保存する。編集コンポーネントが未対応構文を完全保持する保証は未検証のため、保存前に差分確認し、重要ファイルを Git 管理する。拡張書式の font size / color は現 MVP では提供しない。

## JSON フィールド

推奨形式:

```json
{
  "fields": [
    {
      "id": "FLD-CULT-001",
      "name": "cultivation_speed",
      "displayName": "修炼速度",
      "type": "number",
      "defaultValue": 1.0,
      "description": "角色基础修炼速度倍率",
      "constraints": { "minimum": 0 }
    }
  ]
}
```

`id` は永続キーで、名前変更後も維持する。保存時に JSON parse、`fields[].id` の重複、型と `defaultValue` の一致を検証する。アプリ内では `id`、`name`、`type` の存在を検証する。追加制約は JSON と同じディレクトリに `name.schema.json` (例: `character.schema.json`) または `name.json.schema.json` として配置する。JSON Schema Draft 2020-12 を検証し、エラーは instance JSON Pointer と理由を表示する。セキュリティ上 `$ref` による外部ファイル/ネットワーク参照は無効。`constraints` 自体は保存されるがフィールド表からは編集できない。

## 画像と図

Markdown から相対パスで画像を参照する。PNG/JPG/JPEG/SVG の追加は設計書ディレクトリ内 `images/` にコピーし、既存名を上書きせず連番名を使う。置換は文書で参照済みのローカル画像に限り、同拡張子で内容を置き換える。参照切れを表示する。Mermaid のソースは Markdown fenced code として残し、プレビューを別途生成する。

---

# 数据格式

## Markdown

正文以 UTF-8 Markdown 文件为权威来源。标题、段落、强调、链接、列表、表格、引用、代码围栏和相对图片链接按 Markdown 保存。Mermaid 使用 ```` ```mermaid ```` 代码围栏。尚未验证编辑器是否能完整保留所有不支持的语法，因此保存前请查看差异并使用 Git 管理重要文件。当前 MVP 不提供字体大小/颜色等扩展格式。

## JSON 字段

推荐格式：

```json
{
  "fields": [
    {
      "id": "FLD-CULT-001",
      "name": "cultivation_speed",
      "displayName": "修炼速度",
      "type": "number",
      "defaultValue": 1.0,
      "description": "角色基础修炼速度倍率",
      "constraints": { "minimum": 0 }
    }
  ]
}
```

`id` 是持久键，字段改名后也必须保持不变。保存前会解析 JSON，检测 `fields[].id` 重复以及类型与 `defaultValue` 是否匹配。应用检查 `id`、`name`、`type` 是否存在。附加约束 Schema 放在 JSON 同目录，命名为 `name.schema.json`（例如 `character.schema.json`）或 `name.json.schema.json`。使用 JSON Schema Draft 2020-12 验证，错误包含 instance JSON Pointer 和原因。为安全起见，`$ref` 外部文件/网络引用均关闭。`constraints` 可保存，但不能在字段表中编辑。

## 图片和图表

Markdown 使用相对路径引用图片。PNG/JPG/JPEG/SVG 会复制到设计书目录下的 `images/`；不覆盖已有文件，自动添加序号。替换只针对文档中已有的本地图片，并要求扩展名一致。界面会提示失效引用。Mermaid 源码保存在 Markdown fenced code 中，预览单独生成。
