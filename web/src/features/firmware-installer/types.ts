export type FirmwareOperation = 'update' | 'install_recovery'
export type FirmwareTransport = 'devd' | 'browser'
export type FirmwareChannel = 'stable' | 'rc' | 'local'

export type FirmwareStage =
  | 'artifact'
  | 'transport'
  | 'rom_reset'
  | 'chip_flash_security'
  | 'layout_config'
  | 'preflight'
  | 'erase'
  | 'write_segments'
  | 'rom_md5'
  | 'reset'
  | 'runtime_reconnect'
  | 'runtime_verify'

export type FirmwareOutcome =
  | 'idle'
  | 'running'
  | 'preflight_passed'
  | 'blocked'
  | 'failed'
  | 'write_complete_unverified'
  | 'verified'

export interface FirmwareManifest {
  schemaVersion: 1
  mediaType: 'application/vnd.flux-purr.firmware-bundle+zip'
  identity: {
    version: string
    sourceSha: string
    buildId: string
    channel: FirmwareChannel
  }
  target: {
    chip: 'esp32s3'
    package: 'ESP32-S3FH4R2'
    flashSize: 4194304
    psramSize: 2097152
    flashMode: 'dio'
    flashFrequency: '40m'
  }
  layout: {
    id: 'flux-purr.esp32s3fh4r2.factory'
    version: 1
    partitionTableSha256: string
  }
  segments: Array<{
    kind: 'bootloader' | 'partition-table' | 'factory-app'
    path: string
    address: number
    length: number
    sha256: string
    md5: string
  }>
  migrations: string[]
}

export interface ValidatedFirmwareBundle {
  manifest: FirmwareManifest
  bundleSha256: string
  archiveSize: number
  images: Map<string, Uint8Array>
}

export interface FirmwareRunState {
  operation: FirmwareOperation
  transport: FirmwareTransport
  stage: FirmwareStage
  stageIndex: number
  progress: number
  outcome: FirmwareOutcome
  message: string
}
