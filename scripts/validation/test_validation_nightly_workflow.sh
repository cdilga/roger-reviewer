#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WORKFLOW_PATH="${ROOT_DIR}/.github/workflows/validation-main.yml"

ruby - "${WORKFLOW_PATH}" <<'RUBY'
require "yaml"

workflow_path = ARGV.fetch(0)
workflow = YAML.load_file(workflow_path)

on_block = workflow["on"] || workflow[true]
abort("validation-main workflow is missing an on block") unless on_block.is_a?(Hash)
push = on_block.fetch("push")
branches = push.fetch("branches")
abort("validation-main must trigger on push to main") unless branches.include?("main")
abort("validation-main must keep schedule trigger") unless on_block.key?("schedule")
abort("validation-main must keep workflow_dispatch trigger") unless on_block.key?("workflow_dispatch")

concurrency = workflow.fetch("concurrency")
abort("validation-main concurrency group must be branch-scoped") unless concurrency.fetch("group") == "validation-main-${{ github.ref_name }}"
abort("validation-main must preserve the active run while collapsing queued runs") unless concurrency.fetch("cancel-in-progress") == false

inputs = on_block.fetch("workflow_dispatch").fetch("inputs")
tier = inputs.fetch("tier")
abort("validation-main manual tier selector must be a choice input") unless tier.fetch("type") == "choice"
abort("validation-main manual tier selector must include gated/nightly/release") unless tier.fetch("options").sort == ["gated", "nightly", "release"]

steps = workflow.fetch("jobs").fetch("main").fetch("steps")
select_step = steps.find { |step| step["id"] == "select" }
abort("validation-main must select a tier dynamically") unless select_step.is_a?(Hash)

puts "validation-main workflow includes push/schedule/manual tier selection and queued-run concurrency guard"
RUBY

echo "PASS: validation-main workflow guard"
