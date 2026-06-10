#!/usr/bin/env bash

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
harness_dir="${repo_root}/apps/extension/testing"
harness_entry="${harness_dir}/popup_render_harness.cjs"
default_output_dir="${harness_dir}/artifacts/latest"

if ! command -v node >/dev/null 2>&1; then
  echo "node is required to run the popup render harness." >&2
  exit 1
fi

if ! command -v bun >/dev/null 2>&1 && ! command -v npm >/dev/null 2>&1; then
  echo "bun or npm is required to install popup render harness dependencies." >&2
  exit 1
fi

if [[ ! -d "${harness_dir}/node_modules/jsdom" ]]; then
  if command -v bun >/dev/null 2>&1; then
    echo "Installing popup render harness dependencies with bun install..." >&2
    (cd "${harness_dir}" && bun install)
  else
    echo "Installing popup render harness dependencies with npm install..." >&2
    npm install --prefix "${harness_dir}"
  fi
fi

has_output_dir=0
previous=""
for arg in "$@"; do
  if [[ "${arg}" == --output-dir=* ]]; then
    has_output_dir=1
  fi
  if [[ "${previous}" == "--output-dir" ]]; then
    has_output_dir=1
  fi
  previous="${arg}"
done

cmd=(node "${harness_entry}")
if [[ "${has_output_dir}" -eq 0 ]]; then
  cmd+=(--output-dir "${default_output_dir}")
fi
cmd+=("$@")

"${cmd[@]}"
