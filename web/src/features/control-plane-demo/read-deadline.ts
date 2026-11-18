/** Bound headers and body together, including fetch adapters that ignore abort. */
export async function withReadDeadline<T>(
  timeoutMs: number,
  timeoutError: () => Error,
  read: (signal: AbortSignal) => Promise<T>
): Promise<T> {
  const controller = new AbortController()
  let timer: ReturnType<typeof setTimeout> | undefined
  const deadline = new Promise<never>((_, reject) => {
    timer = globalThis.setTimeout(() => {
      reject(timeoutError())
      controller.abort()
    }, timeoutMs)
  })
  try {
    return await Promise.race([read(controller.signal), deadline])
  } finally {
    globalThis.clearTimeout(timer)
  }
}
