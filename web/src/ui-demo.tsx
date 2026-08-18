import { ControlPlaneDemo } from '@/features/control-plane-demo/components/control-plane-demo'
import { LanPairingPanel } from '@/features/control-plane-demo/components/lan-pairing-panel'
import type { LanProbe } from '@/features/control-plane-demo/lan-client'
import { liveControlPlaneScenario } from '@/features/control-plane-demo/live-scenario'
import { controlPlaneScenario } from '@/features/control-plane-demo/mock-data'
import type { FirmwareActivityEntry, OfficialFirmwareArtifact } from '@/features/firmware-installer'

type FirmwareDemoArtifact = Pick<
  OfficialFirmwareArtifact,
  'id' | 'version' | 'channel' | 'target' | 'publishedAt'
>

function demoHex(value: string, length: number) {
  const encoded = Array.from(value, (character) => character.charCodeAt(0).toString(16)).join('')
  return encoded.padEnd(length, '0').slice(0, length)
}

const firmwareDemoArtifacts: OfficialFirmwareArtifact[] = (
  [
    {
      id: 'rc-1.5.0-rc.3',
      version: 'v1.5.0-rc.3',
      channel: 'rc',
      target: 'ESP32-S3FH4R2',
      publishedAt: '2026-08-15T09:20:00Z',
    },
    {
      id: 'rc-1.5.0-rc.2',
      version: 'v1.5.0-rc.2',
      channel: 'rc',
      target: 'ESP32-S3FH4R2',
      publishedAt: '2026-08-11T15:40:00Z',
    },
    {
      id: 'rc-1.5.0-rc.1',
      version: 'v1.5.0-rc.1',
      channel: 'rc',
      target: 'ESP32-S3FH4R2',
      publishedAt: '2026-08-08T08:00:00Z',
    },
    {
      id: 'stable-1.4.2',
      version: 'v1.4.2',
      channel: 'stable',
      target: 'ESP32-S3FH4R2',
      publishedAt: '2026-07-20T08:00:00Z',
    },
    {
      id: 'stable-1.4.1',
      version: 'v1.4.1',
      channel: 'stable',
      target: 'ESP32-S3FH4R2',
      publishedAt: '2026-06-14T08:00:00Z',
    },
    {
      id: 'rc-1.4.0-rc.2',
      version: 'v1.4.0-rc.2',
      channel: 'rc',
      target: 'ESP32-S3FH4R2',
      publishedAt: '2026-05-29T10:10:00Z',
    },
    {
      id: 'stable-1.4.0',
      version: 'v1.4.0',
      channel: 'stable',
      target: 'ESP32-S3FH4R2',
      publishedAt: '2026-05-18T09:15:00Z',
    },
    {
      id: 'stable-1.3.3',
      version: 'v1.3.3',
      channel: 'stable',
      target: 'ESP32-S3FH4R2',
      publishedAt: '2026-04-26T08:25:00Z',
    },
    {
      id: 'stable-1.3.2',
      version: 'v1.3.2',
      channel: 'stable',
      target: 'ESP32-S3FH4R2',
      publishedAt: '2026-04-09T11:35:00Z',
    },
    {
      id: 'rc-1.3.1-rc.1',
      version: 'v1.3.1-rc.1',
      channel: 'rc',
      target: 'ESP32-S3FH4R2',
      publishedAt: '2026-03-23T16:20:00Z',
    },
    {
      id: 'stable-1.3.1',
      version: 'v1.3.1',
      channel: 'stable',
      target: 'ESP32-S3FH4R2',
      publishedAt: '2026-03-14T07:45:00Z',
    },
    {
      id: 'stable-1.3.0',
      version: 'v1.3.0',
      channel: 'stable',
      target: 'ESP32-S3FH4R2',
      publishedAt: '2026-02-26T12:30:00Z',
    },
    {
      id: 'stable-1.2.4',
      version: 'v1.2.4',
      channel: 'stable',
      target: 'ESP32-S3FH4R2',
      publishedAt: '2026-02-08T09:50:00Z',
    },
    {
      id: 'stable-1.2.3',
      version: 'v1.2.3',
      channel: 'stable',
      target: 'ESP32-S3FH4R2',
      publishedAt: '2026-01-18T14:05:00Z',
    },
  ] satisfies FirmwareDemoArtifact[]
).map((artifact) => ({
  ...artifact,
  source: 'release',
  releaseTag: `demo-${artifact.id}`,
  sourceSha: demoHex(`${artifact.id}:source`, 40),
  buildId: demoHex(`${artifact.id}:build`, 16),
  bundleSha256: `sha256:${demoHex(`${artifact.id}:bundle`, 64)}`,
  assetPath: `firmware/releases/demo-${artifact.id}/flux-purr-${artifact.version}.fluxpurr-fw`,
}))

const firmwareDemoActivity: FirmwareActivityEntry[] = [
  {
    id: 'firmware-demo-1',
    time: '20:19:03',
    event: '演示环境',
    detail: '已载入固件维护样本；不会连接或写入真实设备。',
    tone: 'info',
  },
  {
    id: 'firmware-demo-2',
    time: '20:19:07',
    event: '发布目录',
    detail: '目录中有 14 个演示版本，按发布时间倒序显示。',
    tone: 'success',
  },
  {
    id: 'firmware-demo-3',
    time: '20:19:11',
    event: '任务范围',
    detail: '更新保留配置；安装或恢复按完整擦除策略演示。',
    tone: 'info',
  },
  {
    id: 'firmware-demo-4',
    time: '20:19:15',
    event: '传输引擎',
    detail: '浏览器 USB ROM 为当前演示路径；本机 devd 未启用。',
    tone: 'warning',
  },
  {
    id: 'firmware-demo-5',
    time: '20:19:19',
    event: '安全预检',
    detail: '演示数据保留芯片安全、布局与配置验证阶段。',
    tone: 'info',
  },
  {
    id: 'firmware-demo-6',
    time: '20:19:23',
    event: '候选版本',
    detail: 'RC 默认隐藏，开启后与稳定版按相同时间线排列。',
    tone: 'info',
  },
  {
    id: 'firmware-demo-7',
    time: '20:19:27',
    event: '本地文件',
    detail: '本地 .fluxpurr-fw 仍执行结构、哈希和目标校验。',
    tone: 'success',
  },
  {
    id: 'firmware-demo-8',
    time: '20:19:31',
    event: '等待任务',
    detail: '选择任务后可继续模拟完整预检，不会触发真实烧录。',
    tone: 'info',
  },
  {
    id: 'firmware-demo-9',
    time: '20:19:35',
    event: 'ROM 引导',
    detail: '演示事务保留 BOOT、RESET 与重新连接提示，不会操作任何浏览器 USB 端口。',
    tone: 'info',
  },
  {
    id: 'firmware-demo-10',
    time: '20:19:39',
    event: '芯片探测',
    detail: '此处模拟 ESP32-S3 芯片与 4 MiB Flash 容量核对的可审计日志记录。',
    tone: 'success',
  },
  {
    id: 'firmware-demo-11',
    time: '20:19:43',
    event: '安全响应',
    detail: '演示安全记录显示为可继续；真实事务仍会对未知、加密与安全启动状态阻止写入。',
    tone: 'warning',
  },
  {
    id: 'firmware-demo-12',
    time: '20:19:47',
    event: '布局验证',
    detail: '分区表与三段镜像地址按 bundle manifest 的固定布局进行逐项校验。',
    tone: 'info',
  },
  {
    id: 'firmware-demo-13',
    time: '20:19:51',
    event: '配置策略',
    detail: '更新任务保全有效 flux_cfg；安装或恢复仅擦除 MCU internal Flash。',
    tone: 'info',
  },
  {
    id: 'firmware-demo-14',
    time: '20:19:55',
    event: '运行时门禁',
    detail: '更新模式需要已验证的运行时、停热状态以及不高于 40 C 的有效温度。',
    tone: 'warning',
  },
  {
    id: 'firmware-demo-15',
    time: '20:19:59',
    event: '写入校验',
    detail: '真实流程只会在三段写入和 ROM MD5 均通过后继续进行运行时重连验证。',
    tone: 'success',
  },
  {
    id: 'firmware-demo-16',
    time: '20:20:03',
    event: '本地诊断',
    detail: '失败报告由操作者手动下载；演示不会上传诊断、配置原始字节或任何凭据。',
    tone: 'info',
  },
]

const mockProbe: LanProbe = {
  identity: {
    deviceId: '001122334455',
    firmwareVersion: 'fw/v0.4.0',
    buildId: 's3-lan-demo',
    gitSha: 'demo',
    board: 'esp32s3_frontpanel',
    apiVersion: 'v1',
    protocolVersion: 'flux-purr.usb.v1',
    hostname: 'flux-purr-001122334455',
    capabilities: ['status', 'runtime', 'lan_http'],
  },
  network: { state: 'connected', ip: '192.168.1.18', wifiRssi: -48 },
  status: {
    mode: 'idle',
    uptimeSeconds: 3723,
    currentTempC: 25.4,
    targetTempC: 120,
    heaterEnabled: false,
    heaterOutputPercent: 0,
    activeCoolingEnabled: true,
    fanDisplayState: 'AUTO',
    fanEnabled: true,
    fanPwmPermille: 400,
    voltageMv: 20000,
    currentMa: 0,
    boardTempCenti: 2840,
    pdRequestMv: 20000,
    pdContractMv: 20000,
    pdState: 'ready',
    calibration: {
      mode: 'off',
      ppsEnabled: false,
      heaterEnabled: false,
      stable: false,
      job: { status: 'idle', progressPercent: 0, samplesCollected: 0 },
    },
    network: { state: 'connected' },
  },
}

export function UiDemo() {
  const params = new URLSearchParams(window.location.search)
  const demo = params.get('uiDemo')

  if (demo === 'firmware-workspace') {
    const noDeviceSelected = params.get('state') === 'unselected'
    return (
      <ControlPlaneDemo
        scenario={noDeviceSelected ? liveControlPlaneScenario : controlPlaneScenario}
        initialView={params.get('workspace') === 'firmware' ? 'update' : 'dashboard'}
        allowDemoControls={!noDeviceSelected}
        devd={{ enabled: false }}
        firmwareArtifacts={firmwareDemoArtifacts}
        initialFirmwareActivity={firmwareDemoActivity}
        webSerial={{ enabled: false }}
      />
    )
  }

  if (demo !== 'lan-pairing' && demo !== 'true') {
    return null
  }
  return (
    <main className="industrial-ui-demo" aria-label="LAN pairing demo">
      <LanPairingPanel
        initialAddress="http://192.168.1.18"
        pairDevice={async (address, code) => {
          if (code !== '4827') throw new Error('配对码无效。')
          return {
            session: {
              baseUrl: address.replace(/\/$/, ''),
              token: 'a'.repeat(64),
              hostname: mockProbe.identity.hostname,
            },
            probe: mockProbe,
          }
        }}
      />
    </main>
  )
}
