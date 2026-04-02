#!/usr/bin/env bash
set -euo pipefail

workflow_path=".github/workflows/release-publish.yml"

ruby - "${workflow_path}" <<'RUBY'
require "yaml"

path = ARGV.fetch(0)
workflow = YAML.load_file(path)

def fail!(message)
  warn(message)
  exit(1)
end

jobs = workflow.fetch("jobs")
publish = jobs.fetch("publish-release")
fixture = jobs.fetch("fixture-rehearsal")

unless publish["environment"] == "release-publish-approval"
  fail!("expected publish-release.environment=release-publish-approval")
end
unless publish["if"] == "github.event_name == 'workflow_dispatch'"
  fail!("publish-release job must be gated to workflow_dispatch")
end
unless fixture["if"] == "github.event_name == 'pull_request'"
  fail!("fixture-rehearsal job must be gated to pull_request")
end

permissions = workflow.fetch("permissions")
unless permissions["contents"] == "write" && permissions["actions"] == "read"
  fail!("workflow permissions must keep contents=write and actions=read")
end

on_block = workflow["on"]
on_block = workflow[true] if on_block.nil?
fail!("missing workflow trigger block") unless on_block.is_a?(Hash)

inputs = on_block.fetch("workflow_dispatch").fetch("inputs")
pull_request = on_block.fetch("pull_request")
pr_paths = pull_request.fetch("paths")
unless pr_paths.is_a?(Array)
  fail!("pull_request.paths must be an array")
end
required_pr_paths = [
  ".github/workflows/release-publish.yml",
  "scripts/release/publish_release.py",
  "scripts/release/test_release_publish_workflow.sh",
  "scripts/release/test_publish_release.sh",
  "scripts/release/validate_upstream_run.sh",
  "scripts/release/test_validate_upstream_run.sh",
]
required_pr_paths.each do |path_entry|
  unless pr_paths.include?(path_entry)
    fail!("pull_request.paths must include #{path_entry}")
  end
end

required_input_ids = ["core_run_id", "verify_run_id", "publish_mode"]
required_input_ids.each do |id|
  input = inputs[id]
  fail!("missing workflow_dispatch input #{id}") unless input.is_a?(Hash)
  fail!("input #{id} must be required") unless input["required"] == true
end

publish_mode = inputs.fetch("publish_mode")
unless publish_mode["options"].is_a?(Array) &&
       publish_mode["options"].sort == ["draft", "publish"]
  fail!("publish_mode options must be [draft, publish]")
end
unless publish_mode["default"] == "draft"
  fail!("publish_mode default must be draft")
end

operator_smoke_ack = inputs.fetch("operator_smoke_ack")
unless operator_smoke_ack["type"] == "boolean"
  fail!("operator_smoke_ack input must be boolean")
end
unless operator_smoke_ack["default"] == false
  fail!("operator_smoke_ack default must be false")
end

steps = publish.fetch("steps")

validate_step = steps.find do |step|
  step["name"] == "Validate upstream run identities and successful completion"
end
fail!("missing upstream run validation step") unless validate_step.is_a?(Hash)
validate_run_script = validate_step["run"]
unless validate_run_script.is_a?(String) &&
       validate_run_script.include?("scripts/release/validate_upstream_run.sh")
  fail!("upstream run validation step must call scripts/release/validate_upstream_run.sh")
end

plan_step = steps.find { |step| step["name"] == "Build release publication plan" }
fail!("missing Build release publication plan step") unless plan_step.is_a?(Hash)
plan_run = plan_step["run"]
unless plan_run.is_a?(String)
  fail!("Build release publication plan step must define a run script")
end
required_plan_flags = [
  "--upstream-verified-manifest",
  "--core-run-url",
  "--verify-run-url",
  "--publish-mode",
]
required_plan_flags.each do |flag|
  unless plan_run.include?(flag)
    fail!("Build release publication plan must include #{flag}")
  end
end

upload_step = steps.find { |step| step["name"] == "Upload publish-plan evidence" }
fail!("missing Upload publish-plan evidence step") unless upload_step.is_a?(Hash)

artifact_path = upload_step.fetch("with").fetch("path")
unless artifact_path.include?("run-*.json")
  fail!("publish-plan evidence must retain run-*.json payloads")
end
required_artifact_entries = [
  "publish-plan/release-plan.json",
  "publish-plan/release-notes.md",
  "upstream/reverified/release-asset-manifest.json",
  "upstream/reverified/SHA256SUMS",
  "upstream/reverified/release-notes-signing.md",
]
required_artifact_entries.each do |entry|
  unless artifact_path.include?(entry)
    fail!("publish-plan evidence must retain #{entry}")
  end
end

fixture_steps = fixture.fetch("steps")
fixture_script_step = fixture_steps.find do |step|
  run = step["run"]
  run.is_a?(String) && run.include?("test_release_publish_workflow.sh")
end
fail!("fixture-rehearsal must execute test_release_publish_workflow.sh") if fixture_script_step.nil?
fixture_run = fixture_script_step["run"]
required_fixture_commands = [
  "bash scripts/release/test_release_publish_workflow.sh",
  "bash scripts/release/test_validate_upstream_run.sh",
  "bash scripts/release/test_publish_release.sh",
]
required_fixture_commands.each do |cmd|
  unless fixture_run.include?(cmd)
    fail!("fixture-rehearsal command must include #{cmd}")
  end
end

puts("test_release_publish_workflow: PASS")
RUBY
