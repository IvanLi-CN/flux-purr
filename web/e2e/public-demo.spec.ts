import { expect, type Page, test } from '@playwright/test'

async function selectInspectorOption(page: Page, label: string, value: string) {
  await page.getByRole('combobox', { name: label }).click()
  await page.locator(`[role="option"][data-value="${value}"]`).click()
}

test.describe('public demo build', () => {
  test('forces the complete mock console, preserves scene state, and never reaches devd', async ({
    page,
  }) => {
    const controlRequests: string[] = []
    page.on('request', (request) => {
      if (/127\.0\.0\.1:30080|192\.168\.1\.77|\/api\/v1\//.test(request.url())) {
        controlRequests.push(request.url())
      }
    })
    await page.addInitScript(() => {
      const serialCalls = { getPorts: 0, requestPort: 0 }
      Object.defineProperty(window, '__publicDemoSerialCalls', {
        configurable: true,
        value: serialCalls,
      })
      Object.defineProperty(navigator, 'serial', {
        configurable: true,
        value: {
          getPorts: () => {
            serialCalls.getPorts += 1
            return Promise.resolve([])
          },
          requestPort: () => {
            serialCalls.requestPort += 1
            return Promise.resolve(null)
          },
        },
      })
      window.localStorage.setItem(
        'flux-purr:lan-device:http://192.168.1.77',
        JSON.stringify({
          baseUrl: 'http://192.168.1.77',
          token: 'a'.repeat(64),
          deviceId: '001122334477',
          hostname: 'saved-lan-target',
        })
      )
    })

    await page.goto('/?demo=false&uiDemo=true')
    await expect(page).toHaveURL(/\/devices\/fp-lab-01\/overview$/)
    await expect(page.getByRole('heading', { name: '热控工作台' })).toBeVisible()
    await expect(page.getByRole('complementary', { name: 'Demo Inspector' })).toBeVisible()
    await expect(page.getByLabel('LAN pairing demo')).toHaveCount(0)
    expect(await page.locator('script[src="/@vite/client"]').count()).toBe(0)

    await page.getByRole('button', { name: '打开 Demo Inspector' }).click()
    await page.getByRole('button', { name: 'Simulate thermal warning' }).click()
    await expect(
      page.getByText('simulated thermal warning latched on current target').last()
    ).toBeVisible()

    await selectInspectorOption(page, 'Current scene', 'degraded')
    await expect(page).toHaveURL(
      /\/devices\/fp-kit-02\/overview\?(?=.*demoScene=degraded)(?=.*demoLease=conflict)(?=.*demoNetwork=timeout)/
    )
    await expect(page.getByText('Simulate lease conflict')).toBeVisible()
    expect(controlRequests).toEqual([])
    expect(
      await page.evaluate(
        () => (window as unknown as { __publicDemoSerialCalls: unknown }).__publicDemoSerialCalls
      )
    ).toEqual({ getPorts: 0, requestPort: 0 })
  })

  test('serializes concurrent inspector changes into one share state', async ({ page }) => {
    await page.goto('/')
    await page.getByRole('button', { name: '打开 Demo Inspector' }).click()

    await page.getByRole('checkbox', { name: 'Simulate lease conflict' }).evaluate((lease) => {
      const network = Array.from(
        document.querySelectorAll<HTMLInputElement>('input[type="checkbox"]')
      ).find((input) => input.parentElement?.textContent?.includes('Simulate network timeout'))
      ;(lease as HTMLInputElement).click()
      network?.click()
    })

    await expect.poll(() => new URL(page.url()).searchParams.get('demoLease')).toBe('conflict')
    await expect.poll(() => new URL(page.url()).searchParams.get('demoNetwork')).toBe('timeout')
  })

  test('shares initial conditions and resets all in-memory demo results', async ({ page }) => {
    let copiedUrl = ''
    await page.addInitScript(() => {
      Object.defineProperty(navigator, 'clipboard', {
        configurable: true,
        value: {
          writeText: (value: string) => {
            ;(window as unknown as { __copiedDemoUrl: string }).__copiedDemoUrl = value
            return Promise.resolve()
          },
        },
      })
    })
    await page.goto('/devices/fp-lab-01/overview?demoScene=degraded&demoLease=conflict')
    await page.getByRole('button', { name: '打开 Demo Inspector' }).click()

    await selectInspectorOption(page, 'WiFi capability', 'unavailable')
    await page.getByRole('button', { name: 'Simulate thermal warning' }).click()
    await expect(
      page.getByText('simulated thermal warning latched on current target')
    ).toBeVisible()
    await expect(page).toHaveURL(
      /\/devices\/fp-lab-01\/overview\?(?=.*demoScene=degraded)(?=.*demoLease=conflict)(?=.*demoWifiCapability=unavailable)/
    )

    await page.getByRole('button', { name: '复制 Demo 分享链接' }).click()
    await expect(page.getByText('Copied initial scene URL')).toBeVisible()
    copiedUrl = await page.evaluate(
      () => (window as unknown as { __copiedDemoUrl: string }).__copiedDemoUrl
    )
    expect(copiedUrl).toContain('demoScene=degraded')
    expect(copiedUrl).not.toContain('thermal')

    await page.getByRole('button', { name: 'Reset current case' }).click()
    await expect(page).toHaveURL(/\/devices\/fp-lab-01\/overview$/)
    await expect(page.getByText('simulated thermal warning latched on current target')).toHaveCount(
      0
    )
    await expect(page.getByRole('combobox', { name: 'WiFi capability' })).toContainText(
      'Default capability'
    )
    await expect(page.getByRole('checkbox', { name: 'Simulate lease conflict' })).not.toBeChecked()
  })

  test('shares persona permissions and records simulated mutations', async ({ page }) => {
    await page.goto('/')
    await page.getByRole('button', { name: '打开 Demo Inspector' }).click()

    await selectInspectorOption(page, 'Persona', 'maintainer')
    await expect(page).toHaveURL(/\?demoPersona=maintainer$/)
    await page.getByRole('button', { name: '查看当前 Persona 权限' }).click()
    await expect(page.getByRole('note', { name: 'Persona permissions' })).toContainText(
      'Configure · Calibrate · Inspect'
    )
    await page.getByRole('button', { name: 'Simulate thermal warning' }).click()
    await expect(page.getByText(/#1 · flux-purr-001122334455/)).toBeVisible()

    await selectInspectorOption(page, 'Persona', 'observer')
    await expect(page.getByRole('button', { name: 'Simulate thermal warning' })).toBeDisabled()
    await expect(page.getByText('Observer permissions keep actions read-only.')).toBeVisible()
  })

  test('reset atomically closes an active calibration guard', async ({ page }) => {
    await page.goto('/devices/fp-lab-01/calibration/rtd-adc?demo=true')
    await page.getByRole('switch', { name: '标定模式' }).click()
    await expect(page.getByRole('switch', { name: '标定模式' })).toBeChecked()
    await page.getByRole('button', { name: '打开 Demo Inspector' }).click()

    await page.getByRole('button', { name: 'Reset current case' }).click()

    await expect(page).toHaveURL('/devices/fp-lab-01/calibration/rtd-adc')
    await expect(page.getByRole('switch', { name: '标定模式' })).not.toBeChecked()
    await expect(page.getByRole('dialog', { name: '校准未关闭' })).toHaveCount(0)
    await expect(page.getByRole('combobox', { name: 'Current scene' })).toContainText('Normal')
  })

  test('prioritizes reset over a blocked Inspector navigation', async ({ page }) => {
    await page.goto('/devices/fp-lab-01/calibration/rtd-adc?demo=true')
    await page.getByRole('switch', { name: '标定模式' }).click()
    await expect(page.getByRole('switch', { name: '标定模式' })).toBeChecked()
    await page.getByRole('button', { name: '打开 Demo Inspector' }).click()

    await selectInspectorOption(page, 'Current target', 'fp-kit-02')
    await expect(page.getByRole('dialog', { name: '校准未关闭' })).toBeVisible()

    await page.getByRole('button', { name: 'Reset current case' }).click()

    await expect(page).toHaveURL('/devices/fp-lab-01/calibration/rtd-adc')
    await expect(page.getByRole('switch', { name: '标定模式' })).not.toBeChecked()
    await expect(page.getByRole('dialog', { name: '校准未关闭' })).toHaveCount(0)
    await expect(page.getByRole('combobox', { name: 'Current scene' })).toContainText('Normal')
  })

  test('reports clipboard refusal without a false success', async ({ page }) => {
    await page.addInitScript(() => {
      Object.defineProperty(navigator, 'clipboard', {
        configurable: true,
        value: { writeText: () => Promise.reject(new Error('clipboard denied')) },
      })
    })
    await page.goto('/')
    await page.getByRole('button', { name: '打开 Demo Inspector' }).click()
    await page.getByRole('button', { name: '复制 Demo 分享链接' }).click()

    await expect(page.getByText('Copy failed; select the URL manually')).toBeVisible()
    await expect(page.getByText('Copied initial scene URL')).toHaveCount(0)
    await expect(page.getByLabel('当前 Demo 分享链接')).toHaveValue(
      /\/devices\/fp-lab-01\/overview$/
    )
  })

  test('resets the current device and page while rebuilding scene-local results', async ({
    page,
  }) => {
    await page.goto('/devices/fp-kit-02/settings/presets?demoScene=degraded')
    await page.getByRole('button', { name: '打开 Demo Inspector' }).click()
    await page.getByRole('button', { name: 'Reset current case' }).click()

    await expect(page).toHaveURL('/devices/fp-kit-02/settings/presets')
    await expect(page.getByRole('combobox', { name: 'Current scene' })).toContainText('Normal')

    await page.goto('/devices/fp-lab-01/overview')
    await page.getByRole('button', { name: '打开 Demo Inspector' }).click()
    await page.getByRole('button', { name: 'Simulate thermal warning' }).click()
    await expect(
      page.getByText('simulated thermal warning latched on current target')
    ).toBeVisible()
    await selectInspectorOption(page, 'Current scene', 'degraded')
    await selectInspectorOption(page, 'Current scene', 'normal')
    await expect(page.getByText('simulated thermal warning latched on current target')).toHaveCount(
      0
    )
  })

  test('keeps scene routing and overrides aligned during concurrent changes', async ({ page }) => {
    await page.goto('/')
    await page.getByRole('button', { name: '打开 Demo Inspector' }).click()

    await selectInspectorOption(page, 'Current scene', 'degraded')
    await page.getByRole('checkbox', { name: 'Simulate lease conflict' }).click()

    await expect(page).toHaveURL(
      /\/devices\/fp-kit-02\/overview\?(?=.*demoScene=degraded)(?=.*demoNetwork=timeout)/
    )
  })

  test('preserves queued overrides when selecting a target', async ({ page }) => {
    await page.goto('/')
    await page.getByRole('button', { name: '打开 Demo Inspector' }).click()

    await page.getByRole('checkbox', { name: 'Simulate lease conflict' }).click()
    await selectInspectorOption(page, 'Current target', 'fp-kit-02')

    await expect(page).toHaveURL(/\/devices\/fp-kit-02\/overview\?demoLease=conflict$/)
  })

  test('keeps the exact calibration subroute when share state changes', async ({ page }) => {
    await page.goto('/devices/fp-lab-01/calibration/rtd-adc?demo=true')
    await page.getByRole('button', { name: '打开 Demo Inspector' }).click()
    await page.getByRole('checkbox', { name: 'Simulate lease conflict' }).check()

    await expect(page).toHaveURL(/\/devices\/fp-lab-01\/calibration\/rtd-adc\?demoLease=conflict$/)
  })

  test('uses a mobile bubble that opens a touch-safe drawer', async ({ page }) => {
    await page.setViewportSize({ width: 375, height: 812 })
    await page.goto('/')
    const openButton = page.getByRole('button', { name: '打开 Demo Inspector' })
    await expect(openButton).toBeVisible()
    expect((await openButton.boundingBox())?.height).toBeGreaterThanOrEqual(48)
    await openButton.click()
    await expect(page.getByRole('dialog', { name: 'Demo Inspector' })).toBeVisible()
    const reset = page.getByRole('button', { name: 'Reset current case' })
    expect((await reset.boundingBox())?.height).toBeGreaterThanOrEqual(48)

    for (const button of [
      page.getByRole('button', { name: '收起 Demo Inspector' }),
      page.getByRole('button', { name: '复制 Demo 分享链接' }),
    ]) {
      const box = await button.boundingBox()
      expect(box?.height).toBeGreaterThanOrEqual(48)
      expect(box?.width).toBeGreaterThanOrEqual(48)
    }
    const viewport = await page.evaluate(() => ({
      viewportWidth: window.innerWidth,
      viewportHeight: window.innerHeight,
      documentScrollHeight: document.documentElement.scrollHeight,
      bodyScrollHeight: document.body.scrollHeight,
      scrollWidth: document.documentElement.scrollWidth,
    }))
    expect(viewport.scrollWidth).toBeLessThanOrEqual(viewport.viewportWidth)
    expect(viewport.documentScrollHeight).toBeLessThanOrEqual(viewport.viewportHeight)
    expect(viewport.bodyScrollHeight).toBeLessThanOrEqual(viewport.viewportHeight)
  })

  test('keeps the fixed desktop workstation at its natural width before the dock threshold', async ({
    page,
  }) => {
    await page.setViewportSize({ width: 1440, height: 1000 })
    await page.goto('/')

    await expect(page.getByRole('button', { name: '打开 Demo Inspector' })).toBeVisible()
    const layout = await page.evaluate(() => {
      const console = document.querySelector('.industrial-console')?.getBoundingClientRect()
      const wrap = document.querySelector('.industrial-console-wrap')
      return {
        consoleWidth: console?.width ?? 0,
        paddingRight: Number.parseFloat(getComputedStyle(wrap).paddingRight),
        scrollWidth: document.documentElement.scrollWidth,
        viewportWidth: window.innerWidth,
      }
    })

    expect(layout.consoleWidth).toBeGreaterThanOrEqual(1279)
    expect(layout.consoleWidth).toBeLessThanOrEqual(1281)
    expect(layout.paddingRight).toBeLessThan(100)
    expect(layout.scrollWidth).toBeLessThanOrEqual(layout.viewportWidth)

    await page.getByRole('button', { name: '打开 Demo Inspector' }).click()
    await expect(page.getByRole('complementary', { name: 'Demo Inspector' })).toBeVisible()
    const bottomLayout = await page.evaluate(() => {
      const console = document.querySelector('.industrial-console')?.getBoundingClientRect()
      const inspector = document.querySelector('.demo-inspector')?.getBoundingClientRect()
      const wrap = document.querySelector('.industrial-console-wrap')
      return {
        consoleRight: console?.right ?? 0,
        inspectorLeft: inspector?.left ?? 0,
        inspectorRight: inspector?.right ?? 0,
        placement: document.querySelector('.demo-inspector')?.getAttribute('data-placement'),
        paddingLeft: Number.parseFloat(getComputedStyle(wrap).paddingLeft),
        paddingRight: Number.parseFloat(getComputedStyle(wrap).paddingRight),
        documentScrollHeight: document.documentElement.scrollHeight,
        bodyScrollHeight: document.body.scrollHeight,
        bottomColumnCount: document.querySelectorAll('.demo-inspector__bottom-column').length,
        bottomColumnMetrics: Array.from(
          document.querySelectorAll('.demo-inspector__bottom-column')
        ).map((column) => {
          const content = column.querySelector('.simplebar-content')
          const permissionHelp = column.querySelector(
            '.demo-inspector__help--permission .demo-inspector__help-trigger'
          )
          const shareCopy = column.querySelector<HTMLButtonElement>(
            'button[aria-label="复制 Demo 分享链接"]'
          )
          return {
            contentScrollHeight: content?.scrollHeight ?? 0,
            viewportClientHeight:
              column.querySelector('.simplebar-content-wrapper')?.clientHeight ?? 0,
            wrapperRight:
              column.querySelector('.simplebar-content-wrapper')?.getBoundingClientRect().right ??
              0,
            contentRight: content?.getBoundingClientRect().right ?? 0,
            permissionHelpRight: permissionHelp?.getBoundingClientRect().right ?? null,
            shareCopyRight: shareCopy?.getBoundingClientRect().right ?? null,
          }
        }),
        scrollWidth: document.documentElement.scrollWidth,
        viewportWidth: window.innerWidth,
        viewportHeight: window.innerHeight,
      }
    })
    expect(bottomLayout.placement).toBe('bottom')
    expect(bottomLayout.inspectorLeft).toBe(0)
    expect(bottomLayout.inspectorRight).toBe(1440)
    expect(bottomLayout.paddingLeft).toBeLessThan(100)
    expect(bottomLayout.paddingRight).toBeLessThan(100)
    expect(bottomLayout.documentScrollHeight).toBeLessThanOrEqual(bottomLayout.viewportHeight)
    expect(bottomLayout.bodyScrollHeight).toBeLessThanOrEqual(bottomLayout.viewportHeight)
    expect(bottomLayout.bottomColumnCount).toBe(4)
    expect(bottomLayout.bottomColumnMetrics).toHaveLength(4)
    expect(
      bottomLayout.bottomColumnMetrics.every(
        ({ contentScrollHeight, viewportClientHeight }) =>
          contentScrollHeight >= viewportClientHeight
      )
    ).toBe(true)
    const permissionColumn = bottomLayout.bottomColumnMetrics.find(
      ({ permissionHelpRight }) => permissionHelpRight !== null
    )
    expect(permissionColumn?.permissionHelpRight).not.toBeNull()
    expect(bottomLayout.bottomColumnMetrics[3].shareCopyRight).not.toBeNull()
    expect(
      (permissionColumn?.wrapperRight ?? 0) - (permissionColumn?.permissionHelpRight ?? 0)
    ).toBeGreaterThanOrEqual(6)
    expect(
      bottomLayout.bottomColumnMetrics[3].wrapperRight -
        (bottomLayout.bottomColumnMetrics[3].shareCopyRight ?? 0)
    ).toBeGreaterThanOrEqual(6)
    expect(bottomLayout.scrollWidth).toBeLessThanOrEqual(bottomLayout.viewportWidth)

    await page.setViewportSize({ width: 1700, height: 1000 })
    await page.goto('/')
    await expect(page.getByRole('heading', { name: 'Demo Inspector' })).toBeVisible()
    const dockedLayout = await page.evaluate(() => {
      const console = document.querySelector('.industrial-console')?.getBoundingClientRect()
      const inspector = document.querySelector('.demo-inspector')?.getBoundingClientRect()
      const wrap = document.querySelector('.industrial-console-wrap')
      return {
        consoleWidth: console?.width ?? 0,
        consoleRight: console?.right ?? 0,
        inspectorLeft: inspector?.left ?? 0,
        paddingRight: Number.parseFloat(getComputedStyle(wrap).paddingRight),
        documentScrollHeight: document.documentElement.scrollHeight,
        bodyScrollHeight: document.body.scrollHeight,
        bottomColumnCount: document.querySelectorAll('.demo-inspector__bottom-column').length,
        scrollWidth: document.documentElement.scrollWidth,
        viewportWidth: window.innerWidth,
        viewportHeight: window.innerHeight,
      }
    })

    expect(dockedLayout.consoleWidth).toBeGreaterThanOrEqual(1279)
    expect(dockedLayout.consoleWidth).toBeLessThanOrEqual(1281)
    expect(dockedLayout.paddingRight).toBeGreaterThanOrEqual(380)
    expect(dockedLayout.inspectorLeft - dockedLayout.consoleRight).toBeGreaterThanOrEqual(24)
    expect(dockedLayout.scrollWidth).toBeLessThanOrEqual(dockedLayout.viewportWidth)
  })

  test('uses a modal drawer below the desktop layout threshold', async ({ page }) => {
    await page.setViewportSize({ width: 1024, height: 900 })
    await page.goto('/')
    const trigger = page.getByRole('button', { name: '打开 Demo Inspector' })
    await trigger.click()

    const drawer = page.getByRole('dialog', { name: 'Demo Inspector' })
    await expect(drawer).toHaveAttribute('aria-modal', 'true')
    await expect(page.locator('.demo-inspector__backdrop')).toBeVisible()
    await expect
      .poll(() => page.evaluate(() => document.activeElement?.getAttribute('aria-label')))
      .toBe('收起 Demo Inspector')

    await page.keyboard.press('Escape')
    await expect(drawer).toHaveCount(0)
    await expect(trigger).toBeFocused()
  })

  test('keeps the product console intact at 1300px when the inspector opens', async ({ page }) => {
    await page.setViewportSize({ width: 1300, height: 1000 })
    await page.goto('/')
    await expect(page.getByRole('heading', { name: '热控工作台' })).toBeVisible()
    const initialConsoleWidth = await page.evaluate(
      () => document.querySelector('.industrial-console')?.getBoundingClientRect().width ?? 0
    )
    await page.getByRole('button', { name: '打开 Demo Inspector' }).click()

    const layout = await page.evaluate(() => {
      const console = document.querySelector('.industrial-console')?.getBoundingClientRect()
      const inspector = document.querySelector('.demo-inspector')?.getBoundingClientRect()
      const wrap = document.querySelector('.industrial-console-wrap')
      return {
        consoleWidth: console?.width ?? 0,
        inspectorLeft: inspector?.left ?? 0,
        inspectorRight: inspector?.right ?? 0,
        placement: document.querySelector('.demo-inspector')?.getAttribute('data-placement'),
        paddingLeft: Number.parseFloat(getComputedStyle(wrap).paddingLeft),
        paddingRight: Number.parseFloat(getComputedStyle(wrap).paddingRight),
        documentScrollHeight: document.documentElement.scrollHeight,
        bodyScrollHeight: document.body.scrollHeight,
        bottomColumnCount: document.querySelectorAll('.demo-inspector__bottom-column').length,
        scrollWidth: document.documentElement.scrollWidth,
        viewportWidth: window.innerWidth,
        viewportHeight: window.innerHeight,
      }
    })

    expect(layout.consoleWidth).toBe(initialConsoleWidth)
    expect(layout.placement).toBe('bottom')
    expect(layout.inspectorLeft).toBe(0)
    expect(layout.inspectorRight).toBe(1300)
    expect(layout.paddingLeft).toBeLessThan(100)
    expect(layout.paddingRight).toBeLessThan(100)
    expect(layout.documentScrollHeight).toBeLessThanOrEqual(layout.viewportHeight)
    expect(layout.bodyScrollHeight).toBeLessThanOrEqual(layout.viewportHeight)
    expect(layout.bottomColumnCount).toBe(4)
    expect(layout.scrollWidth).toBeLessThanOrEqual(layout.viewportWidth)
  })

  test('opens a compact layout menu from the context menu and long press', async ({ page }) => {
    await page.setViewportSize({ width: 1700, height: 900 })
    await page.goto('/')
    const inspector = page.locator('.demo-inspector')
    const closeButton = page.getByRole('button', { name: '收起 Demo Inspector' })

    await closeButton.click({ button: 'right' })
    const menu = page.getByRole('menu', { name: 'Demo Inspector 布局' })
    await expect(menu).toBeVisible()
    await expect(page.getByRole('button', { name: '设置 Demo Inspector 布局' })).toHaveCount(0)
    for (const label of ['左侧', '右侧', '底部', '悬浮']) {
      await expect(menu.getByRole('menuitemradio', { name: new RegExp(label) })).toBeVisible()
    }
    await expect(menu.getByRole('menuitem', { name: /收起/ })).toBeVisible()

    await page.locator('.industrial-console').click({ position: { x: 24, y: 24 } })
    await expect(menu).toBeHidden()
    await closeButton.click({ button: 'right' })
    await expect(menu).toBeVisible()

    await menu.getByRole('menuitemradio', { name: /左侧/ }).click()
    await expect(inspector).toHaveAttribute('data-placement', 'left')

    await closeButton.dispatchEvent('pointerdown', { pointerType: 'touch', button: 0 })
    await page.waitForTimeout(600)
    await closeButton.dispatchEvent('pointerup', { pointerType: 'touch', button: 0 })
    await expect(menu).toBeVisible()
  })

  test('disables placements that do not fit and restores the preferred placement later', async ({
    page,
  }) => {
    await page.setViewportSize({ width: 1024, height: 700 })
    await page.goto('/')
    const inspector = page.locator('.demo-inspector')
    await page.getByRole('button', { name: '打开 Demo Inspector' }).click()
    await expect(inspector).toHaveAttribute('data-placement', 'floating')
    await inspector.getByRole('button', { name: '收起 Demo Inspector' }).click({ button: 'right' })
    const menu = page.getByRole('menu', { name: 'Demo Inspector 布局' })
    await expect(menu.getByRole('menuitemradio', { name: /左侧/ })).toBeDisabled()
    await expect(menu.getByRole('menuitemradio', { name: /右侧/ })).toBeDisabled()
    await expect(menu.getByRole('menuitemradio', { name: /底部/ })).toBeDisabled()
    await expect(menu.getByRole('menuitemradio', { name: /悬浮/ })).toBeEnabled()

    await page.setViewportSize({ width: 1024, height: 900 })
    await expect(inspector).toHaveAttribute('data-placement', 'bottom')
    await expect(menu.getByRole('menuitemradio', { name: /底部/ })).toBeEnabled()

    await page.keyboard.press('Escape')
    await page.setViewportSize({ width: 1700, height: 900 })
    await expect(inspector).toHaveAttribute('data-placement', 'right')
  })

  test('left click collapses and reopens with the previous placement', async ({ page }) => {
    await page.setViewportSize({ width: 1700, height: 900 })
    await page.goto('/')
    const inspector = page.locator('.demo-inspector')
    const closeButton = page.getByRole('button', { name: '收起 Demo Inspector' })
    await closeButton.click({ button: 'right' })
    await page.getByRole('menuitemradio', { name: /左侧/ }).click()
    await closeButton.click({ button: 'right' })
    await page.getByRole('menuitem', { name: /收起/ }).click()
    const trigger = page.getByRole('button', { name: '打开 Demo Inspector' })
    await expect(trigger).toBeVisible()
    const bubblePosition = await page.evaluate(() => {
      const bubble = document.querySelector('.demo-inspector__bubble')?.getBoundingClientRect()
      return {
        right: bubble?.right ?? 0,
        bottom: bubble?.bottom ?? 0,
        viewportWidth: window.innerWidth,
        viewportHeight: window.innerHeight,
      }
    })
    expect(bubblePosition.right).toBeCloseTo(bubblePosition.viewportWidth - 16, 0)
    expect(bubblePosition.bottom).toBeCloseTo(bubblePosition.viewportHeight - 16, 0)
    await trigger.click()
    await expect(inspector).toHaveAttribute('data-placement', 'left')
  })

  test('uses English names for every demo target fixture', async ({ page }) => {
    await page.goto('/')
    await page.getByRole('button', { name: '打开 Demo Inspector' }).click()

    await page.getByRole('combobox', { name: 'Current target' }).click()
    await expect(page.getByRole('option', { name: /flux-purr-001122334455/ })).toBeVisible()
    await expect(page.getByRole('option', { name: /flux-purr-a0f262f20d6c/ })).toBeVisible()
    await expect(page.getByRole('option', { name: /flux-purr-s3-001/ })).toBeVisible()
    await page.keyboard.press('Escape')
  })

  test('keeps Inspector explanations contextual and Select focus rings inside the control', async ({
    page,
  }) => {
    await page.goto('/')
    await page.getByRole('button', { name: '打开 Demo Inspector' }).click()

    const scopeHelp = page.getByText(
      'Local fixtures only. No device or network connection is available.',
      {
        exact: true,
      }
    )
    await expect(scopeHelp).toHaveCount(0)
    await expect(page.getByRole('combobox', { name: 'Inspector layout' })).toHaveCount(0)
    await page.getByRole('button', { name: '查看 Demo Inspector 说明' }).click()
    await expect(page.getByRole('note', { name: 'Demo scope' })).toContainText(
      'Local fixtures only'
    )

    const target = page.getByRole('combobox', { name: 'Current target' })
    await target.focus()
    await expect(target).toBeFocused()
    await expect
      .poll(() => target.evaluate((element) => getComputedStyle(element).outlineOffset))
      .toBe('-2px')
  })

  test('keeps the compact Inspector help action vertically aligned', async ({ page }) => {
    await page.setViewportSize({ width: 1378, height: 1471 })
    await page.goto('/')
    await page.getByRole('button', { name: '打开 Demo Inspector' }).click()

    const sizes = await page.evaluate(() => {
      const help = document.querySelector(
        '.demo-inspector__help--header .demo-inspector__help-trigger'
      )
      const close = document.querySelector('.demo-inspector__icon-button')
      const helpBox = help?.getBoundingClientRect()
      const closeBox = close?.getBoundingClientRect()
      return {
        help: helpBox
          ? {
              width: helpBox.width,
              height: helpBox.height,
              centerY: helpBox.top + helpBox.height / 2,
            }
          : null,
        close: closeBox
          ? {
              width: closeBox.width,
              height: closeBox.height,
              centerY: closeBox.top + closeBox.height / 2,
            }
          : null,
      }
    })

    expect(sizes.help).toMatchObject({ width: 32, height: 32 })
    expect(sizes.close).toMatchObject({ width: 48, height: 48 })
    expect(sizes.help?.centerY).toBe(sizes.close?.centerY)
  })

  test('keeps mock targets usable and guards a calibration target switch', async ({ page }) => {
    await page.goto('/')
    await page.getByRole('button', { name: '打开 Demo Inspector' }).click()

    await selectInspectorOption(page, 'Current target', 'fp-kit-02')
    await expect(page).toHaveURL(/\/devices\/fp-kit-02\/overview$/)
    await expect(page.getByRole('heading', { name: '热控工作台' })).toBeVisible()
    await expect(page.getByRole('heading', { name: '目标设备暂不可用' })).toHaveCount(0)

    await page.goto('/devices/fp-lab-01/calibration/rtd-adc?demo=true')
    const calibrationMode = page.getByRole('switch', { name: '标定模式' })
    await calibrationMode.click()
    await expect(calibrationMode).toBeChecked()
    await page.getByRole('button', { name: '打开 Demo Inspector' }).click()

    await selectInspectorOption(page, 'Current target', 'fp-kit-02')
    await expect(page.getByRole('dialog', { name: '校准未关闭' })).toBeVisible()
    await expect(page).toHaveURL(/\/devices\/fp-lab-01\/calibration\/rtd-adc$/)
    await page.getByRole('button', { name: '留在当前页' }).click()
    await expect(page.getByRole('dialog', { name: '校准未关闭' })).toHaveCount(0)

    await selectInspectorOption(page, 'Current target', 'fp-kit-02')
    await page.getByRole('button', { name: '关闭并继续' }).click()
    await expect(page).toHaveURL(/\/devices\/fp-kit-02\/overview$/)
    await expect(page.getByRole('heading', { name: '热控工作台' })).toBeVisible()
    await expect(page.getByRole('heading', { name: '目标设备暂不可用' })).toHaveCount(0)
  })

  test('guards scene and target changes from the active heater curve demo', async ({ page }) => {
    await page.goto(
      '/devices/fp-lab-01/calibration/heater-curve?demo=true&demoScene=calibration-active'
    )
    await page.getByRole('button', { name: '打开 Demo Inspector' }).click()

    await selectInspectorOption(page, 'Current scene', 'normal')
    await expect(page.getByRole('dialog', { name: '校准未关闭' })).toBeVisible()
    await expect(page).toHaveURL(
      /\/devices\/fp-lab-01\/calibration\/heater-curve\?demoScene=calibration-active$/
    )
    await page.getByRole('button', { name: '留在当前页' }).click()
    await expect(page.getByRole('dialog', { name: '校准未关闭' })).toHaveCount(0)

    await selectInspectorOption(page, 'Current target', 'fp-kit-02')
    await expect(page.getByRole('dialog', { name: '校准未关闭' })).toBeVisible()
    await page.getByRole('button', { name: '关闭并继续' }).click()
    await expect(page).toHaveURL(/\/devices\/fp-kit-02\/overview$/)
  })

  test('serializes target and override changes behind the calibration leave guard', async ({
    page,
  }) => {
    await page.goto('/devices/fp-lab-01/calibration/rtd-adc?demo=true')
    const calibrationMode = page.getByRole('switch', { name: '标定模式' })
    await calibrationMode.click()
    await expect(calibrationMode).toBeChecked()
    await page.getByRole('button', { name: '打开 Demo Inspector' }).click()

    await selectInspectorOption(page, 'Current target', 'fp-kit-02')
    await page.getByRole('checkbox', { name: 'Simulate lease conflict' }).click()

    await expect(page.getByRole('dialog', { name: '校准未关闭' })).toBeVisible()
    await page.getByRole('button', { name: '关闭并继续' }).click()
    await expect(page).toHaveURL(/\/devices\/fp-kit-02\/overview\?demoLease=conflict$/)
  })

  test('renders every public demo scene through its deterministic route', async ({ page }) => {
    const scenes = [
      {
        url: '/devices/fp-lab-01/overview?demo=true',
        visibleHeading: '热控工作台',
      },
      {
        url: '/devices/fp-kit-02/overview?demo=true&demoScene=degraded',
        visibleHeading: '热控工作台',
      },
      {
        url: '/devices/fp-demo-03/overview?demo=true&demoScene=offline',
        visibleHeading: 'Choose target',
      },
      {
        url: '/devices/fp-lab-01/overview?demo=true&demoScene=blocked-artifact',
        visibleHeading: '热控工作台',
      },
      {
        url: '/devices/fp-lab-01/calibration/heater-curve?demo=true&demoScene=calibration-active',
        visibleHeading: '热控工作台',
      },
    ]

    for (const scene of scenes) {
      await page.goto(scene.url)
      await expect(page.getByRole('heading', { name: scene.visibleHeading })).toBeVisible()
    }

    await page.goto('/devices/fp-demo-03/overview?demo=true&demoScene=offline')
    await expect(page.getByText('flux-purr-s3-001', { exact: true })).toBeVisible()
    await expect(page.getByText('-54 dBm')).toHaveCount(0)

    await page.goto('/devices/fp-lab-01/update?demo=true&demoScene=blocked-artifact')
    await expect(page.getByRole('button', { name: '选择固件包' })).toBeVisible()
    await expect(page.getByRole('button', { name: '运行预检' })).toBeDisabled()
  })
})
