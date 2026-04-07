#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WORKFLOW_PATH="${ROOT_DIR}/.github/workflows/validation-nightly.yml"

ruby - "${WORKFLOW_PATH}" <<'RUBY'
require "yaml"

workflow_path = ARGV.fetch(0)
workflow = YAML.load_file(workflow_path)

on_block = workflow["on"] || workflow[true]
abort("validation-nightly workflow is missing an on block") unless on_block.is_a?(Hash)
push = on_block.fetch("push")
branches = push.fetch("branches")
abort("validation-nightly must trigger on push to main") unless branches.include?("main")
abort("validation-nightly must keep schedule trigger") unless on_block.key?("schedule")
abort("validation-nightly must keep workflow_dispatch trigger") unless on_block.key?("workflow_dispatch")

concurrency = workflow.fetch("concurrency")
abort("validation-nightly concurrency group must be branch-scoped") unless concurrency.fetch("group") == "validation-nightly-${{ github.ref_name }}"
abort("validation-nightly must preserve the active run while collapsing queued runs") unless concurrency.fetch("cancel-in-progress") == false

steps = workflow.fetch("jobs").fetch("nightly").fetch("steps")
debounce = steps.find { |step| step["name"] == "Debounce push-triggered nightly lane" }
abort("validation-nightly should not burn runner minutes on an in-workflow debounce sleep") if debounce

puts "validation-nightly workflow includes push trigger and queued-run concurrency guard"
RUBY

echo "PASS: validation-nightly workflow guard"
