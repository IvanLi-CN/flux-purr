import { createElement } from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { describe, expect, it } from 'vitest'

import { FirmwareTransactionLog, FirmwareWorkbench } from './firmware-workbench'
import { devdFirmwareResponseMessage, resolveCatalogSelection } from './firmware-workbench-logic'

describe('devd firmware error contracts', () => {
  it('preserves the structured service error for the firmware trace', () => {
    expect(
      devdFirmwareResponseMessage(
        { status: 403 },
        {
          error: {
            code: 'rom_security_unknown',
            message: 'ROM security response is unknown.',
          },
        },
        'devd preflight failed'
      )
    ).toBe('ROM security response is unknown.')
  })

  it('falls back to the service code when a response omits its message', () => {
    expect(
      devdFirmwareResponseMessage(
        { status: 409 },
        { error: { code: 'lease_conflict' } },
        'devd preflight failed'
      )
    ).toBe('lease_conflict')
  })
})

describe('FirmwareWorkbench task and target scope', () => {
  it('keeps both tasks available before devd health and target discovery complete', () => {
    const markup = renderToStaticMarkup(
      createElement(FirmwareWorkbench, {
        browserAvailable: true,
        officialArtifacts: [
          {
            id: 'stable-1.4.2',
            version: 'v1.4.2',
            channel: 'stable',
            source: 'release',
            releaseTag: 'v1.4.2',
            sourceSha: 'a'.repeat(40),
            buildId: 'a'.repeat(16),
            assetPath: '/firmware/releases/v1.4.2-demo/flux-purr-v1.4.2.fluxpurr-fw',
            bundleSha256: `sha256:${'a'.repeat(64)}`,
            target: 'ESP32-S3FH4R2',
            publishedAt: '2026-07-20T08:00:00Z',
          },
          {
            id: 'rc-1.5.0-rc.1',
            version: 'v1.5.0-rc.1',
            channel: 'rc',
            source: 'release',
            releaseTag: 'v1.5.0-rc.1',
            sourceSha: 'b'.repeat(40),
            buildId: 'b'.repeat(16),
            assetPath: '/firmware/releases/v1.5.0-rc.1-demo/flux-purr-v1.5.0-rc.1.fluxpurr-fw',
            bundleSha256: `sha256:${'b'.repeat(64)}`,
            target: 'ESP32-S3FH4R2',
            publishedAt: '2026-08-08T08:00:00Z',
          },
        ],
        nativeTargets: [
          {
            id: 'devd-bench-a',
            label: '基准治具 A',
            detail: '/dev/cu.usbmodem101 · 租约就绪',
            leaseId: 'lease-a',
            updateEligible: true,
            currentTemperatureC: 25,
            heaterEnabled: false,
          },
        ],
      })
    )

    expect(markup).toContain('更新现有设备')
    expect(markup).toContain('安装或恢复')
    expect(markup).toContain('选择连接引擎和固件来源后运行完整预检。')
    expect(markup).toContain('aria-pressed="true"')
    expect(markup).not.toContain('本机固件目标')
    expect(markup).not.toContain('基准治具 A · /dev/cu.usbmodem101 · 租约就绪')
    expect(markup).toContain('固件包')
    expect(markup).toContain('选择固件包')
    expect(markup).toContain('Browser USB ROM 引导')
    expect(markup).toContain('选择 / 更换浏览器 USB 端口')
  })

  it('explains an empty published catalog instead of rendering a blank firmware entry', () => {
    const markup = renderToStaticMarkup(
      createElement(FirmwareWorkbench, {
        browserAvailable: true,
        officialArtifacts: [],
        nativeTargets: [],
      })
    )

    expect(markup).toContain('发布目录中没有可用固件包')
  })

  it('selects the newest local build when development has no stable release mirror', () => {
    const markup = renderToStaticMarkup(
      createElement(FirmwareWorkbench, {
        browserAvailable: true,
        officialArtifacts: [
          {
            id: 'local-0.16.4',
            version: '0.16.4-dev.79d0dd2',
            channel: 'local',
            source: 'local',
            releaseTag: null,
            sourceSha: 'c'.repeat(40),
            buildId: 'c'.repeat(16),
            assetPath: '/firmware/releases/local/flux-purr.fluxpurr-fw',
            bundleSha256: `sha256:${'c'.repeat(64)}`,
            target: 'ESP32-S3FH4R2',
            publishedAt: '2026-08-18T08:00:00Z',
          },
        ],
        nativeTargets: [],
      })
    )

    expect(markup).toContain('0.16.4-dev.79d0dd2')
    expect(markup).toContain('发布版本 · 本地构建')
  })

  it('uses the shared scroll area for the bounded firmware transaction log', () => {
    const markup = renderToStaticMarkup(
      createElement(FirmwareTransactionLog, {
        entries: [
          {
            id: 'firmware-log-1',
            time: '12:00:00',
            event: '浏览器 USB 端口已选择',
            detail: '选择器已确认串口设备。',
            tone: 'success',
          },
        ],
      })
    )

    expect(markup).toContain('data-slot="scroll-area"')
    expect(markup).toContain('aria-label="固件事务日志条目"')
    expect(markup).toContain('浏览器 USB 端口已选择')
  })
})

describe('FirmwareWorkbench catalog selection', () => {
  const artifact = (id: string, publishedAt: string) => ({
    id,
    version: `0.18.3-dev.${id}`,
    channel: 'local' as const,
    source: 'local' as const,
    releaseTag: null,
    sourceSha: id.padEnd(40, 'a').slice(0, 40),
    buildId: id.padEnd(16, 'a').slice(0, 16),
    assetPath: `/firmware/releases/${id}/flux-purr-current.fluxpurr-fw`,
    bundleSha256: `sha256:${id.padEnd(64, 'a').slice(0, 64)}`,
    target: 'ESP32-S3FH4R2' as const,
    publishedAt,
  })

  it('adopts the newest local build after a catalog refresh until an operator chooses a version', () => {
    const oldBuild = artifact('old', '2026-08-20T07:00:00Z')
    const currentBuild = artifact('current', '2026-08-20T08:00:00Z')

    expect(resolveCatalogSelection([oldBuild, currentBuild], oldBuild.id, false)?.id).toBe(
      currentBuild.id
    )
    expect(resolveCatalogSelection([oldBuild, currentBuild], oldBuild.id, true)?.id).toBe(
      oldBuild.id
    )
  })
})
