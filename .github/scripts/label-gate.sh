#!/usr/bin/env bash
set -euo pipefail

if [[ "${GITHUB_EVENT_NAME:-}" != "pull_request" && "${GITHUB_EVENT_NAME:-}" != "pull_request_target" ]]; then
  echo "label-gate only validates pull_request events"
  exit 0
fi

if [[ -n "${LABELS_JSON:-}" ]]; then
  labels_json="$(jq -c 'if type == "array" then . else error("labels JSON must be an array") end' "${LABELS_JSON}")"
else
  labels_json="$(jq -c 'if (.pull_request.labels // []) | type == "array" then (.pull_request.labels // []) else error("event labels must be an array") end' "${GITHUB_EVENT_PATH}")"
fi
type_labels=()
while IFS= read -r label; do
  [[ -n "${label}" ]] && type_labels+=("${label}")
done < <(jq -r '.[] | .name | select(startswith("type:"))' <<<"${labels_json}")

channel_labels=()
while IFS= read -r label; do
  [[ -n "${label}" ]] && channel_labels+=("${label}")
done < <(jq -r '.[] | .name | select(startswith("channel:"))' <<<"${labels_json}")

component_labels=()
while IFS= read -r label; do
  [[ -n "${label}" ]] && component_labels+=("${label}")
done < <(jq -r '.[] | .name | select(startswith("component:"))' <<<"${labels_json}")

valid_types=("type:patch" "type:minor" "type:major" "type:docs" "type:skip")
valid_channels=("channel:stable" "channel:rc")
valid_components=("component:web" "component:firmware" "component:host-tools" "component:docs")

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
for component in "${component_labels[@]-}"; do
  [[ -z "${component}" ]] && continue
  if [[ ! " ${valid_components[*]} " =~ " ${component} " ]]; then
    echo "Unsupported component label: ${component}" >&2
    exit 1
  fi
done

echo "Label gate passed: ${type_labels[0]} + ${channel_labels[0]}"
