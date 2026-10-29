import { describe, expect, it, vi } from 'vitest'
import capabilities from '../../src-tauri/capabilities/default.json'
import { createCloseRequestHandler, type CloseRequestedEvent } from './windowClose'

function closeEvent() {
  const preventDefault = vi.fn()
  return { event: { preventDefault } satisfies CloseRequestedEvent, preventDefault }
}

describe('window close protection', () => {
  it('grants the main window permission to complete close requests', () => {
    expect(capabilities.windows).toContain('main')
    expect(capabilities.permissions).toContain('core:window:allow-destroy')
  })

  it('allows closing immediately when there are no unsaved edits', async () => {
    const confirmDiscard = vi.fn(async () => true)
    const destroyWindow = vi.fn(async () => {})
    const handler = createCloseRequestHandler({ hasUnsavedChanges: () => false, confirmDiscard, destroyWindow, onError: vi.fn() })
    const { event, preventDefault } = closeEvent()

    await handler(event)

    expect(preventDefault).toHaveBeenCalledOnce()
    expect(confirmDiscard).not.toHaveBeenCalled()
    expect(destroyWindow).toHaveBeenCalledOnce()
  })

  it('keeps the window open when the user cancels exit', async () => {
    const destroyWindow = vi.fn(async () => {})
    const handler = createCloseRequestHandler({ hasUnsavedChanges: () => true, confirmDiscard: async () => false, destroyWindow, onError: vi.fn() })
    const { event, preventDefault } = closeEvent()

    await handler(event)

    expect(preventDefault).toHaveBeenCalledOnce()
    expect(destroyWindow).not.toHaveBeenCalled()
  })

  it('closes after the user confirms discarding unsaved edits', async () => {
    const destroyWindow = vi.fn(async () => {})
    const handler = createCloseRequestHandler({ hasUnsavedChanges: () => true, confirmDiscard: async () => true, destroyWindow, onError: vi.fn() })
    const { event, preventDefault } = closeEvent()

    await handler(event)

    expect(preventDefault).toHaveBeenCalledOnce()
    expect(destroyWindow).toHaveBeenCalledOnce()
  })

  it('prevents repeated close requests while the confirmation is pending', async () => {
    let resolveConfirmation: (confirmed: boolean) => void = () => {}
    const confirmDiscard = vi.fn(() => new Promise<boolean>((resolve) => { resolveConfirmation = resolve }))
    const destroyWindow = vi.fn(async () => {})
    const handler = createCloseRequestHandler({ hasUnsavedChanges: () => true, confirmDiscard, destroyWindow, onError: vi.fn() })
    const first = closeEvent()
    const second = closeEvent()

    const firstRequest = handler(first.event)
    await handler(second.event)

    expect(first.preventDefault).toHaveBeenCalledOnce()
    expect(second.preventDefault).toHaveBeenCalledOnce()
    expect(confirmDiscard).toHaveBeenCalledOnce()
    resolveConfirmation(false)
    await firstRequest
    expect(destroyWindow).not.toHaveBeenCalled()
  })

  it('reports close failures without leaving the handler locked', async () => {
    const onError = vi.fn()
    const destroyWindow = vi.fn().mockRejectedValueOnce(new Error('close failed')).mockResolvedValueOnce(undefined)
    const handler = createCloseRequestHandler({ hasUnsavedChanges: () => true, confirmDiscard: async () => true, destroyWindow, onError })

    await handler(closeEvent().event)
    await handler(closeEvent().event)

    expect(onError).toHaveBeenCalledOnce()
    expect(destroyWindow).toHaveBeenCalledTimes(2)
  })
})
