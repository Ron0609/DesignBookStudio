import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'

export function isPreviewImage(path: string) {
  return /\.(?:jpe?g|png)$/i.test(path)
}

export function ImagePreview({ path }: { path: string }) {
  const [source, setSource] = useState('')
  const [error, setError] = useState('')
  const [dimensions, setDimensions] = useState('')
  const [revision, setRevision] = useState(0)
  const [zoom, setZoom] = useState(1)
  const [naturalWidth, setNaturalWidth] = useState(0)
  const [fitWidth, setFitWidth] = useState(0)
  const viewport = useRef<HTMLDivElement>(null)
  const zoomValue = useRef(1)
  const anchor = useRef<{ x: number; y: number; left: number; top: number; ratio: number } | null>(null)
  const changeZoom = useCallback((value: number, clientX?: number, clientY?: number) => {
    const node = viewport.current
    if (!node) return
    const next = Math.min(8, Math.max(0.1, value))
    const bounds = node.getBoundingClientRect()
    const x = clientX === undefined ? node.clientWidth / 2 : clientX - bounds.left - node.clientLeft
    const y = clientY === undefined ? node.clientHeight / 2 : clientY - bounds.top - node.clientTop
    anchor.current = { x, y, left: node.scrollLeft, top: node.scrollTop, ratio: next / zoomValue.current }
    zoomValue.current = next
    setZoom(next)
  }, [])
  useLayoutEffect(() => {
    const node = viewport.current
    const point = anchor.current
    if (node && point) {
      node.scrollLeft = (point.left + point.x) * point.ratio - point.x
      node.scrollTop = (point.top + point.y) * point.ratio - point.y
    }
    anchor.current = null
  }, [zoom])
  useEffect(() => {
    const node = viewport.current
    if (!node) return
    const wheel = (event: WheelEvent) => {
      if (!event.ctrlKey || !source || event.deltaY === 0) return
      event.preventDefault()
      event.stopPropagation()
      const delta = event.deltaY * (event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? node.clientHeight : 1)
      changeZoom(zoomValue.current * Math.exp(-Math.max(-300, Math.min(300, delta)) * 0.002), event.clientX, event.clientY)
    }
    node.addEventListener('wheel', wheel, { passive: false })
    return () => node.removeEventListener('wheel', wheel)
  }, [source, changeZoom])
  useEffect(() => {
    const node = viewport.current
    if (!node || !naturalWidth) return
    const resize = new ResizeObserver(() => setFitWidth(node.clientWidth))
    resize.observe(node)
    return () => resize.disconnect()
  }, [naturalWidth])
  useEffect(() => {
    let cancelled = false
    let objectUrl = ''
    setSource(''); setError(''); setDimensions(''); setNaturalWidth(0); setFitWidth(0); setZoom(1); zoomValue.current = 1; anchor.current = null
    if (viewport.current) { viewport.current.scrollLeft = 0; viewport.current.scrollTop = 0 }
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
    <div className="image-zoom-controls" role="group" aria-label="画像の拡大縮小">
      <button disabled={!source || zoom <= 0.1} onClick={() => changeZoom(zoomValue.current / 1.2)} aria-label="画像を縮小">−</button>
      <output aria-live="polite">{Math.round(zoom * 100)}%</output>
      <button disabled={!source || zoom >= 8} onClick={() => changeZoom(zoomValue.current * 1.2)} aria-label="画像を拡大">＋</button>
      <button disabled={!source} onClick={() => changeZoom(1)}>幅に合わせる</button><span>Ctrl＋マウスホイールで拡大・縮小</span>
    </div>
    {error && <p role="alert">{error}</p>}
    <div className="image-preview-viewport" ref={viewport} tabIndex={0} aria-label="画像表示領域">
      {source && !error ? <img src={source} alt={path.split('/').at(-1) ?? '画像'} style={{ width: fitWidth ? fitWidth * zoom : '100%' }} onLoad={(event) => { const image = event.currentTarget; setDimensions(`${image.naturalWidth} × ${image.naturalHeight}`); setNaturalWidth(image.naturalWidth); setFitWidth(viewport.current?.clientWidth ?? image.naturalWidth) }} onError={() => setError('画像を表示できません。ファイルの形式・破損を確認してください。')} /> : !error && <p role="status">画像を読み込んでいます…</p>}
    </div>
  </section>
}
