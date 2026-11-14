# LAN 设备会话与固件热控调优读取

> 本文冻结真实 Web 控制台在 LAN 设备连接、配对凭据复用、固件热控调优读取和写入租约之间的长期行为契约；当前实现覆盖见 `./IMPLEMENTATION.md`，生命周期与兼容性背景见 `./HISTORY.md`。

## Context and Scope

- Context: 设备状态和实时温度已经能够通过 LAN 读取，但固件热控调优页面仍把单个调优读取失败错误显示成设备连接失败。
- In scope: 浏览器 direct LAN session、设备身份与网络地址关系、配对凭据恢复、`/api/v1/thermal-tuning` GET/POST、能力声明、错误分类、调优页面状态和有限重试。
- Out of scope: LAN 初始 WiFi provisioning、固件烧录、热控算法本身、云端服务和 WAN 控制。

## Terms and Interfaces

- `deviceId`: 设备稳定身份；同一物理设备在 DHCP 地址变化后仍保持不变。
- `baseUrl`: 当前网络定位地址；它决定请求发往哪里，不代表设备身份，也不单独决定是否需要重新配对。
- `pairing credential`: 设备通过物理配对产生的 bearer 凭据；它按设备身份复用，不因正常地址更新而重复索取。
- `device connection`: 浏览器已经验证目标设备身份，并能读取基础身份、网络或运行态事实的状态。
- `tuning read state`: 固件热控调优页面针对最近一次调优读取操作的状态；它不改变顶部设备连接状态。
- `control lease`: 允许修改设备控制输入的短生命周期租约；只读请求不因缺少租约而失败。
- Interface: firmware `/api/v1/identity`, `/api/v1/status`, `/api/v1/thermal-tuning`, `/health`, `/api/v1/pairing/claim`; Web `lan-client.ts` and `FirmwareTuningWorkspace`.

## Requirements

### REQ-LTR-001 Identity and locator separation

- The system MUST treat `deviceId` as the stable device identity and `baseUrl` as a replaceable network locator.
- A saved LAN session MUST be associated with the device identity and retain its current verified `baseUrl`; a locator change MUST NOT by itself require a new four-digit pairing.
- The client MUST first use anonymous `/health` to compare the device identity at a new locator, then use the saved bearer to probe authenticated identity and status before replacing the stored locator or enabling normal operation.
- A locator that identifies a different device MUST never receive the saved bearer; the old device session remains preserved until the operator resolves the mismatch.
- covers: `VER-LTR-001`, `VER-LTR-002`

### REQ-LTR-002 Pairing credential lifecycle

- The client MUST reuse a valid saved pairing credential for the same verified device across page reloads and repeated connections.
- The client MUST show a pairing action only when no credential exists for the selected device, the device returns HTTP `401`/`unauthorized`, or the operator explicitly chooses to pair again.
- A lease expiry, tuning read failure, protocol error, or browser private-network error MUST NOT by itself delete the credential or open a pairing dialog.
- covers: `VER-LTR-003`, `VER-LTR-004`

### REQ-LTR-003 Independent connection and tuning states

- The top-level device connection state MUST remain connected when identity/status/temperature reads succeed, even if `/api/v1/thermal-tuning` fails.
- The tuning page MUST expose an internal read state with at least `idle`, `loading`, `ready`, `unsupported`, `unauthorized`, `transport_error`, and `protocol_error` outcomes.
- The tuning read state MUST be rendered in the tuning workspace and MUST NOT become a second top-level connection badge.
- covers: `VER-LTR-005`, `VER-LTR-006`

### REQ-LTR-004 Thermal-tuning capability contract

- Firmware that exposes `/api/v1/thermal-tuning` through either USB JSONL or LAN HTTP MUST advertise the `thermal_tuning` capability in `Identity.capabilities`.
- A firmware build that does not expose the route MUST return a stable HTTP error envelope with code `thermal_tuning_unsupported` for that route; it MUST NOT return a generic network or pairing error.
- The Web client MUST display “固件未提供热控调优接口” only when the capability is absent or the stable unsupported code is received.
- covers: `VER-LTR-007`, `VER-LTR-008`

### REQ-LTR-005 Read and write authorization boundary

- `GET /api/v1/thermal-tuning` MUST require a valid bearer but MUST NOT require an active control lease, create a lease, renew a lease, or change device control state.
- `POST /api/v1/thermal-tuning` MUST require a valid bearer and an active control lease, and MUST preserve the existing revision and safety checks for mutations.
- The same boundary MUST hold for the corresponding `devd` and Web Serial adapters: read operations are read-only, while tuning commands that change device state remain lease-protected.
- covers: `VER-LTR-009`, `VER-LTR-010`

### REQ-LTR-006 Typed endpoint error mapping

- The transport layer MUST preserve the operation name, endpoint path, HTTP status when available, protocol error code when available, and retryability classification.
- The client MUST distinguish `unauthorized`, `thermal_tuning_unsupported`, `transport_unavailable`, `server_retryable`, and `response_invalid` outcomes.
- A browser `TypeError` MUST NOT be translated unconditionally into a private-network permission failure; the message MUST state that the tuning read did not complete and MUST include the available diagnostic detail.
- A tuning failure MUST NOT be rewritten as “无法连接 LAN 设备” when the same session has already proved that identity and status reads work.
- covers: `VER-LTR-011`, `VER-LTR-012`

### REQ-LTR-007 Bounded polling and explicit retry

- The tuning page MUST perform at most three automatic retries for a retryable tuning read, using bounded backoff and a finite overall window.
- After the automatic retry budget is exhausted, the page MUST stop polling, retain the last successful tuning snapshot if one exists, and provide a visible manual “重试读取” action.
- Unsupported, unauthorized, and invalid-response outcomes MUST stop automatic polling immediately; only `401` may offer a re-pair action, and only the unsupported outcome may offer a firmware compatibility explanation.
- covers: `VER-LTR-013`, `VER-LTR-014`

### REQ-LTR-008 User-facing error language

- The tuning page MUST name the failed operation and explain the next safe action.
- Required message categories are: `设备已连接，但固件调优读取未完成`, `设备拒绝了当前配对凭据，请重新配对`, `当前固件未提供热控调优接口`, `设备返回的数据格式无效`, and `读取暂时失败，可重试`.
- The page MUST never request pairing for a transport-only failure and MUST never claim that the device is disconnected when status and temperature remain readable.
- covers: `VER-LTR-015`, `VER-LTR-016`

### REQ-LTR-009 Diagnostic trace integrity

- Each tuning read attempt MUST emit a bounded diagnostic event containing device identity, operation, endpoint, result category, retry count, and HTTP status or protocol code when available.
- Bearer tokens, four-digit pairing codes, passwords, and full authorization headers MUST be redacted before the event reaches the UI log, export, or persisted history.
- The event stream MUST make it possible to distinguish a successful status read followed by a failed tuning read.
- covers: `VER-LTR-017`

### REQ-LTR-010 Transport parity

- Direct LAN, `devd` LAN bridge, `devd` USB, and Web Serial MUST expose the same tuning domain states and error categories, even when their wire mechanisms differ.
- A transport adapter MUST not report capability or lease errors that belong to another adapter.
- The public Demo build MUST keep these behaviors mockable without opening LAN, Web Serial, or `devd` connections.
- covers: `VER-LTR-018`, `VER-LTR-019`

## Verification

### VER-LTR-001 Locator reconciliation fixture

- Method: Exercise a saved device session against a new `baseUrl` whose authenticated identity matches the saved `deviceId`.
- covers: `REQ-LTR-001`
- Pass condition: The locator is updated, the saved credential is reused, and no pairing dialog is shown.

### VER-LTR-002 Identity mismatch fixture

- Method: Exercise a saved session against a locator that reports a different `deviceId`.
- covers: `REQ-LTR-001`
- Pass condition: The bearer is not sent to the mismatched device, the old record remains intact, and the UI requests an explicit resolution.

### VER-LTR-003 Reload and reconnect fixture

- Method: Pair once, reload the page, and reconnect the same device at the same or verified new locator.
- covers: `REQ-LTR-002`
- Pass condition: The saved credential is reused without a second pairing code.

### VER-LTR-004 Authorization invalidation fixture

- Method: Return HTTP `401` from a previously paired device.
- covers: `REQ-LTR-002`
- Pass condition: Only the affected device credential becomes invalid, the pairing action is shown, and unrelated device records remain available.

### VER-LTR-005 Connected-with-tuning-failure fixture

- Method: Return successful `/health`, identity, status, and temperature reads followed by a failed tuning GET.
- covers: `REQ-LTR-003`
- Pass condition: The top-level device remains connected and only the tuning workspace enters an error state.

### VER-LTR-006 Tuning state rendering fixture

- Method: Replay ready, loading, unsupported, unauthorized, transport, and invalid-response tuning outcomes.
- covers: `REQ-LTR-003`, `REQ-LTR-008`
- Pass condition: Each outcome has the correct workspace message and action without changing the device connection badge incorrectly.

### VER-LTR-007 Capability parity inspection

- Method: Build firmware with `net_http` and inspect `Identity.capabilities`; build the compatible USB-only variant and inspect the same field.
- covers: `REQ-LTR-004`
- Pass condition: Every build exposing the tuning route advertises `thermal_tuning`.

### VER-LTR-008 Unsupported response fixture

- Method: Query a fixture without the tuning route.
- covers: `REQ-LTR-004`
- Pass condition: The response code is `thermal_tuning_unsupported`, the UI explains firmware compatibility, and no pairing flow opens.

### VER-LTR-009 Read authorization fixture

- Method: Send an authorized tuning GET without a lease and an unauthorized tuning GET without a bearer.
- covers: `REQ-LTR-005`
- Pass condition: The first succeeds without lease creation; the second returns `401`.

### VER-LTR-010 Write authorization fixture

- Method: Send tuning POST with no lease, an expired lease, and a valid lease.
- covers: `REQ-LTR-005`
- Pass condition: Missing or expired leases are rejected; a valid lease reaches the mutation path.

### VER-LTR-011 Error envelope mapping tests

- Method: Unit-test HTTP status, protocol code, invalid JSON, and transport exception mapping in the LAN client.
- covers: `REQ-LTR-006`
- Pass condition: Each input produces the matching typed category and preserves diagnostic fields.

### VER-LTR-012 Connected-session error isolation test

- Method: Complete status polling, then inject a tuning transport exception.
- covers: `REQ-LTR-006`
- Pass condition: The error message names tuning read failure and does not mention a disconnected LAN device or private-network permission unless independently evidenced.

### VER-LTR-013 Bounded retry test

- Method: Inject three retryable tuning failures followed by no response.
- covers: `REQ-LTR-007`
- Pass condition: Automatic attempts stop at the configured limit and no background timer remains active.

### VER-LTR-014 Manual retry test

- Method: Click “重试读取” after automatic retries stop.
- covers: `REQ-LTR-007`
- Pass condition: Exactly one new read sequence begins and the retry budget is reset for that user action.

### VER-LTR-015 Message catalog test

- Method: Snapshot the tuning workspace for each typed error category.
- covers: `REQ-LTR-008`
- Pass condition: The rendered Chinese message names the operation and next action without claiming a false connection failure.

### VER-LTR-016 Pairing prompt isolation test

- Method: Inject a transport failure and a server retryable failure after a valid session has been established.
- covers: `REQ-LTR-008`
- Pass condition: No pairing dialog or credential deletion occurs.

### VER-LTR-017 Redacted trace test

- Method: Record tuning attempts with bearer, pairing, and response metadata present.
- covers: `REQ-LTR-009`
- Pass condition: Trace entries retain endpoint diagnostics and contain none of the sensitive values.

### VER-LTR-018 Cross-transport contract fixture

- Method: Replay the same tuning outcome matrix through direct LAN, `devd` LAN, `devd` USB, and Web Serial adapters.
- covers: `REQ-LTR-010`
- Pass condition: Domain states and user-facing categories are equivalent across transports.

### VER-LTR-019 Demo isolation test

- Method: Build and run the public Demo fixture with tuning failures injected.
- covers: `REQ-LTR-010`
- Pass condition: The Demo never starts LAN, Web Serial, or `devd` activity while reproducing the same state model.

## Related ADRs

- [ADR 0026: Separate device identity, network locator, and operation readiness](../../adr/0026-separate-device-identity-network-locator-and-operation-readiness.md)

## Visual Evidence

- None

## References

- `./IMPLEMENTATION.md`
- `./HISTORY.md`
- [`docs/specs/real-control-plane-runtime/SPEC.md`](../real-control-plane-runtime/SPEC.md)
- [`docs/solutions/device-control/web-native-wifi-bridge-console.md`](../../solutions/device-control/web-native-wifi-bridge-console.md)
