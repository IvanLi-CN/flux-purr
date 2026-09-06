import { readFile } from 'node:fs/promises'
import path from 'node:path'
import { describe, expect, it } from 'vitest'

const webRoot = path.resolve(import.meta.dirname, '..')

async function readPublic(relativePath: string) {
  return readFile(path.join(webRoot, 'public', relativePath))
}

function pngDimensions(bytes: Uint8Array) {
  expect([...bytes.subarray(0, 8)]).toEqual([137, 80, 78, 71, 13, 10, 26, 10])
  expect(new TextDecoder().decode(bytes.subarray(12, 16))).toBe('IHDR')
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength)
  return {
    width: view.getUint32(16),
    height: view.getUint32(20),
  }
}

describe('Flux Purr brand assets', () => {
  it('keeps approved SVG masters and derivatives raster-free', async () => {
    const paths = [
      'brand/flux-purr-logo-duotone.svg',
      'brand/flux-purr-logo-monochrome.svg',
      'brand/flux-purr-logo-dark.svg',
      'brand/flux-purr-logo-with-tagline.svg',
      'brand/flux-purr-logo-with-tagline-light.svg',
      'favicon.svg',
      'safari-pinned-tab.svg',
    ]

    for (const relativePath of paths) {
      const source = await readPublic(relativePath)
      const text = new TextDecoder().decode(source)
      expect(text).not.toMatch(/<image\b|<text\b|data:image|href=/i)
    }
  })

  it('keeps raster assets at their platform dimensions', async () => {
    const expected = {
      'favicon-16x16.png': [16, 16],
      'favicon-32x32.png': [32, 32],
      'apple-touch-icon.png': [180, 180],
      'icons/icon-192.png': [192, 192],
      'icons/icon-512.png': [512, 512],
      'icons/icon-512-maskable.png': [512, 512],
      'brand/flux-purr-logo-with-tagline.reference.png': [1774, 887],
    } as const

    for (const [relativePath, dimensions] of Object.entries(expected)) {
      const actual = pngDimensions(await readPublic(relativePath))
      expect([actual.width, actual.height]).toEqual(dimensions)
    }

    const ico = await readPublic('favicon.ico')
    const icoView = new DataView(ico.buffer, ico.byteOffset, ico.byteLength)
    expect(icoView.getUint16(0, true)).toBe(0)
    expect(icoView.getUint16(2, true)).toBe(1)
    expect(icoView.getUint16(4, true)).toBe(2)
    const icoDimensions = [0, 1].map((index) => [
      ico[6 + index * 16] || 256,
      ico[7 + index * 16] || 256,
    ])
    expect(icoDimensions).toEqual([
      [16, 16],
      [32, 32],
    ])
  })

  it('keeps both theme Logo lockups as transparent vector paths', async () => {
    for (const relativePath of [
      'brand/flux-purr-logo-with-tagline.svg',
      'brand/flux-purr-logo-with-tagline-light.svg',
    ]) {
      const source = await readPublic(relativePath)
      const text = new TextDecoder().decode(source)

      expect(text).toContain('viewBox="0 0 1425 316"')
      expect(text).not.toMatch(/<rect\b/i)
      expect(text.match(/<path\b/g)?.length).toBeGreaterThanOrEqual(4)
      expect(text).toContain('id="wordmark"')
      expect(text).toContain('id="tagline"')
      expect(text.match(/[Cc]/g)?.length).toBeGreaterThanOrEqual(150)
    }
  })

  it('declares every browser asset in the HTML and manifest contracts', async () => {
    const html = await readFile(path.join(webRoot, 'index.html'), 'utf8')
    const manifest = JSON.parse(
      await readFile(path.join(webRoot, 'public/site.webmanifest'), 'utf8')
    ) as {
      name: string
      start_url: string
      display: string
      icons: Array<{ src: string; sizes: string; type: string; purpose: string }>
    }

    expect(html).toContain('<title>Flux Purr Web App</title>')
    for (const href of [
      '/favicon.svg',
      '/favicon-32x32.png',
      '/favicon-16x16.png',
      '/favicon.ico',
      '/apple-touch-icon.png',
      '/safari-pinned-tab.svg',
      '/site.webmanifest',
    ]) {
      expect(html).toContain(`href="${href}"`)
    }
    expect(manifest.name).toBe('Flux Purr Web App')
    expect(manifest.start_url).toBe('/devices')
    expect(manifest.display).toBe('standalone')
    expect(manifest.icons).toEqual([
      { src: '/icons/icon-192.png', sizes: '192x192', type: 'image/png', purpose: 'any' },
      { src: '/icons/icon-512.png', sizes: '512x512', type: 'image/png', purpose: 'any' },
      {
        src: '/icons/icon-512-maskable.png',
        sizes: '512x512',
        type: 'image/png',
        purpose: 'maskable',
      },
    ])
  })
})
