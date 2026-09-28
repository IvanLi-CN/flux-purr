# PD Sink HIL Procedure

This procedure verifies observable FUSB302B Sink behavior with a real PPS
source through the repository's `flux-purr-devd` and `flux-purr` CLI. The HIL
result is based on the device starting normally, reaching requested voltages,
holding power, and remaining responsive without restarts. Firmware tests cover
the internal PD message state machine; HIL checks its observable effect on
the powered device.

## Fixture and record

Record the board revision, PPS source model, cable rating, firmware ELF
SHA-256, CLI/devd Git SHA, test time, exact authorized USB port, status samples,
and observed front-panel/source behavior. Keep the port and raw logs in the
private test receipt, not in this procedure. `voltageMv` is the board's VIN
reading and provides the physical-voltage check; an external VBUS meter may
supplement it. `pdContractCurrentMa` and `pdContractPowerMw` are negotiated
limits, not measured load current or power.

The fixture needs a PPS APDO covering 15 V, 17.3 V, and 20 V at 3 A, and 20 V
at 5 A for the 100 W contract step. A missing capability makes that step **not
applicable**, not passed. Keep the heater disabled. Do not flash or reset
merely to repeat HIL; test the running image and record its identity. If the
authorized port disappears or changes, stop device operations. Never select a
replacement port from discovery output.

Do not use an idle voltage copied from this document as an acceptance value.
Record the firmware `gitSha`, `buildId`, and ELF SHA-256, then identify the
idle policy of that exact build; the current checkout alone may differ from
the running image. In the current source tree,
`FUSB302B_INITIAL_PPS_REQUEST_MV` defines the target, and
`automatic_idle_contract_is_confirmed` accepts a suitable PPS contract or a
fixed fallback allowed by that policy. A later firmware may choose another
target or fallback. Record the observed automatic idle contract as the
baseline only after confirming that it agrees with the tested firmware's
policy and the source capabilities. If the matching policy is unavailable,
mark the policy check unresolved instead of judging the idle voltage against
an assumed number.

Use 17.3 V to check adjustable PPS operation away from common fixed-PDO
voltages. `pdContractKind=pps` and VIN near 17.3 V establish that the actual
contract uses PPS. If the source publishes its fixed-PDO list, record that
17.3 V is absent before making the narrower claim that fixed PDOs cannot
provide it; this list is not required to judge PPS interoperability.

## Setup

Build the repository host tools if needed:

```sh
cargo build --manifest-path tools/flux-purr-devd/Cargo.toml --bins
```

Run all commands from the checkout root. In terminal A, set `PORT` to the exact
port authorized for this run. Obtain a port lease with the global port manager
before starting devd. Use the returned
`port` as `DEVD_PORT`, and a private absolute path as `SOCKET`:

```sh
set -euo pipefail
ROOT="$(git rev-parse --show-toplevel)"
PORT='<authorized-exact-serial-port>'
SCOPE="flux-purr--$(printf %s "$ROOT" | shasum -a 256 | cut -c1-8)--pd-sink-hil"
python3 "${CODEX_HOME:-$HOME/.codex}/skills/global-port-manager/scripts/port_registry.py" --json allocate \
  --scope-id "$SCOPE" --project flux-purr --repo-root "$ROOT" \
  --branch "$(git branch --show-current)" --worktree-path "$ROOT" --service devd
DEVD_PORT='<leased-port-from-output>'
SOCKET_DIR="$(mktemp -d "${TMPDIR:-/tmp}/flux-purr-pd-hil.XXXXXX")"
SOCKET="$SOCKET_DIR/devd.sock"
test -c "$PORT"
target/debug/flux-purr-devd serve --bind "127.0.0.1:$DEVD_PORT" \
  --serial-port "$PORT" --artifact-root "$ROOT" \
  --control-socket "$SOCKET" --no-dev-cors
```

The local control socket mode does not expose the HTTP API, even though `--bind`
is configured. In terminal B, set `SOCKET` to the same path and use only the
device ID associated with the authorized port:

```sh
set -o pipefail
SOCKET='<same-private-absolute-socket-path>'
target/debug/flux-purr devices --devd "$SOCKET" --json \
  | jq '.devices[] | {id, portPath}'
DEVICE='<id-whose-portPath-matches-the-authorized-port>'
target/debug/flux-purr identity --devd "$SOCKET" --device "$DEVICE" --json \
  | jq '{firmwareVersion, gitSha, buildId, deviceId}'
target/debug/flux-purr status --devd "$SOCKET" --device "$DEVICE" --json \
  | jq '{uptimeSeconds, pdController, pdState, pdRequestMv,
         pdContractKind, pdContractMv,
         pdContractCurrentMa, pdContractPowerMw, ppsCapabilityMinMv,
         ppsCapabilityMaxMv, ppsCapabilityMaxMa, voltageMv,
         manualPpsEnabled, heaterEnabled, heaterPhysicalOutputPercent}'
```

Identity must match the intended build. Before a voltage request, require
`pdController=fusb302b`, `pdState=ready`, a policy-valid `pps` or `fixed`
contract, `manualPpsEnabled=false`, `heaterEnabled=false`, and
`heaterPhysicalOutputPercent=0`. If a manual override is already active, do
not clear an owner-held setting just to manufacture an idle baseline.
Confirm the front panel remains usable and the source does not cycle its output.
Record a second status after at least 10 seconds: uptime must increase without
a restart and the contract and VIN must remain stable. Save the confirmed idle
contract kind, voltage, current, source capability summary, and VIN for the
later restoration comparison.

## Manual PPS and restoration

For each supported test point below, issue the command once, then read status
independently after settling and during a 20-second hold. The immediate `set`
response can contain the new request alongside the previous confirmed current
and VIN; it is not the acceptance sample.

| Point                | CLI command                                                                                             | Confirmed status                                                                                             |
| -------------------- | ------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------ |
| 15 V / 3 A           | `target/debug/flux-purr pd pps set --devd "$SOCKET" --device "$DEVICE" --volts 15.0 --amps 3.00 --json` | `pdContractMv=15000`, `pdContractCurrentMa=3000`, `manualPpsMv=15000`, `manualPpsMa=3000`                    |
| Adjustable PPS / 3 A | Same command with `--volts 17.3 --amps 3.00`                                                            | `pdContractKind=pps`, `pdContractMv=17300`, `pdContractCurrentMa=3000`, VIN near 17.3 V                      |
| 20 V / 3 A           | Same command with `--volts 20.0 --amps 3.00`                                                            | `pdContractMv=20000`, `pdContractCurrentMa=3000`, `pdContractPowerMw=60000`                                  |
| 20 V / 5 A           | Same command with `--volts 20.0 --amps 5.00`                                                            | `pdContractMv=20000`, `pdContractCurrentMa=5000`, `pdContractPowerMw=100000`, `pdPerformanceGuaranteed=true` |

Use this status projection for the settled and hold samples:

```sh
target/debug/flux-purr status --devd "$SOCKET" --device "$DEVICE" --json \
  | jq '{uptimeSeconds, pdState, pdContractKind, pdContractMv,
         pdContractCurrentMa, pdContractPowerMw, pdPerformanceGuaranteed,
         manualPpsEnabled, manualPpsMv, manualPpsMa, voltageMv,
         heaterEnabled, heaterPhysicalOutputPercent}'
```

Take samples at settling, then about 5, 10, 15, and 20 seconds after the
request; hold 17.3 V for at least 60 seconds. Every held sample must report a
ready PPS contract at the requested voltage/current, monotonically increasing
uptime, no visible front-panel reboot or source-output cycling, and zero
physical heater output. For the board's calibrated `voltageMv` screening, a
deviation larger than the greater of 500 mV or 5% of the request requires
investigation. If an external meter is available, record its VBUS reading too.
Avoid repeating a point already supported by an adequate receipt for the same
image and fixture.

Normally release the override and confirm automatic idle within 10 seconds:

```sh
target/debug/flux-purr pd pps clear --devd "$SOCKET" --device "$DEVICE" --json
target/debug/flux-purr status --devd "$SOCKET" --device "$DEVICE" --json \
  | jq '{uptimeSeconds, pdState, pdContractKind, pdContractMv,
         pdContractCurrentMa, manualPpsEnabled, voltageMv,
         heaterEnabled, heaterPhysicalOutputPercent}'
```

The later status must show `manualPpsEnabled=false`, a ready contract
consistent with the tested firmware's idle policy, VIN near that confirmed
contract, increasing uptime, and zero physical heater output. With unchanged
source capabilities and heater-off state, compare the contract against the
saved automatic idle baseline rather than a fixed voltage in this procedure.
If the owner explicitly asks to keep a selected voltage, leave the override
in place and record the last held status instead of clearing it. Restore any
runtime setting changed for a separate control-plane check and read it back
before leaving the bench.

## Normal-workload check

While holding a PPS voltage, use the front panel and read `status` and
`runtime get` through the CLI. Apply at least 60 seconds of read-only
control-plane load:

```sh
(
  set -euo pipefail
  for sample in $(seq 1 60); do
    target/debug/flux-purr status --devd "$SOCKET" --device "$DEVICE" --json \
      | jq -ec '{uptimeSeconds, pdState, pdContractKind, pdContractMv,
                 voltageMv, heaterPhysicalOutputPercent}'
    target/debug/flux-purr runtime get --devd "$SOCKET" --device "$DEVICE" --json \
      | jq -ec '{targetTempC, heaterEnabled}'
    sleep 1
  done
)
```

All reads must succeed. The front panel and source output must remain stable;
status must retain a ready PPS contract at the selected voltage, VIN near the
request, increasing uptime, and zero physical heater output. This tests the
user-visible effect of scheduling under ordinary control-plane work.

## Result and cleanup

Record `PASS`, `FAIL`, `NOT APPLICABLE` (capability absent), or `NOT RUN` for:

| Check                  | Required evidence                                                                                                                                                              |
| ---------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Boot and idle          | Intended firmware identity, usable front panel, stable source output, policy-valid automatic idle contract recorded as a baseline, increasing uptime.                          |
| Requested PPS voltages | Supported 15 V, 17.3 V, and 20 V points reach the requested PPS contract and VIN range without output cycling or reboot.                                                       |
| Sustained operation    | Held samples show stable VIN and uptime, usable front panel, and zero heater output; 17.3 V remains stable for at least 60 seconds.                                            |
| Normal workload        | The read loop runs for at least 60 seconds while the front panel remains usable, the requested PPS contract and VIN remain stable, and uptime increases.                       |
| Restoration or hold    | `clear` returns to the firmware's policy-valid automatic idle contract and saved baseline under unchanged conditions, or an owner instruction keeps the manual voltage active. |

The heater-off PD Sink interoperability HIL passes when all applicable checks
above pass on the intended image and source. Record the artifact SHA, status
samples, and observed hardware behavior in the receipt. Internal protocol
rules and exceptional recovery remain covered by firmware tests. Contractual
current and power fields do not establish measured high-current or heater-on
performance, which require a separately scoped test.

On any reset, unexpected power loss, missing authorized port, unsafe voltage,
or nonzero heater output, stop the sequence and preserve evidence. Clear the
manual override only if the original authorized port remains available; do
not switch to a newly enumerated port.

Stop devd in terminal A. Release only this run's port lease:

```sh
rm -- "$SOCKET"
rmdir -- "$SOCKET_DIR"
python3 "${CODEX_HOME:-$HOME/.codex}/skills/global-port-manager/scripts/port_registry.py" --json \
  release-service --scope-id "$SCOPE" --service devd
```
