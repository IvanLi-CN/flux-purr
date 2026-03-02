#!/usr/bin/env bash
set -euo pipefail

if [[ "${GITHUB_EVENT_NAME:-}" == "workflow_dispatch" ]]; then
  level="${INPUT_RELEASE_LEVEL:-patch}"
  channel="${INPUT_RELEASE_CHANNEL:-stable}"
  echo "release_enabled=true" >> "${GITHUB_OUTPUT}"
  echo "release_level=${level}" >> "${GITHUB_OUTPUT}"
  echo "release_channel=${channel}" >> "${GITHUB_OUTPUT}"
  echo "release_reason=manual_dispatch" >> "${GITHUB_OUTPUT}"
  exit 0
fi

if [[ "${GITHUB_EVENT_NAME:-}" != "push" || "${GITHUB_REF:-}" != "refs/heads/main" ]]; then
  echo "release_enabled=false" >> "${GITHUB_OUTPUT}"
  echo "release_reason=unsupported_event" >> "${GITHUB_OUTPUT}"
  exit 0
fi

repo="${GITHUB_REPOSITORY:?missing GITHUB_REPOSITORY}"
sha="${GITHUB_SHA:?missing GITHUB_SHA}"
api="https://api.github.com"
auth_header="Authorization: Bearer ${GITHUB_TOKEN:?missing GITHUB_TOKEN}"
accept_header="Accept: application/vnd.github+json"

pulls_json="$(curl -fsSL -H "${auth_header}" -H "${accept_header}" "${api}/repos/${repo}/commits/${sha}/pulls")"
pr_count="$(jq 'length' <<<"${pulls_json}")"
if [[ "${pr_count}" -ne 1 ]]; then
  echo "release_enabled=false" >> "${GITHUB_OUTPUT}"
  echo "release_reason=ambiguous_or_missing_pr" >> "${GITHUB_OUTPUT}"
  exit 0
fi

pr_number="$(jq -r '.[0].number' <<<"${pulls_json}")"
labels_json="$(curl -fsSL -H "${auth_header}" -H "${accept_header}" "${api}/repos/${repo}/issues/${pr_number}/labels")"

type_labels=()
while IFS= read -r label; do
  [[ -n "${label}" ]] && type_labels+=("${label}")
done < <(jq -r '.[] | .name | select(startswith("type:"))' <<<"${labels_json}")

channel_labels=()
while IFS= read -r label; do
  [[ -n "${label}" ]] && channel_labels+=("${label}")
done < <(jq -r '.[] | .name | select(startswith("channel:"))' <<<"${labels_json}")

if [[ "${#type_labels[@]}" -ne 1 || "${#channel_labels[@]}" -ne 1 ]]; then
  echo "release_enabled=false" >> "${GITHUB_OUTPUT}"
  echo "release_reason=invalid_labels" >> "${GITHUB_OUTPUT}"
  exit 0
fi

type_label="${type_labels[0]}"
channel_label="${channel_labels[0]}"

case "${type_label}" in
  type:patch) level="patch" ;;
  type:minor) level="minor" ;;
  type:major) level="major" ;;
  type:docs|type:skip)
    echo "release_enabled=false" >> "${GITHUB_OUTPUT}"
    echo "release_reason=skip_type_label" >> "${GITHUB_OUTPUT}"
    exit 0
    ;;
  *)
    echo "release_enabled=false" >> "${GITHUB_OUTPUT}"
    echo "release_reason=unknown_type_label" >> "${GITHUB_OUTPUT}"
    exit 0
    ;;
esac

case "${channel_label}" in
  channel:stable) channel="stable" ;;
  channel:rc) channel="rc" ;;
  *)
    echo "release_enabled=false" >> "${GITHUB_OUTPUT}"
    echo "release_reason=unknown_channel_label" >> "${GITHUB_OUTPUT}"
    exit 0
    ;;
esac

echo "release_enabled=true" >> "${GITHUB_OUTPUT}"
echo "release_level=${level}" >> "${GITHUB_OUTPUT}"
echo "release_channel=${channel}" >> "${GITHUB_OUTPUT}"
echo "release_reason=pr_labels" >> "${GITHUB_OUTPUT}"
