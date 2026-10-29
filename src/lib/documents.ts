export type FieldRecord = { id?: string; name?: string; [key: string]: unknown }

export function parseFields(content: string): FieldRecord[] {
  let value: unknown
  try { value = JSON.parse(content) } catch { return [] }
  if (Array.isArray(value)) return value as FieldRecord[]
  if (value && typeof value === 'object' && 'fields' in value && Array.isArray(value.fields)) return value.fields as FieldRecord[]
  return []
}

export function validateStructuredDocument(content: string): string[] {
  let value: unknown
  try { value = JSON.parse(content) } catch (error) { return [`Invalid JSON: ${error instanceof Error ? error.message : 'parse error'}`] }
  if (value && typeof value === 'object' && !Array.isArray(value) && 'fields' in value && !Array.isArray(value.fields)) return ['fields must be an array']
  const fields = Array.isArray(value) ? value : value && typeof value === 'object' && 'fields' in value && Array.isArray(value.fields) ? value.fields : null
  if (!fields) return []
  const errors: string[] = []
  const ids = new Set<string>()
  fields.forEach((field, index) => {
    if (!field || typeof field !== 'object' || Array.isArray(field)) { errors.push(`Field ${index + 1} must be an object`); return }
    const record = field as Record<string, unknown>
    if (typeof record.id !== 'string' || !record.id.trim()) errors.push(`Field ${index + 1} needs a stable id`)
    else if (ids.has(record.id)) errors.push(`Duplicate field id: ${record.id}`)
    else ids.add(record.id)
    if (typeof record.name !== 'string' || !record.name.trim()) errors.push(`Field ${index + 1} needs a programmatic name`)
    if (typeof record.type !== 'string' || !['string', 'number', 'integer', 'boolean', 'array', 'object'].includes(record.type)) errors.push(`Field ${index + 1} has an unsupported type`)
    if ('defaultValue' in record) {
      const validDefault = record.type === 'string' ? typeof record.defaultValue === 'string'
        : record.type === 'number' ? typeof record.defaultValue === 'number' && Number.isFinite(record.defaultValue)
          : record.type === 'integer' ? typeof record.defaultValue === 'number' && Number.isInteger(record.defaultValue)
            : record.type === 'boolean' ? typeof record.defaultValue === 'boolean'
              : record.type === 'array' ? Array.isArray(record.defaultValue)
                : record.type === 'object' ? Boolean(record.defaultValue) && typeof record.defaultValue === 'object' && !Array.isArray(record.defaultValue)
                  : false
      if (!validDefault) errors.push(`Field ${index + 1} has a default value that does not match its type`)
    }
  })
  return errors
}

export function findDocumentMatches(files: Array<{ path: string; kind: 'markdown' | 'json' }>, contents: Record<string, string>, query: string) {
  const needle = query.trim().toLocaleLowerCase()
  if (!needle) return files
  return files.filter((file) => `${file.path}\n${contents[file.path] ?? ''}`.toLocaleLowerCase().includes(needle))
}
