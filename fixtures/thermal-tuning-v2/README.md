# Thermal Tuning V2 Example

`realistic-pps5a-input/` is a deterministic recorder-shaped pps5a fixture for
reviewing the canonical report. It contains non-uniform target runs, distinct
candidate trials, thermal inertia, bounded overshoot, hold ripple, VIN/PPS
variation, and explicit rejected-trial evidence. It is software evidence only;
it is not a hardware qualification record.

Regenerate the input and render the report with the normal CLI path:

```sh
python3 scripts/generate_thermal_tuning_v2_mock.py \
  fixtures/thermal-tuning-v2/realistic-pps5a-input

cargo run --quiet --manifest-path tools/flux-purr-devd/Cargo.toml --bin flux-purr -- \
  --devd /tmp/flux-purr-no-device.sock thermal report render-v2 \
  --input-dir fixtures/thermal-tuning-v2/realistic-pps5a-input \
  --output-dir /tmp/flux-purr-thermal-v2-realistic-example
```

The second command is the only report renderer. It produces the canonical
five-file `thermal-tuning-v2` bundle.
