import { expect, test } from '@playwright/test'

test.describe('thermal plant result layout', () => {
  test('keeps the live trace and supply-boundary table readable after a desktop re-entry', async ({
    page,
  }) => {
    await page.setViewportSize({ width: 960, height: 900 })
    await page.goto('/devices/fp-lab-01/calibration/heater-curve')

    const resultCard = page.getByRole('article', { name: '自动热模型标定结果' })
    await expect(resultCard).toBeVisible({ timeout: 10_000 })
    await expect(resultCard.getByText('温度—申请电压')).toBeVisible()
    await expect(resultCard.getByRole('heading', { name: '供电观测' })).toBeVisible()
    await expect(
      resultCard
        .getByRole('table', { name: '供电边界采样' })
        .getByRole('cell', { name: '21.0 V' })
        .first()
    ).toBeVisible()
    await expect(resultCard.getByText('R(T)', { exact: false })).toHaveCount(0)

    const boundaryChart = resultCard.locator('svg[aria-label="RTD 理论温度—申请电压散点图"]')
    const traceChart = resultCard.locator('svg[aria-label="加热和自然冷却温度曲线"]')
    const chartAspectDifference = (chart: typeof boundaryChart) => async () =>
      chart.evaluate((svg) => {
        const viewBox = svg.getAttribute('viewBox')?.split(/\s+/).map(Number) ?? []
        const rect = svg.getBoundingClientRect()
        return Math.abs(
          (viewBox[2] ?? 0) / (viewBox[3] ?? 1) - rect.width / Math.max(rect.height, 1)
        )
      })
    const boundaryAspectDifference = chartAspectDifference(boundaryChart)
    const traceAspectDifference = chartAspectDifference(traceChart)
    await expect.poll(boundaryAspectDifference).toBeLessThan(0.03)
    await expect.poll(traceAspectDifference).toBeLessThan(0.03)

    await page.setViewportSize({ width: 1440, height: 900 })
    await expect.poll(boundaryAspectDifference).toBeLessThan(0.03)
    await expect.poll(traceAspectDifference).toBeLessThan(0.03)
    const chartHeight = await boundaryChart.evaluate((svg) => svg.getBoundingClientRect().height)
    expect(chartHeight).toBeLessThan(600)
    const desktopLayout = await resultCard.evaluate((element) => {
      const read = (selector: string) => {
        const node = element.querySelector<HTMLElement>(selector)
        if (!node) return null
        const rect = node.getBoundingClientRect()
        return { x: rect.x, width: rect.width, right: rect.right }
      }
      const cardRect = element.getBoundingClientRect()
      return {
        card: { height: cardRect.height },
        boundary: read('.thermal-plant-run-card__chart--boundary'),
        trace: read('.thermal-plant-run-card__trace-panel'),
        table: read('.thermal-plant-run-card__table-wrap'),
        footer: read('.thermal-plant-run-card__footer'),
      }
    })
    expect(desktopLayout.card?.height ?? 0).toBeGreaterThan(500)
    expect(desktopLayout.boundary?.x ?? 0).toBeLessThan(desktopLayout.table?.x ?? 0)
    expect(desktopLayout.trace?.x ?? 0).toBeLessThan(desktopLayout.table?.x ?? 0)
    expect(Math.abs((desktopLayout.boundary?.x ?? 0) - (desktopLayout.trace?.x ?? 0))).toBeLessThan(
      2
    )
    expect(
      Math.abs((desktopLayout.boundary?.width ?? 0) - (desktopLayout.trace?.width ?? 0))
    ).toBeLessThan(2)
    expect(desktopLayout.footer?.x ?? 0).toBeGreaterThanOrEqual((desktopLayout.table?.x ?? 0) - 1)
    expect(desktopLayout.footer?.right ?? 0).toBeLessThanOrEqual(
      (desktopLayout.table?.right ?? 0) + 1
    )

    const surfaceStyles = await resultCard.evaluate((element) => {
      const read = (node: Element | null) => {
        const styles = getComputedStyle(node ?? element)
        return {
          borderRadius: styles.borderRadius,
          boxShadow: styles.boxShadow,
          padding: styles.padding,
          backgroundColor: styles.backgroundColor,
        }
      }
      return {
        surface: read(element),
        metrics: read(element.querySelector('.thermal-plant-run-card__metrics')),
        boundary: read(element.querySelector('.thermal-plant-run-card__chart--boundary')),
        trace: read(element.querySelector('.thermal-plant-run-card__trace-panel')),
        table: read(element.querySelector('.thermal-plant-run-card__table-wrap')),
        footer: read(element.querySelector('.thermal-plant-run-card__footer')),
      }
    })
    expect(surfaceStyles.surface.borderRadius).toBe('0px')
    expect(surfaceStyles.surface.boxShadow).toBe('none')
    expect(surfaceStyles.surface.padding).toBe('0px')
    for (const section of [
      surfaceStyles.metrics,
      surfaceStyles.boundary,
      surfaceStyles.trace,
      surfaceStyles.table,
    ]) {
      expect(section.borderRadius).toBe('12px')
      expect(section.boxShadow).not.toBe('none')
    }
    expect(surfaceStyles.footer.borderRadius).toBe('0px')
    expect(surfaceStyles.footer.boxShadow).toBe('none')
    expect(surfaceStyles.footer.backgroundColor).toBe('rgba(0, 0, 0, 0)')
    const footerCommand = await resultCard
      .locator('.thermal-plant-run-card__footer')
      .evaluate((footer) => {
        const command = footer.querySelector('button')
        if (!command) throw new Error('Calibration command is missing')
        const styles = getComputedStyle(footer)
        const frame = footer.getBoundingClientRect()
        const button = command.getBoundingClientRect()
        return {
          leftInset: button.left - frame.left,
          rightInset: frame.right - button.right,
          paddingLeft: Number.parseFloat(styles.paddingLeft),
          paddingRight: Number.parseFloat(styles.paddingRight),
        }
      })
    expect(Math.abs(footerCommand.leftInset - footerCommand.paddingLeft)).toBeLessThan(1)
    expect(Math.abs(footerCommand.rightInset - footerCommand.paddingRight)).toBeLessThan(1)
    const toolbarStyles = await page.locator('.industrial-status-strip').evaluate((toolbar) => {
      const styles = getComputedStyle(toolbar)
      return { backgroundColor: styles.backgroundColor, boxShadow: styles.boxShadow }
    })
    expect(toolbarStyles.backgroundColor).toBe('rgba(0, 0, 0, 0)')
    expect(toolbarStyles.boxShadow).toBe('none')

    await page.setViewportSize({ width: 1440, height: 850 })
    const shortHeightLayout = await resultCard.evaluate((element) => {
      const read = (selector: string) => {
        const node = element.querySelector<HTMLElement>(selector)
        if (!node) return null
        const rect = node.getBoundingClientRect()
        return { y: rect.y, bottom: rect.bottom }
      }
      const header = element.querySelector<HTMLElement>('.thermal-plant-run-card__header')
      const surface = element.getBoundingClientRect()
      const styles = getComputedStyle(element)
      return {
        surface: { y: surface.y, bottom: surface.bottom },
        header: read('.thermal-plant-run-card__header'),
        metrics: read('.thermal-plant-run-card__metrics'),
        body: read('.thermal-plant-run-card__body'),
        gridTemplateRows: styles.gridTemplateRows,
        headerPosition: header ? getComputedStyle(header).position : null,
      }
    })
    expect(shortHeightLayout.headerPosition).toBeNull()
    expect(shortHeightLayout.gridTemplateRows.split(' ').length).toBe(2)
    expect((shortHeightLayout.metrics?.y ?? 0) - (shortHeightLayout.surface?.y ?? 0)).toBeLessThan(
      2
    )
    expect(
      (shortHeightLayout.body?.y ?? 0) - (shortHeightLayout.metrics?.bottom ?? 0)
    ).toBeLessThan(16)

    await page.setViewportSize({ width: 393, height: 852 })
    await expect.poll(boundaryAspectDifference).toBeLessThan(0.03)
    await expect.poll(traceAspectDifference).toBeLessThan(0.03)
    const mobileChartHeight = await boundaryChart.evaluate(
      (svg) => svg.getBoundingClientRect().height
    )
    expect(mobileChartHeight).toBeGreaterThan(200)
    expect(mobileChartHeight).toBeLessThan(260)

    await page.setViewportSize({ width: 1440, height: 900 })
    await expect.poll(boundaryAspectDifference).toBeLessThan(0.03)
    await expect.poll(traceAspectDifference).toBeLessThan(0.03)

    const samples = [] as Array<{
      documentHeight: number
      tableHeight: number
      traceHeight: number
    }>
    for (let index = 0; index < 12; index += 1) {
      samples.push(
        await resultCard.evaluate((element) => ({
          documentHeight: document.documentElement.scrollHeight,
          tableHeight:
            element.querySelector('.thermal-plant-run-card__table-wrap')?.getBoundingClientRect()
              .height ?? 0,
          traceHeight:
            element.querySelector('.thermal-plant-run-card__trace-panel')?.getBoundingClientRect()
              .height ?? 0,
        }))
      )
      await page.waitForTimeout(250)
    }

    const documentHeights = samples.map((sample) => sample.documentHeight)
    const tableHeights = samples.map((sample) => sample.tableHeight)
    const traceHeights = samples.map((sample) => sample.traceHeight)

    expect(Math.max(...traceHeights) - Math.min(...traceHeights)).toBeLessThanOrEqual(1)
    expect(Math.min(...tableHeights)).toBeGreaterThan(120)
    expect(Math.max(...documentHeights)).toBeLessThanOrEqual(900)
  })
})
