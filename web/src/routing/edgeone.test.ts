import { readFile } from 'node:fs/promises'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'

describe('EdgeOne SPA fallback', () => {
  it('rewrites device history paths without catching static assets', async () => {
    const config = JSON.parse(
      await readFile(resolve(process.cwd(), 'public/edgeone.json'), 'utf8')
    ) as {
      rewrites?: Array<Record<string, unknown>>
    }

    expect(config.rewrites).toContainEqual({
      source: '/devices',
      destination: '/index.html',
    })
    expect(config.rewrites).toContainEqual({
      source: '/devices/*',
      destination: '/index.html',
    })
    expect(config.rewrites).not.toContainEqual({
      source: '/*',
      destination: '/index.html',
    })
  })
})
