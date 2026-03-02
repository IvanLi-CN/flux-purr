#!/usr/bin/env bash
set -euo pipefail

if [[ "${GITHUB_EVENT_NAME:-}" != "pull_request" ]]; then
  echo "label-gate only validates pull_request events"
  exit 0
fi

labels_json="$(jq -c '.pull_request.labels // []' "${GITHUB_EVENT_PATH}")"

type_labels=()
while IFS= read -r label; do
  [[ -n "${label}" ]] && type_labels+=("${label}")
done < <(jq -r '.[] | .name | select(startswith("type:"))' <<<"${labels_json}")

channel_labels=()
while IFS= read -r label; do
  [[ -n "${label}" ]] && channel_labels+=("${label}")
done < <(jq -r '.[] | .name | select(startswith("channel:"))' <<<"${labels_json}")

valid_types=("type:patch" "type:minor" "type:major" "type:docs" "type:skip")
valid_channels=("channel:stable" "channel:rc")

if [[ "${#type_labels[@]}" -ne 1 ]]; then
  echo "Expected exactly one type:* label, got ${#type_labels[@]}: ${type_labels[*]:-none}" >&2
  exit 1
fi

if [[ "${#channel_labels[@]}" -ne 1 ]]; then
  echo "Expected exactly one channel:* label, got ${#channel_labels[@]}: ${channel_labels[*]:-none}" >&2
  exit 1
fi

if [[ ! " ${valid_types[*]} " =~ " ${type_labels[0]} " ]]; then
  echo "Unsupported type label: ${type_labels[0]}" >&2
  exit 1
fi

if [[ ! " ${valid_channels[*]} " =~ " ${channel_labels[0]} " ]]; then
  echo "Unsupported channel label: ${channel_labels[0]}" >&2
  exit 1
fi

echo "Label gate passed: ${type_labels[0]} + ${channel_labels[0]}"
