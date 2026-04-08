#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WORKFLOW_PATH="${ROOT_DIR}/.github/workflows/release.yml"

ruby - "$WORKFLOW_PATH" <<'RUBY'
require "yaml"

workflow_path = ARGV.fetch(0)
workflow = YAML.load_file(workflow_path)
jobs = workflow.fetch("jobs")

migration = jobs.fetch("migration-rehearsal") do
  abort("release workflow must define migration-rehearsal job")
end

unless migration["if"] == "github.event_name != 'pull_request'"
  abort("migration-rehearsal must run on non-pull_request release lanes")
end

steps = migration.fetch("steps")
toolchain = steps.any? do |step|
  step.is_a?(Hash) && step["uses"] == "dtolnay/rust-toolchain@stable"
end
abort("migration-rehearsal must install stable Rust toolchain") unless toolchain

rehearsal_step = steps.find do |step|
  step.is_a?(Hash) && step["run"].to_s.include?("cargo test -p roger-storage --test release_migration_gate")
end
abort("migration-rehearsal must run cargo test -p roger-storage --test release_migration_gate") unless rehearsal_step

derive = jobs.fetch("derive-release-metadata") do
  abort("release workflow must define derive-release-metadata job")
end
needs = Array(derive["needs"])
unless needs.include?("migration-rehearsal")
  abort("derive-release-metadata must depend on migration-rehearsal")
end

puts "release workflow keeps prior-schema migration rehearsal before release generation"
RUBY

echo "PASS: release workflow migration gate guard"
