# devd firmware bundle HTTP contract

All fields use camelCase. Errors use the existing `ApiError` envelope and never expose arbitrary host paths or configuration bytes.

## `GET /api/v1/firmware-bundles`

Returns imported local and configured release-catalog bundles after strict validation. Each item contains `artifactId`, `source`, `channel`, `version`, `sourceSha`, `buildId`, `bundleSha256`, `size`, `layoutId`, and `operations`.

## `POST /api/v1/firmware-bundles`

Accepts one `.fluxpurr-fw` body with the bundle media type. The daemon streams into private temporary storage, enforces both 8 MiB limits, validates before publishing, and assigns `artifactId` from the full bundle SHA-256. Requests cannot name a host path.

## `POST /api/v1/devices/{deviceId}/firmware`

```json
{
  "leaseId": "lease-123",
  "artifactId": "sha256:...",
  "operation": "update",
  "dryRun": true,
  "approvalToken": null,
  "confirm": null,
  "allowDowngrade": false
}
```

- `operation`: `update | install_recovery`
- Dry-run performs full preflight and returns a 5-minute, single-use `approvalToken` only when execution could proceed.
- A token binds lease ID, exact authorized port, ROM MAC, bundle SHA-256, operation, downgrade decision, and canonical preflight digest.
- Execute re-probes all bound facts before consuming the token. Mismatch or expiry blocks without writing.
- Real update requires `confirm=FLASH`; real install/recovery requires `confirm=ERASE_INSTALL`.
- `FLUX_PURR_DEVD_ALLOW_REAL_FLASH=1`, lease ownership and serial exclusivity remain mandatory.

## Outcomes

- `blocked`: no write started.
- `failed`: write did not complete.
- `write_complete_unverified`: all segments passed ROM MD5 but runtime verification timed out or disagreed.
- `verified`: segment verification and target runtime identity/layout/install-status all match.

## Runtime Install Status

`GET /api/v1/devices/{deviceId}/install-status?lease_id={leaseId}` is a read-only native-serial endpoint. It requires the active exact-port lease and proxies only the firmware USB JSONL `get_install_status` response. It never reads or exposes raw persistence bytes.
