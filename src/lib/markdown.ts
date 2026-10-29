export function extractImageReferences(markdown: string): string[] {
  const lines = markdown.split(/\r?\n/)
  const references: string[] = []
  let fence: string | null = null
  for (const line of lines) {
    const fenceMatch = line.match(/^\s*(```+|~~~+)/)
    if (fenceMatch) {
      const marker = fenceMatch[1][0]
      if (!fence) fence = marker
      else if (fence === marker) fence = null
      continue
    }
    if (fence) continue
    const imagePattern = /!\[[^\]]*\]\(\s*(?:<([^>]+)>|([^\s)]+))(?:\s+['"][^'"]*['"])?\s*\)/g
    for (const match of line.matchAll(imagePattern)) {
      const raw = (match[1] ?? match[2] ?? '').replace(/\\([\\()[\]])/g, '$1')
      try { references.push(decodeURIComponent(raw)) } catch { references.push(raw) }
    }
  }
  return [...new Set(references)]
}

export function markdownImageLink(documentPath: string, imagePath: string): string {
  const from = documentPath.split('/').slice(0, -1)
  const to = imagePath.split('/')
  let common = 0
  while (common < from.length && common < to.length && from[common] === to[common]) common++
  const relative = [...Array(from.length - common).fill('..'), ...to.slice(common)].join('/')
  const label = imagePath.split('/').at(-1)?.replace(/\.[^.]+$/, '') ?? 'image'
  return `![${label}](<${relative}>)`
}
