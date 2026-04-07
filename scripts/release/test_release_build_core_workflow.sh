#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WORKFLOW_PATH="${ROOT_DIR}/.github/workflows/release-build-core.yml"

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

puts "release-build-core workflow target matrix includes linux/aarch64"

aggregate_steps = workflow.fetch("jobs")
  .fetch("aggregate-core-manifest")
  .fetch("steps")

installer_upload_step = aggregate_steps.find do |step|
  step.is_a?(Hash) &&
    step["name"] == "Upload installer bootstrap scripts for release-page entrypoints"
end
abort("release-build-core must upload installer bootstrap scripts as release assets") unless installer_upload_step.is_a?(Hash)

with_block = installer_upload_step["with"]
abort("installer bootstrap upload step must define with.path") unless with_block.is_a?(Hash)
path_block = with_block["path"].to_s
unless path_block.include?("scripts/release/rr-install.sh") && path_block.include?("scripts/release/rr-install.ps1")
  abort("installer bootstrap upload step must include rr-install.sh and rr-install.ps1")
end
RUBY

echo "PASS: release-build-core workflow target matrix guards linux/aarch64 lane"
