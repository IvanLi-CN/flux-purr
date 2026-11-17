import { expect, test } from '@playwright/test'

// Use Chromium's full browser in headless mode, matching the installed CI browser.
test.use({ channel: 'chromium' })

test('stops active tuning reads at the retry budget and retains the last snapshot', async ({
  page,
}) => {
  const errors: string[] = []
  page.on('pageerror', (error) => errors.push(error.message))
  await page.goto('/e2e/fixtures/firmware-tuning-polling.html')
  const reads = page.getByTestId('read-count')
  // One successful active-run read, followed by three failures.
  await expect(reads).toHaveText('4')
  await expect(page.getByText('Fixture tuning read unavailable')).toBeVisible()
  await expect(page.getByText('PPS 3A · polling-fixture-run', { exact: true })).toBeVisible()
  await expect(page.getByRole('button', { name: '停止', exact: true })).toBeEnabled()
  await page.waitForTimeout(2_500)
  await expect(reads).toHaveText('4')

  await page.getByRole('button', { name: '重试读取' }).click()
  await expect(reads).toHaveText('7')
  await page.waitForTimeout(2_500)
  await expect(reads).toHaveText('7')
  await expect(page.getByTestId('write-count')).toHaveText('0')
  // Read recovery must not revoke the safety stop for the retained active run.
  const stop = page.getByRole('button', { name: '停止', exact: true })
  await expect(stop).toBeEnabled()
  await stop.click()
  await expect(page.getByTestId('write-count')).toHaveText('1')
  await expect(page.getByTestId('write-count')).toHaveAttribute(
    'data-command',
    JSON.stringify({ op: 'cancel', runId: 'polling-fixture-run' })
  )
  expect(errors).toEqual([])
})
