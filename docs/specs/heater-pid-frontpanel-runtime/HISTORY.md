# Heater PID frontpanel runtime history

## Legacy identity

- Former legacy ID: `q2aw6`.

## Lifecycle

- `active`: this topic is the current heater, fan, and dashboard runtime truth source.

## Partial replacement

- It locally supersedes older heater/fan runtime wording in `frontpanel-ui-contract` and `frontpanel-input-interaction`; those topics retain their visual and input contracts.

## Calibration boundary

- ADR 0019 supersedes resistance-from-contract-current calibration and the fixed auxiliary-current reserve. Current calibration retains whole-device supply observations and an estimated thermal-model trace; obsolete heater/model records are absent.
