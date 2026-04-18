#!/usr/bin/env bash
set -euo pipefail

workflow_path=".github/workflows/release.yml"

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
build_core = jobs.fetch("build-core")
verify_release_assets = jobs.fetch("verify-release-assets")
windows_rehearsal = jobs.fetch("windows-install-update-rehearsal")
wsl_rehearsal = jobs.fetch("wsl-install-update-rehearsal")

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
  ".github/workflows/release.yml",
  "scripts/release/**",
  "docs/RELEASE_AND_TEST_MATRIX.md",
  "docs/RELEASE_CALVER_VERSIONING_CONTRACT.md",
  "docs/release-publish-operator-smoke.md",
]
required_pr_paths.each do |path_entry|
  unless pr_paths.include?(path_entry)
    fail!("pull_request.paths must include #{path_entry}")
  end
end

required_input_ids = ["publish_mode"]
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

needs = publish.fetch("needs")
unless needs.sort == ["derive-release-metadata", "verify-release-assets", "windows-install-update-rehearsal", "wsl-install-update-rehearsal"]
  fail!("publish-release must depend on derive-release-metadata, verify-release-assets, windows-install-update-rehearsal, and wsl-install-update-rehearsal")
end

plan_step = steps.find { |step| step["name"] == "Build release publication plan" }
fail!("missing Build release publication plan step") unless plan_step.is_a?(Hash)
plan_run = plan_step["run"]
unless plan_run.is_a?(String)
  fail!("Build release publication plan step must define a run script")
end
required_plan_flags = [
  "--upstream-verified-manifest",
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
required_artifact_entries = [
  "publish-plan/release-plan.json",
  "publish-plan/release-notes.md",
  "upstream/verify-report/release-asset-manifest.json",
  "upstream/verify-report/SHA256SUMS",
  "upstream/verify-report/release-notes-signing.md",
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
  "bash scripts/release/test_release_build_core_workflow.sh",
  "bash scripts/release/test_release_publish_workflow.sh",
  "bash scripts/release/test_publish_release.sh",
]
required_fixture_commands.each do |cmd|
  unless fixture_run.include?(cmd)
    fail!("fixture-rehearsal command must include #{cmd}")
  end
end

matrix_targets = build_core.fetch("strategy").fetch("matrix").fetch("include").map { |entry| entry.fetch("target") }
unless matrix_targets.include?("aarch64-unknown-linux-gnu")
  fail!("release build-core matrix must keep linux arm64")
end

verify_needs = verify_release_assets.fetch("needs")
unless verify_needs.sort == ["aggregate-core-manifest", "derive-release-metadata", "summarize-optional-lanes"]
  fail!("verify-release-assets must fan in aggregate-core-manifest, derive-release-metadata, and summarize-optional-lanes")
end

unless windows_rehearsal["if"] == "github.event_name != 'pull_request'"
  fail!("windows-install-update-rehearsal must run on non-pull_request release lanes")
end
unless windows_rehearsal["runs-on"] == "windows-2022"
  fail!("windows-install-update-rehearsal must run on windows-2022")
end

windows_needs = windows_rehearsal.fetch("needs")
unless windows_needs.sort == ["aggregate-core-manifest", "derive-release-metadata", "verify-release-assets"]
  fail!("windows-install-update-rehearsal must depend on derive-release-metadata, aggregate-core-manifest, and verify-release-assets")
end

windows_steps = windows_rehearsal.fetch("steps")
windows_run_step = windows_steps.find do |step|
  step["name"] == "Rehearse PowerShell install + installed-binary dry-run update"
end
fail!("missing Windows PowerShell install/update rehearsal step") unless windows_run_step.is_a?(Hash)
unless windows_run_step["shell"] == "pwsh"
  fail!("Windows install/update rehearsal step must run under pwsh")
end
windows_run = windows_run_step["run"]
unless windows_run.is_a?(String)
  fail!("Windows install/update rehearsal step must define a run script")
end
["rr-install.ps1", "rr update", "--dry-run", "--robot"].each do |token|
  unless windows_run.include?(token)
    fail!("Windows install/update rehearsal must include #{token}")
  end
end

windows_upload = windows_steps.find do |step|
  step["name"] == "Upload Windows install/update rehearsal evidence"
end
fail!("missing upload step for Windows install/update rehearsal evidence") unless windows_upload.is_a?(Hash)
windows_upload_with = windows_upload.fetch("with")
unless windows_upload_with["name"] == "windows-install-update-rehearsal"
  fail!("Windows install/update rehearsal upload artifact name must be windows-install-update-rehearsal")
end
windows_upload_path = windows_upload_with.fetch("path")
[
  "windows-install-update-rehearsal-summary.json",
  "windows-update-dry-run.json",
].each do |entry|
unless windows_upload_path.include?(entry)
    fail!("Windows install/update rehearsal upload must include #{entry}")
  end
end

unless wsl_rehearsal["if"] == "github.event_name != 'pull_request'"
  fail!("wsl-install-update-rehearsal must run on non-pull_request release lanes")
end
unless wsl_rehearsal["runs-on"] == "windows-2022"
  fail!("wsl-install-update-rehearsal must run on windows-2022")
end

wsl_needs = wsl_rehearsal.fetch("needs")
unless wsl_needs.sort == ["aggregate-core-manifest", "derive-release-metadata", "verify-release-assets"]
  fail!("wsl-install-update-rehearsal must depend on derive-release-metadata, aggregate-core-manifest, and verify-release-assets")
end

wsl_steps = wsl_rehearsal.fetch("steps")
wsl_run_step = wsl_steps.find do |step|
  step["name"] == "Rehearse WSL install + installed-binary dry-run update"
end
fail!("missing WSL install/update rehearsal step") unless wsl_run_step.is_a?(Hash)
unless wsl_run_step["shell"] == "pwsh"
  fail!("WSL install/update rehearsal step must run under pwsh")
end
wsl_run = wsl_run_step["run"]
unless wsl_run.is_a?(String)
  fail!("WSL install/update rehearsal step must define a run script")
end
["wsl.exe", "rr-install.sh", "rr update", "--dry-run", "--robot", "roger.release.wsl-install-update-rehearsal.v1"].each do |token|
  unless wsl_run.include?(token)
    fail!("WSL install/update rehearsal must include #{token}")
  end
end

wsl_upload = wsl_steps.find do |step|
  step["name"] == "Upload WSL install/update rehearsal evidence"
end
fail!("missing upload step for WSL install/update rehearsal evidence") unless wsl_upload.is_a?(Hash)
wsl_upload_with = wsl_upload.fetch("with")
unless wsl_upload_with["name"] == "wsl-install-update-rehearsal"
  fail!("WSL install/update rehearsal upload artifact name must be wsl-install-update-rehearsal")
end
wsl_upload_path = wsl_upload_with.fetch("path")
[
  "wsl-install-update-rehearsal-summary.json",
  "wsl-update-dry-run.json",
].each do |entry|
  unless wsl_upload_path.include?(entry)
    fail!("WSL install/update rehearsal upload must include #{entry}")
  end
end

release_matrix_doc = File.read("docs/RELEASE_AND_TEST_MATRIX.md")
unless release_matrix_doc.include?("wsl-install-update-rehearsal")
  fail!("RELEASE_AND_TEST_MATRIX.md must reference wsl-install-update-rehearsal evidence")
end

update_contract_doc = File.read("docs/UPDATE_RELEASE_AND_TESTED_UPGRADE_CONTRACT.md")
unless update_contract_doc.include?("| WSL user running Linux-side `rr` inside WSL |")
  fail!("UPDATE_RELEASE_AND_TESTED_UPGRADE_CONTRACT.md must keep explicit WSL user cohort row")
end
unless update_contract_doc.include?("| WSL install/update cohort |")
  fail!("UPDATE_RELEASE_AND_TESTED_UPGRADE_CONTRACT.md must keep explicit WSL proof-matrix row")
end
unless update_contract_doc.include?("wsl-install-update-rehearsal")
  fail!("UPDATE_RELEASE_AND_TESTED_UPGRADE_CONTRACT.md must reference wsl-install-update-rehearsal evidence")
end

puts("test_release_publish_workflow: PASS")
RUBY
