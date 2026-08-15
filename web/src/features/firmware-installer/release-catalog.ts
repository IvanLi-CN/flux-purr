import { validateFirmwareBundle } from './bundle'
import type { FirmwareChannel, ValidatedFirmwareBundle } from './types'

const RELEASES_API = 'https://api.github.com/repos/IvanLi-CN/flux-purr/releases'

interface GithubRelease {
  prerelease: boolean
  assets: Array<{ name: string; browser_download_url: string }>
}

export async function fetchOfficialBundle(
  channel: Exclude<FirmwareChannel, 'local'>
): Promise<{ bytes: Uint8Array; bundle: ValidatedFirmwareBundle }> {
  const response = await fetch(
    channel === 'stable' ? `${RELEASES_API}/latest` : `${RELEASES_API}?per_page=20`
  )
  if (!response.ok) throw new Error(`Official release catalog failed (${response.status}).`)
  const payload = await response.json()
  const release =
    channel === 'stable'
      ? (payload as GithubRelease)
      : (payload as GithubRelease[]).find((candidate) => candidate.prerelease)
  const asset = release?.assets.find((candidate) => candidate.name.endsWith('.fluxpurr-fw'))
  if (!asset) throw new Error(`No ${channel.toUpperCase()} .fluxpurr-fw asset is published.`)
  const download = await fetch(asset.browser_download_url)
  if (!download.ok) throw new Error(`Official firmware download failed (${download.status}).`)
  const bytes = new Uint8Array(await download.arrayBuffer())
  const bundle = await validateFirmwareBundle(bytes)
  if (bundle.manifest.identity.channel !== channel) {
    throw new Error('Official firmware channel does not match the selected catalog.')
  }
  return { bytes, bundle }
}
