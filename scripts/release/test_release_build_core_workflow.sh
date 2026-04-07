#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WORKFLOW_PATH="${ROOT_DIR}/.github/workflows/release.yml"

ruby - "$WORKFLOW_PATH" <<'RUBY'
require "yaml"

workflow_path = ARGV.fetch(0)
workflow = YAML.load_file(workflow_path)
entries = workflow.fetch("jobs")
  .fetch("build-core")
  .fetch("strategy")
  .fetch("matrix")
  .fetch("include")

targets = entries.map { |entry| entry.fetch("target") }
expected_targets = [
  "x86_64-unknown-linux-gnu",
  "aarch64-unknown-linux-gnu",
  "aarch64-apple-darwin",
  "x86_64-apple-darwin",
  "x86_64-pc-windows-msvc"
]

missing_targets = expected_targets - targets
unless missing_targets.empty?
  abort("release-build-core matrix is missing targets: #{missing_targets.join(', ')}")
end

linux_arm = entries.find { |entry| entry["target"] == "aarch64-unknown-linux-gnu" }
unless linux_arm && linux_arm["os"] == "ubuntu-24.04-arm"
  abort("linux/aarch64 lane must use ubuntu-24.04-arm runner")
end

puts "release workflow build-core matrix includes linux/aarch64"
RUBY

echo "PASS: release workflow build-core matrix guards linux/aarch64 lane"
