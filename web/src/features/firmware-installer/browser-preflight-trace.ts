import {
  type BrowserSerial,
  type BrowserSerialPort,
  getBrowserSerial,
  normalizeBrowserSerialError,
  selectBrowserSerialPort,
} from '../control-plane-demo/web-serial'

export interface BrowserPreflightTraceEvent {
  at: string
  event: string
  detail: string
  tone: 'info' | 'success' | 'warning' | 'error'
}

export function beginBrowserUsbPreflight({
  serial = getBrowserSerial(),
  now = () => new Date(),
  onTrace,
}: {
  serial?: BrowserSerial | null
  now?: () => Date
  onTrace: (entry: BrowserPreflightTraceEvent) => void
}): Promise<BrowserSerialPort> {
  const userActivation = navigator.userActivation?.isActive === true
  onTrace(createTraceEvent(now, '预检已点击', '浏览器 USB 预检由用户操作触发。', 'info'))

  if (!serial) {
    const error = new Error('Browser USB requires desktop Chrome or Edge on HTTPS or localhost.')
    onTrace(createTraceEvent(now, '浏览器 USB 不可用', error.message, 'error'))
    return Promise.reject(error)
  }

  let selection: Promise<BrowserSerialPort>
  try {
    // This call must stay in the click stack. Any await before it removes the
    // transient activation required for Chrome to display the chooser.
    selection = selectBrowserSerialPort(serial, undefined, true)
  } catch (error) {
    const normalized = normalizeBrowserSerialError(error)
    onTrace(createTraceEvent(now, '浏览器 USB 选择器被拒绝', normalized.message, 'error'))
    return Promise.reject(normalized)
  }

  const tracedSelection = selection.then(
    (port) => {
      onTrace(
        createTraceEvent(
          now,
          '浏览器 USB 端口已选择',
          `选择器已确认串口设备；requestPort() 调用时 userActivation=${userActivation ? 'active' : 'inactive'}。`,
          'success'
        )
      )
      return port
    },
    (error) => {
      const normalized = normalizeBrowserSerialError(error)
      onTrace(createTraceEvent(now, '浏览器 USB 选择器被拒绝', normalized.message, 'error'))
      throw normalized
    }
  )
  void tracedSelection.catch(() => undefined)
  onTrace(
    createTraceEvent(
      now,
      '浏览器 USB 选择器已请求',
      `requestPort() 已在点击同步栈内发起；userActivation=${userActivation ? 'active' : 'inactive'}。`,
      'info'
    )
  )
  return tracedSelection
}

function createTraceEvent(
  now: () => Date,
  event: string,
  detail: string,
  tone: BrowserPreflightTraceEvent['tone']
): BrowserPreflightTraceEvent {
  return { at: now().toISOString(), event, detail, tone }
}
