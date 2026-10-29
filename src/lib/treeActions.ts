export function containsEntry(parent: string, path: string): boolean {
  return path === parent || path.startsWith(`${parent}/`)
}

export function parentDirectory(path: string): string {
  return path.slice(0, path.lastIndexOf('/'))
}

export function relocatedPath(path: string, source: string, destination: string): string {
  return containsEntry(source, path) ? destination + path.slice(source.length) : path
}

export function canDropEntry(source: string, directory: string): boolean {
  return Boolean(source && directory && !containsEntry(source, directory) && parentDirectory(source) !== directory)
}

export function entryProtectionReason(source: string, current: string, dirty: boolean, externalChanged: boolean): string {
  return containsEntry(source, current) && (dirty || externalChanged) ? '操作する文書に未保存の編集または外部変更があります。先に保存・確認してください。' : ''
}
