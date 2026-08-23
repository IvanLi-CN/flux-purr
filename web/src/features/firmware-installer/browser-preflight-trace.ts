import {
  type BrowserSerial,
  type BrowserSerialPort,
  getBrowserSerial,
  isFluxPurrUsbSerialPort,
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
  preauthorizedPorts,
  forcePortSelection = false,
  selectionReason = 'preflight',
  now = () => new Date(),
  onTrace,
}: {
  serial?: BrowserSerial | null
  preauthorizedPorts?: readonly BrowserSerialPort[]
  /** Always use Chrome's chooser, even when this origin already has one port. */
  forcePortSelection?: boolean
  selectionReason?: 'preflight' | 'change_port'
  now?: () => Date
  onTrace: (entry: BrowserPreflightTraceEvent) => void
}): Promise<BrowserSerialPort> {
  const userActivation = navigator.userActivation?.isActive === true
  onTrace(
    createTraceEvent(
      now,
      selectionReason === 'change_port' ? '浏览器 USB 端口更换已点击' : '预检已点击',
      selectionReason === 'change_port'
        ? '将打开浏览器选择器；不会自动选取已授权的其他设备。'
        : '浏览器 USB 预检由用户操作触发。',
      'info'
    )
  )

  if (!serial) {
    const error = new Error('Browser USB requires desktop Chrome or Edge on HTTPS or localhost.')
    onTrace(createTraceEvent(now, '浏览器 USB 不可用', error.message, 'error'))
    return Promise.reject(error)
  }

  const reusablePorts = (preauthorizedPorts ?? []).filter(isFluxPurrUsbSerialPort)
  const reusedPort = forcePortSelection
    ? null
    : reusablePorts.length === 1
      ? reusablePorts[0]
      : null
  if (reusedPort) {
    onTrace(
      createTraceEvent(
        now,
        '浏览器 USB 已复用授权端口',
        '已复用本站点唯一已授权的 ESP32-S3 USB JTAG/serial 端口；未调用 requestPort()。',
        'success'
      )
    )
    return Promise.resolve(reusedPort)
  }

  let selection: Promise<BrowserSerialPort>
  try {
    // This call must stay in the click stack. Any await before it removes the
    // transient activation required for Chrome to display the chooser. The
    // authorization cache above was obtained before this click through
    // navigator.serial.getPorts(), so an absent or ambiguous port must fall
    // back to the native chooser here.
    selection = selectBrowserSerialPort(serial)
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
          selectionReason === 'change_port' ? '浏览器 USB 端口已更换' : '浏览器 USB 端口已选择',
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
      selectionReason === 'change_port'
        ? '浏览器 USB 选择器已请求更换端口'
        : '浏览器 USB 选择器已请求',
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
