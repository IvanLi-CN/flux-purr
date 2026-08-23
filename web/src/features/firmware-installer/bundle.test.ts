import { readFile } from 'node:fs/promises'
import { resolve } from 'node:path'

import { zipSync } from 'fflate'
import SparkMD5 from 'spark-md5'
import { describe, expect, it } from 'vitest'

import fixture from '../../../../docs/specs/web-firmware-install-recovery/contracts/fixtures/valid-manifest.json'
import { validateFirmwareBundle } from './bundle'

async function sha256(bytes: Uint8Array) {
  const digest = await crypto.subtle.digest('SHA-256', Uint8Array.from(bytes))
  return Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, '0')).join('')
}

async function bundleBytes(extraManifestField = false) {
  const bootloader = new Uint8Array(0x4000).fill(0x11)
  const sourcePartition = new Uint8Array(
    await readFile(resolve(import.meta.dirname, '../../../../firmware/partitions.bin'))
  )
  const partition = new Uint8Array(0x1000).fill(0xff)
  partition.set(sourcePartition)
  const app = new Uint8Array(0x5000).fill(0x33)
  const images = [bootloader, partition, app]
  const manifest = structuredClone(fixture) as Record<string, unknown> & {
    segments: Array<{ length: number; sha256: string; md5: string }>
  }
  for (const [index, image] of images.entries()) {
    manifest.segments[index].length = image.byteLength
    manifest.segments[index].sha256 = `sha256:${await sha256(image)}`
    manifest.segments[index].md5 = SparkMD5.ArrayBuffer.hash(Uint8Array.from(image).buffer)
  }
  if (extraManifestField) manifest.unexpected = true
  return zipSync({
    'manifest.json': new TextEncoder().encode(`${JSON.stringify(manifest)}\n`),
    'images/bootloader.bin': bootloader,
    'images/partition-table.bin': partition,
    'images/factory-app.bin': app,
  })
}

describe('firmware bundle browser validation', () => {
  it('accepts the current deterministic three-segment contract', async () => {
    const bundle = await validateFirmwareBundle(await bundleBytes())
    expect(bundle.manifest.layout.id).toBe('flux-purr.esp32s3fh4r2.factory')
    expect(bundle.images).toHaveLength(3)
  })

  it('rejects unknown manifest fields', async () => {
    await expect(validateFirmwareBundle(await bundleBytes(true))).rejects.toMatchObject({
      code: 'manifest_invalid',
    })
  })

  it('rejects an oversized declared expansion before unzipping', async () => {
    const bytes = await bundleBytes()
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength)
    for (let offset = 0; offset + 46 <= bytes.byteLength; offset += 1) {
      if (view.getUint32(offset, true) === 0x02014b50) {
        view.setUint32(offset + 24, 9 * 1024 * 1024, true)
        break
      }
    }
    await expect(validateFirmwareBundle(bytes)).rejects.toMatchObject({ code: 'bundle_too_large' })
  })
})
