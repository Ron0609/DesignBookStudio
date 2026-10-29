export type CloseRequestedEvent = { preventDefault: () => void }

export type CloseRequestActions = {
  hasUnsavedChanges: () => boolean
  confirmDiscard: () => Promise<boolean>
  destroyWindow: () => Promise<void>
  onError: (error: unknown) => void
}

export function createCloseRequestHandler(actions: CloseRequestActions) {
  let confirmationPending = false

  return async (event: CloseRequestedEvent): Promise<void> => {
    if (confirmationPending) {
      event.preventDefault()
      return
    }
    // Close explicitly so permission/IPC failures are reported by our error handler.
    event.preventDefault()
    confirmationPending = true
    try {
      if (!actions.hasUnsavedChanges() || await actions.confirmDiscard()) await actions.destroyWindow()
    } catch (error) {
      actions.onError(error)
    } finally {
      confirmationPending = false
    }
  }
}
