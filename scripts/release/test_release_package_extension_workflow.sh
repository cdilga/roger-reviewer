#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WORKFLOW_PATH="${ROOT_DIR}/.github/workflows/release.yml"

ruby - "$WORKFLOW_PATH" <<'RUBY'
require "yaml"

workflow_path = ARGV.fetch(0)
workflow = YAML.load_file(workflow_path)
jobs = workflow.fetch("jobs")
job = jobs.fetch("package-extension")

needs = Array(job.fetch("needs"))
required_needs = ["derive-release-metadata", "verify-bridge-contract"]
missing_needs = required_needs - needs
unless missing_needs.empty?
  abort("package-extension job missing needs: #{missing_needs.join(', ')}")
end

steps = job.fetch("steps")
pack_step = steps.find { |step| step["name"] == "Package extension lane output after contract verification" }
abort("package-extension run step missing") unless pack_step

run_script = pack_step["run"].to_s
unless run_script.include?("scripts/release/package_extension_bundle.sh")
  abort("package-extension run step no longer calls scripts/release/package_extension_bundle.sh")
end

upload_step = steps.find { |step| step.dig("with", "name") == "extension-bundle" }
abort("package-extension upload step missing extension-bundle artifact") unless upload_step

artifact_path = upload_step.dig("with", "path").to_s
required_entries = [
  "dist/extension/*-extension.zip",
  "dist/extension/extension-bundle-manifest.json",
  "dist/extension/bridge-verify.json",
  "dist/extension/pack-extension.json",
]
missing_entries = required_entries.reject { |entry| artifact_path.include?(entry) }
unless missing_entries.empty?
  abort("extension-bundle upload path missing entries: #{missing_entries.join(', ')}")
end

puts "release workflow package-extension lane references bundle script + expected artifacts"
RUBY

echo "PASS: release workflow package-extension lane contract"
