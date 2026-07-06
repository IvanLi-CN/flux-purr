export function shouldRefreshCalibrationDraft(
  currentDraft: string,
  nextValue: number,
  previousValueRef: { current: number | null }
) {
  if (previousValueRef.current !== nextValue) {
    previousValueRef.current = nextValue
    return true
  }

  return currentDraft.length === 0
}

export function syncCalibrationDraftText(
  currentDraft: string,
  acknowledgedValue: number | null,
  fallbackValue: number | null,
  previousValueRef: { current: number | null }
) {
  if (acknowledgedValue != null) {
    return shouldRefreshCalibrationDraft(currentDraft, acknowledgedValue, previousValueRef)
      ? String(acknowledgedValue)
      : currentDraft
  }

  previousValueRef.current = null
  if (currentDraft.length === 0 && fallbackValue != null) {
    return String(fallbackValue)
  }

  return currentDraft
}
