import { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'

export function isPreviewImage(path: string) {
  return /\.(?:jpe?g|png)$/i.test(path)
}

export function ImagePreview({ path }: { path: string }) {
  const [source, setSource] = useState('')
  const [error, setError] = useState('')
  const [dimensions, setDimensions] = useState('')
  const [revision, setRevision] = useState(0)
  useEffect(() => {
    let cancelled = false
    let objectUrl = ''
    setSource(''); setError(''); setDimensions('')
    void invoke<ArrayBuffer>('read_image', { path }).then((bytes) => {
      if (cancelled) return
      const mime = /\.png$/i.test(path) ? 'image/png' : 'image/jpeg'
      objectUrl = URL.createObjectURL(new Blob([bytes], { type: mime }))
      setSource(objectUrl)
    }).catch((reason) => { if (!cancelled) setError(`画像を読み込めません: ${String(reason)}`) })
    return () => { cancelled = true; if (objectUrl) URL.revokeObjectURL(objectUrl) }
  }, [path, revision])
  return <section className="image-file-preview" aria-label="画像プレビュー">
    <div className="image-preview-info"><span>{path.split('/').at(-1)}{dimensions && ` · ${dimensions}`}</span><button onClick={() => setRevision((value) => value + 1)}>再読み込み</button></div>
    <p className="image-preview-note">画像は閲覧専用です。ファイル名で検索できます。画像内の文字検索には対応していません。</p>
    {error ? <p role="alert">{error}</p> : source ? <img src={source} alt={path.split('/').at(-1) ?? '画像'} onLoad={(event) => setDimensions(`${event.currentTarget.naturalWidth} × ${event.currentTarget.naturalHeight}`)} onError={() => setError('画像を表示できません。ファイルの形式・破損を確認してください。')} /> : <p role="status">画像を読み込んでいます…</p>}
  </section>
}
