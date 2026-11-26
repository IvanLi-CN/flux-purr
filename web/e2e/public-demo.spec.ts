import { expect, type Page, test } from '@playwright/test'

test('Web Demo confirms tuning save despite a later record read failure', async ({ page }) => {
  await page.clock.install()
  await page.goto('/devices/fp-lab-01/calibration/firmware-tuning?demoScene=tuning-save')
  await expect(page.getByRole('button', { name: '保存', exact: true })).toBeEnabled()
  // Freeze close to "now" but with enough slack that the two CDP round trips
  // cannot let the installed clock tick past the target.
  await page.clock.pauseAt(new Date((await page.evaluate(() => Date.now())) + 2_000))
  await page.getByRole('button', { name: '保存', exact: true }).click()
  const saving = page.getByRole('button', { name: '保存中', exact: true })
  await expect(saving).toHaveAttribute('aria-busy', 'true')
  await page.clock.runFor(999)
  await expect(saving).toBeVisible()
  await page.clock.runFor(1)
  const saved = page.getByRole('button', { name: '已保存', exact: true })
  await expect(saved).toBeVisible()
  await expect(page.getByRole('status').filter({ hasText: '调优参数已保存到设备' })).toBeVisible()
  await page.clock.runFor(2_999)
  await expect(saved).toBeVisible()
  await expect(page.getByText('调优记录读取失败，已保存的参数不受影响。')).toBeVisible()
  await page.clock.runFor(1)
  await expect(page.getByRole('button', { name: '开始调优' })).toBeEnabled()
  await expect(page.getByText('上次保存结果', { exact: true })).toBeVisible()

  await page.clock.resume()
  await page.getByRole('button', { name: '打开 Demo Inspector' }).click()
  await page.getByRole('button', { name: '重置应用状态' }).click()
  await expect(page.getByRole('button', { name: '保存', exact: true })).toBeEnabled()
  await expect(page.getByText('上次保存结果', { exact: true })).toHaveCount(0)
  await expect(page.getByText('调优记录读取失败，已保存的参数不受影响。')).toHaveCount(0)
  await expect(page).toHaveURL(/\/calibration\/firmware-tuning\?demoScene=tuning-save$/)
})

test('exposes deterministic firmware tuning presets in the Demo Inspector', async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 860 })
  await page.goto('/devices/fp-lab-01/calibration/firmware-tuning')
  await page.getByRole('button', { name: '打开 Demo Inspector' }).click()

  const moduleSelect = page.getByRole('combobox', { name: 'Current demo module' })
  await expect(moduleSelect).toContainText('Tuning · Firmware tuning')

  await page.getByRole('combobox', { name: 'Current scene' }).click()
  const presetValues = await page
    .locator('[role="option"]')
    .evaluateAll((options) => options.map((option) => option.getAttribute('data-value')))
  expect(presetValues).toEqual([
    'tuning-idle',
    'tuning-active',
    'tuning-save',
    'tuning-partial',
    'tuning-unsupported',
  ])
  await page.keyboard.press('Escape')
  await expect(page.getByText('5 presets available for this module')).toBeVisible()

  await selectInspectorOption(page, 'Current scene', 'tuning-idle')
  await expect(page.getByRole('button', { name: '开始调优' })).toBeEnabled()
  await expect(page.locator('.firmware-tuning__chart-empty')).toBeVisible()
  await expect(page.locator('.firmware-tuning__facts').getByText('待命')).toBeVisible()

  await selectInspectorOption(page, 'Current scene', 'tuning-active')
  await expect(page.locator('.firmware-tuning__chart-plot svg')).toBeVisible({ timeout: 10_000 })
  await expect(page.getByRole('button', { name: '停止', exact: true })).toBeEnabled()
  await expect(page.locator('.firmware-tuning__facts')).toContainText(/warmup|approach|hold/)

  await selectInspectorOption(page, 'Current scene', 'tuning-unsupported')
  await expect(page.getByText('当前设备固件不支持调优事件协议。')).toBeVisible()

  await selectInspectorOption(page, 'Current scene', 'tuning-save')
  await expect(page.getByRole('button', { name: '保存', exact: true })).toBeEnabled()
  await expect(page.locator('.firmware-tuning__target-grid button')).toHaveCount(8)
})

async function selectInspectorOption(page: Page, label: string, value: string) {
  await page.getByRole('combobox', { name: label }).click()
  await page.locator(`[role="option"][data-value="${value}"]`).click()
}

test.describe('public demo build', () => {
  test('keeps overview feedback reachable inside the main panel at desktop sizes', async ({
    page,
  }) => {
    for (const viewport of [
      { width: 1440, height: 900, standard: true, scrollable: false },
      { width: 1280, height: 860, standard: true, scrollable: false },
      { width: 1280, height: 800, standard: false, scrollable: true },
    ]) {
      await page.setViewportSize(viewport)
      await page.goto('/devices/fp-lab-01/overview')

      const collapseInspector = page.getByRole('button', { name: '收起 Demo Inspector' })
      if (await collapseInspector.isVisible()) await collapseInspector.click()

      const panel = page.locator('.industrial-console__main')
      const shell = page.locator('.industrial-console')
      const dashboardScroll = page.locator(
        '.industrial-dashboard-scroll .simplebar-content-wrapper'
      )
      const feedback = page.locator('.industrial-action-feedback')
      await expect(panel).toBeVisible()
      await expect(feedback).toBeAttached()

      const scrollable = await dashboardScroll.evaluate(
        (element) => element.scrollHeight > element.clientHeight
      )
      expect(scrollable).toBe(viewport.scrollable)
      if (viewport.standard) {
        const shellBounds = await shell.evaluate((element) =>
          element.getBoundingClientRect().toJSON()
        )
        expect(Math.abs(shellBounds.width - 1280)).toBeLessThan(1)
        expect(Math.abs(shellBounds.height - 860)).toBeLessThan(1)
        const documentSize = await page.evaluate(() => ({
          width: document.documentElement.scrollWidth,
          height: document.documentElement.scrollHeight,
        }))
        expect(documentSize.width).toBeLessThanOrEqual(viewport.width)
        expect(documentSize.height).toBeLessThanOrEqual(viewport.height)
      }
      if (viewport.scrollable) {
        await feedback.scrollIntoViewIfNeeded()
        expect(await dashboardScroll.evaluate((element) => element.scrollTop)).toBeGreaterThan(0)
      }

      const bounds = await Promise.all([
        panel.evaluate((element) => element.getBoundingClientRect().toJSON()),
        page
          .locator('.industrial-dashboard-scroll')
          .evaluate((element) => element.getBoundingClientRect().toJSON()),
        feedback.evaluate((element) => element.getBoundingClientRect().toJSON()),
      ])
      expect(bounds[1].left).toBeGreaterThanOrEqual(bounds[0].left)
      expect(bounds[1].right).toBeLessThanOrEqual(bounds[0].right)
      expect(bounds[2].top).toBeGreaterThanOrEqual(bounds[0].top - 1)
      expect(bounds[2].bottom).toBeLessThanOrEqual(bounds[0].bottom + 1)

      if (viewport.scrollable) {
        const track = await page
          .locator('.industrial-dashboard-scroll .simplebar-track.simplebar-vertical')
          .evaluate((element) => element.getBoundingClientRect().toJSON())
        const content = await page.locator('.industrial-dashboard-content').evaluate((element) => {
          const rect = element.getBoundingClientRect()
          return {
            right: rect.right,
            paddingRight: Number.parseFloat(getComputedStyle(element).paddingRight),
          }
        })
        expect(Math.abs(track.x + track.width / 2 - (bounds[0].right - 15))).toBeLessThan(1)
        expect(Math.abs(track.top - (bounds[0].top + 23))).toBeLessThan(1)
        expect(Math.abs(track.bottom - (bounds[0].bottom - 23))).toBeLessThan(1)
        expect(track.left - (content.right - content.paddingRight)).toBeGreaterThanOrEqual(8)
        const thumb = await page
          .locator(
            '.industrial-dashboard-scroll .simplebar-track.simplebar-vertical .simplebar-scrollbar'
          )
          .evaluate((element) => {
            const style = getComputedStyle(element, '::before')
            return {
              backgroundImage: style.backgroundImage,
              corners: [
                style.borderTopLeftRadius,
                style.borderTopRightRadius,
                style.borderBottomRightRadius,
                style.borderBottomLeftRadius,
              ],
            }
          })
        expect(thumb.backgroundImage).toBe('none')
        expect(new Set(thumb.corners).size).toBe(1)
      }
    }
  })

  test('keeps a consistent bounded log column across desktop workspaces', async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 860 })
    const routes = [
      '/devices/fp-lab-01/overview',
      '/devices/fp-lab-01/settings/presets',
      '/devices/fp-lab-01/calibration/heater-curve',
      '/devices/fp-lab-01/calibration/firmware-tuning',
    ]
    const widths: Array<{ main: number; log: number }> = []

    for (const route of routes) {
      await page.goto(route)
      const collapseInspector = page.getByRole('button', { name: '收起 Demo Inspector' })
      if (await collapseInspector.isVisible()) await collapseInspector.click()
      await expect(page.locator('.industrial-log-panel')).toBeVisible()
      await page.evaluate(() => {
        const inspector = document.querySelector('.demo-inspector')
        if (inspector instanceof HTMLElement) inspector.style.setProperty('display', 'none')
      })

      const geometry = await page.evaluate(() => {
        const workspace = document.querySelector('.industrial-console__workspace')
        const main = document.querySelector('.industrial-console__main')
        const log = document.querySelector('.industrial-log-panel')
        if (!workspace || !main || !log) throw new Error('Workspace columns are not rendered')
        const workspaceStyle = getComputedStyle(workspace)
        return {
          main: main.getBoundingClientRect().width,
          log: log.getBoundingClientRect().width,
          gap: Number.parseFloat(workspaceStyle.columnGap),
          workspace: workspace.getBoundingClientRect().width,
        }
      })

      expect(geometry.log).toBe(416)
      expect(geometry.main).toBe(818)
      expect(
        Math.abs(geometry.main + geometry.log + geometry.gap - geometry.workspace)
      ).toBeLessThan(1)
      widths.push({ main: geometry.main, log: geometry.log })
    }

    expect(
      Math.max(...widths.map(({ log }) => log)) - Math.min(...widths.map(({ log }) => log))
    ).toBe(0)
    expect(
      Math.max(...widths.map(({ main }) => main)) - Math.min(...widths.map(({ main }) => main))
    ).toBe(0)

    await page.setViewportSize({ width: 1440, height: 900 })
    await page.goto('/devices/fp-lab-01/overview')
    const wideLogWidth = await page
      .locator('.industrial-log-panel')
      .evaluate((element) => element.getBoundingClientRect().width)
    expect(wideLogWidth).toBe(416)
  })

  test('keeps the tuning workspace width monotonic across viewport resizes', async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 860 })
    await page.goto('/devices/fp-lab-01/calibration/firmware-tuning')
    const collapseInspector = page.getByRole('button', { name: '收起 Demo Inspector' })
    if (await collapseInspector.isVisible()) await collapseInspector.click()
    await expect(page.locator('.firmware-tuning__detail')).toBeVisible()

    const measure = () =>
      page.evaluate(() => {
        const detail = document.querySelector('.firmware-tuning__detail')
        const workspace = document.querySelector('.industrial-console__workspace')
        const scroll = document.querySelector(
          '.industrial-calibration-tabs__firmware-scroll .simplebar-content-wrapper'
        )
        if (!detail || !workspace) throw new Error('Tuning workspace is not rendered')
        const scrollArea = document.querySelector('.industrial-calibration-tabs__firmware-scroll')
        const content = scrollArea?.closest('.industrial-calibration-tabs__content')
        if (!scrollArea || !content) throw new Error('Tuning scroll container is missing')
        return {
          detail: detail.getBoundingClientRect().width,
          workspace: workspace.getBoundingClientRect().width,
          content: content.getBoundingClientRect().width,
          scrollArea: scrollArea.getBoundingClientRect().width,
          overflow: scroll ? scroll.scrollHeight - scroll.clientHeight : 0,
          horizontal: scroll ? scroll.scrollWidth - scroll.clientWidth : 0,
        }
      })

    // The grid track used to latch on the widened content, so returning to the
    // baseline viewport left the detail column stuck at the narrow-layout size.
    const baseline = await measure()
    expect(baseline.horizontal).toBe(0)

    for (const width of [1180, 1280, 1180, 1280, 1100, 1280]) {
      await page.setViewportSize({ width, height: 860 })
      const current = await measure()
      expect(current.horizontal).toBe(0)
      // The scroll area is a grid item, so an unset min-width let it keep the
      // width it had at the wider layout and spill out of its column.
      expect(current.scrollArea).toBeLessThanOrEqual(current.content + 1)
      expect(current.detail).toBeLessThanOrEqual(current.content + 1)
      if (width === 1280) {
        expect(current.detail).toBeCloseTo(baseline.detail, 0)
        expect(current.workspace).toBeCloseTo(baseline.workspace, 0)
        expect(current.overflow).toBeLessThanOrEqual(1)
      } else {
        expect(current.workspace).toBeLessThan(baseline.workspace)
      }
    }
  })

  test('shows the full identifier in the tooltip and leaves the controls usable', async ({
    page,
  }) => {
    await page.setViewportSize({ width: 1280, height: 860 })
    await page.goto('/devices/fp-lab-01/calibration/firmware-tuning?demoScene=tuning-save')
    const collapseInspector = page.getByRole('button', { name: '收起 Demo Inspector' })
    if (await collapseInspector.isVisible()) await collapseInspector.click()

    const summaries = page.locator('.firmware-tuning__identifier-summary')
    // The status read resolves after mount; the summary renders an em dash until
    // then and has no tooltip trigger at all.
    await expect(summaries.first()).not.toHaveClass(/identifier-empty/, { timeout: 15_000 })

    for (let index = 0; index < 2; index += 1) {
      const tooltip = page.locator('[data-slot="tooltip-content"]')
      const trigger = summaries.nth(index)
      // The trigger lives inside the scroll area, so hover only lands once the
      // row is scrolled into view. Radix opens the bubble after a short delay.
      await trigger.scrollIntoViewIfNeeded()
      await trigger.hover()
      await expect(trigger).toHaveAttribute('data-state', /open/, { timeout: 10_000 })

      const measured = await page.evaluate((target) => {
        const bubble = document.querySelector('[data-slot="tooltip-content"]')
        const triggers = document.querySelectorAll('.firmware-tuning__identifier-summary')
        const copies = document.querySelectorAll('.firmware-tuning__identifier-copy')
        if (!bubble || !copies[target]) throw new Error('Tooltip geometry targets are missing')

        const box = bubble.getBoundingClientRect()
        const ownButton = copies[target].getBoundingClientRect()
        const ownRow = triggers[target].closest('.firmware-tuning__identifier-field')
        // The bubble uses a shorthand font, so lineHeight computes to "normal";
        // measure one line directly instead of dividing by a NaN.
        const probe = bubble.cloneNode(true)
        if (probe instanceof HTMLElement) {
          probe.style.position = 'absolute'
          probe.style.visibility = 'hidden'
          probe.style.width = 'max-content'
          probe.style.whiteSpace = 'nowrap'
          document.body.append(probe)
        }
        const singleLine = probe instanceof HTMLElement ? probe.getBoundingClientRect().height : 0
        probe.remove()

        return {
          side: bubble.getAttribute('data-side'),
          // Radix mirrors the label into a hidden node for aria-describedby, so
          // read the first text node rather than the concatenated tree.
          text: (bubble.firstChild?.textContent ?? '').trim(),
          expected: triggers[target].textContent ?? '',
          width: box.width,
          lines: singleLine > 0 ? Math.round(box.height / singleLine) : 1,
          // The bubble must not sit on the button of the row it describes.
          coversOwnButton: !(
            box.bottom < ownButton.top ||
            box.top > ownButton.bottom ||
            box.right < ownButton.left ||
            box.left > ownButton.right
          ),
          inViewport: box.left >= 0 && box.right <= window.innerWidth,
          rowWidth: ownRow?.getBoundingClientRect().width ?? 0,
          // Whatever it floats over stays clickable, so overlap is harmless.
          buttonsReachable: [...copies].every((copy) => {
            const button = copy.getBoundingClientRect()
            const hit = document.elementFromPoint(
              button.left + button.width / 2,
              button.top + button.height / 2
            )
            return hit === copy || copy.contains(hit)
          }),
        }
      }, index)

      // Radix flips to the top when the bubble does not fit, which put it right
      // on top of the copy button for the same row.
      expect(measured.side).toBe('bottom')
      expect(measured.coversOwnButton).toBe(false)
      expect(measured.inViewport).toBe(true)
      expect(measured.buttonsReachable).toBe(true)
      // Identifiers are read by pattern, so the full value must stay on one line
      // rather than be squeezed into the narrow column.
      expect(measured.text).toBe(measured.expected)
      expect(measured.lines).toBe(1)
      expect(measured.width).toBeGreaterThan(measured.rowWidth)

      // A single jump skips the pointer leave event, so walk the pointer out in
      // steps to let Radix close the bubble before the next iteration.
      await page.mouse.move(900, 400, { steps: 25 })
      await expect(trigger).toHaveAttribute('data-state', 'closed', { timeout: 10_000 })
      await expect(tooltip).toHaveCount(0)
    }
  })

  test('lets the tuning identifiers use the full column width before clipping', async ({
    page,
  }) => {
    await page.setViewportSize({ width: 1280, height: 860 })
    await page.goto('/devices/fp-lab-01/calibration/firmware-tuning?demoScene=tuning-save')
    const collapseInspector = page.getByRole('button', { name: '收起 Demo Inspector' })
    if (await collapseInspector.isVisible()) await collapseInspector.click()

    const summaries = page.locator('.firmware-tuning__identifier-summary')
    await expect(summaries.first()).toBeVisible()

    const measured = await summaries.evaluateAll((nodes) =>
      nodes.map((node) => ({
        text: node.textContent ?? '',
        clientWidth: node.clientWidth,
        scrollWidth: node.scrollWidth,
        // The identifier sits in a grid track that spans the whole column.
        trackWidth: node.closest('.firmware-tuning__identifier-field')?.clientWidth ?? 0,
        columnWidth:
          node.closest('.firmware-tuning__detail > section')?.getBoundingClientRect().width ?? 0,
      }))
    )

    expect(measured).toHaveLength(2)
    for (const item of measured) {
      // A fixed character budget used to clip the value even when the column had
      // room, so the rendered text must never carry a pre-baked ellipsis.
      expect(item.text).not.toContain('…')
      expect(item.clientWidth).toBeGreaterThan(0)
      expect(item.clientWidth).toBeLessThanOrEqual(item.columnWidth + 1)
      // The value box fills the whole track instead of a fixed character budget.
      expect(item.trackWidth).toBeGreaterThan(0)
      expect(item.clientWidth).toBeCloseTo(item.trackWidth, 0)
    }
    const overflowing = measured.filter((item) => item.scrollWidth > item.clientWidth + 1)
    expect(overflowing.every((item) => item.text.length > 20)).toBe(true)
  })

  test('keeps the heater meter full width with an unscaled ramp and unclipped button shadows', async ({
    page,
  }) => {
    await page.setViewportSize({ width: 1280, height: 860 })
    await page.goto('/devices/fp-lab-01/overview')
    const collapseInspector = page.getByRole('button', { name: '收起 Demo Inspector' })
    if (await collapseInspector.isVisible()) await collapseInspector.click()

    const meter = page.locator('.industrial-heat-output')
    await expect(meter).toBeVisible()

    const meterBox = await page.evaluate(() => {
      const track = document.querySelector('.industrial-heat-output-track')
      const field = document.querySelector('.industrial-heat-output')
      if (!track || !field) throw new Error('Heater meter is missing')
      const trackBox = track.getBoundingClientRect()
      const reading = track.nextElementSibling?.getBoundingClientRect()
      return {
        trackWidth: trackBox.width,
        readingWidth: reading?.width ?? 0,
        radius: getComputedStyle(track).borderRadius,
        value: Number((field as HTMLMeterElement).value),
      }
    })

    // The bar follows the dial instead of stopping at a fixed cap.
    expect(meterBox.trackWidth).toBeGreaterThan(360)
    expect(Math.abs(meterBox.trackWidth - meterBox.readingWidth)).toBeLessThan(2)
    // A pill, not a rectangle.
    expect(Number.parseFloat(meterBox.radius)).toBeGreaterThan(6)
    expect(meterBox.value).toBeGreaterThan(0)

    const gradientSizing = await page.evaluate(() => {
      const field = document.querySelector('.industrial-heat-output')
      if (!field) throw new Error('Heater meter is missing')
      const style = getComputedStyle(field)
      return {
        backgroundSize: style.backgroundSize,
        clipPath: style.clipPath,
        borderRadius: style.borderRadius,
      }
    })
    // The ramp spans the whole track and is clipped to the value, so a partial
    // reading shows the colour at that value rather than squashing the ramp.
    expect(gradientSizing.backgroundSize).toContain('100%')
    expect(gradientSizing.clipPath).toContain('inset')
    // The reveal edge keeps a pill cap, so the fill never shows a square corner.
    expect(gradientSizing.clipPath).toContain('round')

    const shadow = await page.evaluate(() => {
      const button = [...document.querySelectorAll('button')].find((node) =>
        node.textContent?.includes('Hold heater')
      )
      const wrapper = document.querySelector(
        '.industrial-dashboard-scroll .simplebar-content-wrapper'
      )
      if (!button || !wrapper) throw new Error('Heater hold button is missing')
      return {
        gap: wrapper.getBoundingClientRect().right - button.getBoundingClientRect().right,
        hasLiftShadow: getComputedStyle(button).boxShadow !== 'none',
      }
    })

    // SimpleBar clips its wrapper, so the shadow needs clearance to the right of
    // the button instead of the button sitting flush against the edge.
    expect(shadow.hasLiftShadow).toBe(true)
    expect(shadow.gap).toBeGreaterThanOrEqual(8)
  })

  test('keeps every demo inspector label readable in both dock states', async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 860 })
    await page.goto('/devices/fp-lab-01/overview')

    const measureSmallText = () =>
      page.evaluate(() => {
        const offenders: { text: string; size: number; className: string }[] = []
        for (const root of document.querySelectorAll('.demo-inspector, .demo-inspector *')) {
          const element = root as HTMLElement
          if (!element.offsetParent) continue
          const own = [...element.childNodes]
            .filter((node) => node.nodeType === Node.TEXT_NODE)
            .map((node) => node.textContent?.trim() ?? '')
            .join('')
          if (!own) continue
          const size = Number.parseFloat(getComputedStyle(element).fontSize)
          if (size < 11) {
            offenders.push({
              text: own.slice(0, 30),
              size: Number(size.toFixed(2)),
              className: String(element.className),
            })
          }
        }
        return offenders
      })

    // The collapsed bubble renders its own label, so both states need checking.
    await expect(page.locator('.demo-inspector__bubble')).toBeVisible()
    expect(await measureSmallText()).toEqual([])

    await page.locator('.demo-inspector__bubble').click()
    await expect(page.locator('.demo-inspector__group legend').first()).toBeVisible()
    expect(await measureSmallText()).toEqual([])
  })

  test('keeps the panel kicker subordinate to the title it introduces', async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 860 })
    await page.goto('/devices/fp-lab-01/overview')
    const collapseInspector = page.getByRole('button', { name: '收起 Demo Inspector' })
    if (await collapseInspector.isVisible()) await collapseInspector.click()

    const kickers = page.locator('.industrial-panel-kicker')
    await expect(kickers).toHaveCount(2)

    const measured = await kickers.evaluateAll((nodes) =>
      nodes.map((node) => {
        const style = getComputedStyle(node)
        const title = node.parentElement?.querySelector('h2')
        const titleStyle = title ? getComputedStyle(title) : null
        return {
          text: node.textContent?.trim() ?? '',
          weight: Number(style.fontWeight),
          titleWeight: titleStyle ? Number(titleStyle.fontWeight) : 0,
          size: Number.parseFloat(style.fontSize),
          titleSize: titleStyle ? Number.parseFloat(titleStyle.fontSize) : 0,
          transformed: style.textTransform,
          color: style.color,
        }
      })
    )

    for (const kicker of measured) {
      // Small install text, never a competing all-caps stamp.
      expect(kicker.text.length).toBeGreaterThan(0)
      expect(kicker.size).toBeGreaterThanOrEqual(11)
      expect(kicker.size).toBeLessThan(kicker.titleSize)
      expect(kicker.weight).toBeLessThan(kicker.titleWeight)
      expect(kicker.transformed).not.toBe('uppercase')
    }

    // The dark log panel needs its own text token; the muted one is dark-on-dark.
    const darkKicker = measured.find((kicker) => kicker.text === '全局日志')
    const lightKicker = measured.find((kicker) => kicker.text === 'Dashboard')
    expect(darkKicker).toBeDefined()
    expect(darkKicker?.color).not.toBe(lightKicker?.color)
  })

  test('gives the empty tuning chart a full-height illustration and uniform desktop spacing', async ({
    page,
  }) => {
    await page.setViewportSize({ width: 1280, height: 860 })
    await page.goto('/devices/fp-lab-01/calibration/firmware-tuning')
    const collapseInspector = page.getByRole('button', { name: '收起 Demo Inspector' })
    if (await collapseInspector.isVisible()) await collapseInspector.click()
    await expect(page.locator('.firmware-tuning__chart-empty')).toBeVisible()
    const targetSequence = page.locator('.firmware-tuning__target-sequence')
    await expect(targetSequence).toBeVisible()
    await expect(targetSequence).toContainText('60°C')
    await expect(targetSequence).toContainText('220°C')
    expect(await targetSequence.locator('button').count()).toBe(8)

    const layout = await page.evaluate(() => {
      const tuning = document.querySelector('.firmware-tuning')
      const empty = document.querySelector('.firmware-tuning__chart-empty')
      const visual = document.querySelector('.firmware-tuning__chart-empty-visual')
      if (!tuning || !empty || !visual) throw new Error('Chart empty state is not rendered')
      const blocks = [...tuning.children]
        .map((element) => {
          const rect = element.getBoundingClientRect()
          return { top: rect.top, bottom: rect.bottom }
        })
        .filter((rect) => rect.bottom > rect.top)
      return {
        gap: Number.parseFloat(getComputedStyle(tuning).rowGap),
        gaps: blocks.slice(1).map((block, index) => block.top - blocks[index].bottom),
        emptyHeight: empty.getBoundingClientRect().height,
        visual: visual.getBoundingClientRect().toJSON(),
        title: empty.querySelector('strong')?.textContent?.trim(),
        description: empty.querySelector('span')?.textContent?.trim(),
      }
    })
    expect(layout.gap).toBe(8)
    expect(layout.gaps.length).toBeGreaterThanOrEqual(2)
    expect(layout.gaps.every((gap) => Math.abs(gap - 8) < 1)).toBe(true)
    expect(layout.emptyHeight).toBeGreaterThan(350)
    expect(layout.visual.width).toBe(84)
    expect(layout.visual.height).toBe(84)
    expect(layout.title).toBe('暂无有效温度采样')
    expect(layout.description).toContain('有效采样')
  })

  test('renders the populated tuning curve from the deterministic save fixture', async ({
    page,
  }) => {
    await page.setViewportSize({ width: 1280, height: 860 })
    await page.goto('/devices/fp-lab-01/calibration/firmware-tuning?demoScene=tuning-save')
    const curve = page.locator('.firmware-tuning__chart-plot svg')
    await expect(curve).toBeVisible({ timeout: 10_000 })
    expect(await curve.locator('polyline').count()).toBeGreaterThan(0)
    await expect(page.getByText('预热', { exact: true })).toBeVisible()
    await expect(page.getByText('接近目标', { exact: true })).toBeVisible()
    await expect(page.getByText('稳温', { exact: true })).toBeVisible()
    const targetGrid = page.locator('.firmware-tuning__target-grid button')
    await expect(targetGrid).toHaveCount(8)
    await expect(targetGrid).toHaveText([
      '60°C',
      '220°C',
      '120°C',
      '80°C',
      '100°C',
      '160°C',
      '140°C',
      '180°C',
    ])
    await expect(page.locator('.firmware-tuning__target-grid button.is-selected')).toHaveCount(1)
    await expect(page.locator('.firmware-tuning__target-grid button.is-selected')).toHaveText(
      '180°C'
    )

    const detailHeights = await page.evaluate(() => {
      const candidate = document.querySelector('.firmware-tuning__detail > section:first-child')
      const results = document.querySelector('.firmware-tuning__results')
      const card = document.querySelector('.firmware-tuning__detail > section:first-child')
      const wrapper = document.querySelector(
        '.industrial-calibration-tabs__firmware-scroll .simplebar-content-wrapper'
      )
      if (!candidate || !results || !card) throw new Error('Tuning detail columns are not rendered')
      return {
        candidate: candidate.getBoundingClientRect().height,
        results: results.getBoundingClientRect().height,
        rows: [...document.querySelectorAll('.firmware-tuning__target-grid button')].map(
          (row) => row.getBoundingClientRect().height
        ),
        overflow: wrapper ? wrapper.scrollHeight - wrapper.clientHeight : 0,
      }
    })
    // Both detail columns share one row track, so the candidate card matches
    // the chart card and the target list absorbs the leftover height.
    expect(Math.abs(detailHeights.candidate - detailHeights.results)).toBeLessThan(1)
    expect(detailHeights.overflow).toBeLessThanOrEqual(1)
    expect(Math.max(...detailHeights.rows) - Math.min(...detailHeights.rows)).toBeLessThan(1)

    const controls = await page
      .locator('.firmware-tuning__primary-actions > button')
      .evaluateAll((buttons) => buttons.map((button) => button.getBoundingClientRect().height))
    expect(controls.length).toBeGreaterThan(0)
    expect(Math.max(...controls)).toBeLessThanOrEqual(40)
  })

  test('switches the tuning curve when a target cell is clicked', async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 860 })
    await page.goto('/devices/fp-lab-01/calibration/firmware-tuning?demoScene=tuning-save')
    await expect(page.locator('.firmware-tuning__chart-plot svg')).toBeVisible({ timeout: 10_000 })
    // The floating Inspector overlays the candidate card; collapse it so the
    // target grid receives real pointer events instead of a hit-test miss.
    const collapseInspector = page.getByRole('button', { name: '收起 Demo Inspector' })
    if (await collapseInspector.isVisible()) await collapseInspector.click()
    const grid = page.locator('.firmware-tuning__target-grid')
    const selected = page.locator('.firmware-tuning__target-grid button.is-selected')
    await expect(selected).toHaveText('180°C')

    // Every target in a full run is selectable, and the curve follows the pick.
    await grid.getByRole('radio', { name: '60°C', exact: true }).click()
    await expect(selected).toHaveText('60°C')
    await expect(page.locator('.firmware-tuning__target-sequence-heading')).toContainText(
      '当前 60°C'
    )
    await expect(page.getByText(/峰值 60\.\d °C/)).toBeVisible()
    await expect(page.locator('.firmware-tuning__chart-target-label')).toHaveText('目标 60 °C')
    expect(
      await page.evaluate(
        () => document.querySelector('.firmware-tuning__chart-plot polyline') !== null
      )
    ).toBe(true)

    await grid.getByRole('radio', { name: '180°C', exact: true }).click()
    await expect(selected).toHaveText('180°C')
    await expect(page.getByText(/峰值 180\.\d °C/)).toBeVisible()

    // Targets without samples are disabled rather than silently falling back.
    await page.goto('/devices/fp-lab-01/calibration/firmware-tuning?demoScene=tuning-unsupported')
    await expect(page.locator('.firmware-tuning__target-grid button:disabled')).toHaveCount(8)
    await expect(page.locator('.firmware-tuning__target-grid button.is-selected')).toHaveCount(0)
  })

  test('disables target cells that have no tuning trace in a partial run', async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 860 })
    await page.goto('/devices/fp-lab-01/calibration/firmware-tuning?demoScene=tuning-partial')
    await expect(page.locator('.firmware-tuning__chart-plot svg')).toBeVisible({ timeout: 10_000 })

    const cells = page.locator('.firmware-tuning__target-grid button')
    await expect(cells).toHaveCount(8)
    // Only the three targets the run actually reached stay selectable.
    await expect(cells.nth(0)).toBeEnabled()
    await expect(cells.nth(1)).toBeEnabled()
    await expect(cells.nth(2)).toBeEnabled()
    for (const index of [3, 4, 5, 6, 7]) await expect(cells.nth(index)).toBeDisabled()
    await expect(page.locator('.firmware-tuning__target-grid button:disabled')).toHaveCount(5)
    await expect(page.locator('.firmware-tuning__target-grid button:disabled').first()).toHaveText(
      /80°C\s*无采样/
    )
    await expect(
      page.locator('.firmware-tuning__target-grid button:enabled').first().locator('small')
    ).toHaveCount(0)

    const selected = page.locator('.firmware-tuning__target-grid button.is-selected')
    await expect(selected).toHaveText('120°C')
    await cells.nth(0).click()
    await expect(selected).toHaveText('60°C')
    await expect(page.getByText(/峰值 60\.\d °C/)).toBeVisible()

    // Disabled cells must not change the active selection.
    await page
      .locator('.firmware-tuning__target-grid button:disabled')
      .first()
      .click({ force: true })
    await expect(selected).toHaveText('60°C')
  })

  test('grows the in-progress run across targets instead of pinning one', async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 860 })
    await page.goto('/devices/fp-lab-01/calibration/firmware-tuning?demoScene=tuning-active')
    const cells = page.locator('.firmware-tuning__target-grid button')
    await expect(cells).toHaveCount(8)
    await expect(page.locator('.firmware-tuning__target-grid button:enabled')).toHaveCount(1)

    // A single hardcoded target left five cells permanently unsampled and the
    // curve at its full length from the first frame.
    await expect(page.locator('.firmware-tuning__target-grid button:enabled')).not.toHaveCount(1, {
      timeout: 15_000,
    })
    const plot = page.locator('.firmware-tuning__chart-plot polyline').first()
    const points = () =>
      plot.evaluate((element) => (element.getAttribute('points') ?? '').trim().split(/\s+/).length)
    const initial = await points()
    expect(initial).toBeLessThan(12)
    await expect.poll(points, { timeout: 20_000 }).toBeGreaterThan(initial)
  })

  test('preserves the tuning chart area by keeping the facts row single-line', async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 860 })
    await page.goto('/devices/fp-lab-01/calibration/firmware-tuning')
    const collapseInspector = page.getByRole('button', { name: '收起 Demo Inspector' })
    if (await collapseInspector.isVisible()) await collapseInspector.click()
    await expect(page.locator('.firmware-tuning__facts')).toBeVisible()

    const desktop = await page.evaluate(() => {
      const facts = document.querySelector('.firmware-tuning__facts')
      const detail = document.querySelector('.firmware-tuning__detail')
      const results = document.querySelector('.firmware-tuning__results')
      const candidate = document.querySelector('.firmware-tuning__detail > section:first-child')
      if (!facts || !detail || !results || !candidate) {
        throw new Error('Tuning chart workspace is not rendered')
      }
      const cells = [...facts.children].map((cell) => {
        const rect = cell.getBoundingClientRect()
        return { y: rect.y, width: rect.width }
      })
      return {
        columns: getComputedStyle(facts).gridTemplateColumns.split(' ').length,
        factsHeight: facts.getBoundingClientRect().height,
        cells,
        detailHeight: detail.getBoundingClientRect().height,
        candidateHeight: candidate.getBoundingClientRect().height,
        resultsHeight: results.getBoundingClientRect().height,
      }
    })
    expect(desktop.columns).toBe(6)
    expect(desktop.factsHeight).toBeLessThan(64)
    expect(desktop.cells.every((cell) => Math.abs(cell.y - desktop.cells[0].y) < 1)).toBe(true)
    expect(desktop.cells.every((cell) => cell.width < 140)).toBe(true)
    expect(desktop.detailHeight).toBeGreaterThan(400)
    expect(Math.abs(desktop.candidateHeight - desktop.resultsHeight)).toBeLessThan(1)
    expect(desktop.resultsHeight).toBeGreaterThan(400)
  })

  test('keeps functional desktop text at a readable minimum size', async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 860 })
    const routes = [
      {
        path: '/devices/fp-lab-01/overview',
        selector:
          '.industrial-log-filters button, .industrial-log-count, .industrial-log-follow__label, .industrial-device-select-value small, .industrial-advanced__state, .industrial-setpoint-control span',
      },
      {
        path: '/devices/fp-lab-01/calibration/heater-curve',
        selector:
          '.industrial-log-filters button, .industrial-log-count, .industrial-log-follow__label, .industrial-device-select-value small, .thermal-plant-run-card__metric small, .thermal-plant-run-card__section-heading > span, .thermal-plant-run-card__run-evidence, .thermal-plant-run-card__table-wrap th, .thermal-plant-run-card__table-wrap td',
      },
      {
        path: '/devices/fp-lab-01/calibration/firmware-tuning',
        selector:
          '.industrial-log-filters button, .industrial-log-count, .industrial-log-follow__label, .industrial-device-select-value small, .firmware-tuning__identifier-header, .firmware-tuning__identifier-copy, .firmware-tuning__facts span, .firmware-tuning__parameters p, .firmware-tuning__parameter-grid span',
      },
    ]

    for (const route of routes) {
      await page.goto(route.path)
      const collapseInspector = page.getByRole('button', { name: '收起 Demo Inspector' })
      if (await collapseInspector.isVisible()) await collapseInspector.click()
      await expect(page.locator('.industrial-log-panel')).toBeVisible()
      await page.evaluate(() => {
        const inspector = document.querySelector('.demo-inspector')
        if (inspector instanceof HTMLElement) inspector.style.setProperty('display', 'none')
      })

      const sizes = await page.locator(route.selector).evaluateAll((elements) =>
        elements
          .map((element) => {
            const rect = element.getBoundingClientRect()
            return {
              fontSize: Number.parseFloat(getComputedStyle(element).fontSize),
              visible: rect.width > 0 && rect.height > 0,
              text: element.textContent?.trim(),
            }
          })
          .filter((item) => item.visible && item.text)
          .map((item) => item.fontSize)
      )
      expect(sizes.length).toBeGreaterThan(0)
      expect(Math.min(...sizes)).toBeGreaterThanOrEqual(11)
    }
  })

  test('compacts the desktop header without shrinking its primary controls', async ({ page }) => {
    for (const viewport of [
      { width: 1440, height: 900 },
      { width: 1280, height: 860 },
    ]) {
      await page.setViewportSize(viewport)
      await page.goto('/devices/fp-lab-01/overview')

      const collapseInspector = page.getByRole('button', { name: '收起 Demo Inspector' })
      if (await collapseInspector.isVisible()) await collapseInspector.click()

      const measure = async (selector: string) =>
        page.locator(selector).evaluate((element) => {
          const rect = element.getBoundingClientRect()
          return { top: rect.top, bottom: rect.bottom, height: rect.height }
        })
      const [shell, header, nav, workspace, device, workspaceSwitch, navTab] = await Promise.all([
        measure('.industrial-console'),
        measure('.industrial-console__top'),
        measure('.industrial-view-tabs'),
        measure('.industrial-console__work-area'),
        measure('.industrial-device-select'),
        measure('.industrial-workspace-switch'),
        measure('.industrial-view-tab:first-child'),
      ])

      expect(shell.height).toBe(860)
      expect(header.height).toBeLessThanOrEqual(58)
      expect(nav.height).toBeLessThanOrEqual(50)
      expect(workspace.top).toBeLessThanOrEqual(nav.bottom + 8)
      expect(device.height).toBeGreaterThanOrEqual(44)
      expect(workspaceSwitch.height).toBeGreaterThanOrEqual(40)
      expect(navTab.height).toBeGreaterThanOrEqual(40)
    }

    await page.setViewportSize({ width: 1280, height: 800 })
    await page.goto('/devices/fp-lab-01/overview')
    const adaptiveHeader = await page.locator('.industrial-console__top').evaluate((element) => {
      const style = getComputedStyle(element)
      return { gap: style.gap, columns: style.gridTemplateColumns }
    })
    expect(adaptiveHeader.gap).toBe('12.8px')
    expect(adaptiveHeader.columns.split(' ').length).toBe(2)
  })

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

    await page.getByRole('button', { name: '重置应用状态' }).click()
    await expect(page).toHaveURL(copiedUrl)
    await expect(page.getByText('simulated thermal warning latched on current target')).toHaveCount(
      0
    )
    await expect(page.getByRole('combobox', { name: 'WiFi capability' })).toContainText(
      'Unavailable'
    )
    await expect(page.getByRole('checkbox', { name: 'Simulate lease conflict' })).toBeChecked()
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
    await expect(page.getByText(/#1 · Research Bench/)).toBeVisible()

    await selectInspectorOption(page, 'Persona', 'observer')
    await expect(page.getByRole('button', { name: 'Simulate thermal warning' })).toBeDisabled()
    await expect(page.getByText('Observer permissions keep actions read-only.')).toBeVisible()
  })

  test('reset atomically closes an active calibration guard', async ({ page }) => {
    await page.goto('/devices/fp-lab-01/calibration/rtd-adc?demo=true')
    await page.getByRole('switch', { name: '标定模式' }).click()
    await expect(page.getByRole('switch', { name: '标定模式' })).toBeChecked()
    await page.getByRole('button', { name: '打开 Demo Inspector' }).click()

    await page.getByRole('button', { name: '重置应用状态' }).click()

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

    await page.getByRole('button', { name: '重置应用状态' }).click()

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
    await page.getByRole('button', { name: '重置应用状态' }).click()

    await expect(page).toHaveURL('/devices/fp-kit-02/settings/presets?demoScene=degraded')
    await expect(page.getByRole('combobox', { name: 'Current scene' })).toContainText('Degraded')

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
    const reset = page.getByRole('button', { name: '重置应用状态' })
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

  test('uses concise names for every demo target fixture', async ({ page }) => {
    await page.goto('/')
    await page.getByRole('button', { name: '打开 Demo Inspector' }).click()

    await page.getByRole('combobox', { name: 'Current target' }).click()
    await expect(page.getByRole('option', { name: /Research Bench/ })).toBeVisible()
    await expect(page.getByRole('option', { name: /Production Bench/ })).toBeVisible()
    await expect(page.getByRole('option', { name: /Service Bench/ })).toBeVisible()
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
    await expect(page.getByText('Service Bench', { exact: true })).toBeVisible()
    await expect(page.getByText('-54 dBm')).toHaveCount(0)

    await page.goto('/devices/fp-lab-01/update?demo=true&demoScene=blocked-artifact')
    await expect(page.getByRole('button', { name: '选择固件包' })).toBeVisible()
    await expect(page.getByRole('button', { name: '运行预检' })).toBeDisabled()
  })
})
