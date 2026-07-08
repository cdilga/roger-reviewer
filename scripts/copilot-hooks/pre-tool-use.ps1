# Roger review_readonly pre-tool-use policy hook (PowerShell twin).
#
# Mirrors scripts/copilot-hooks/pre-tool-use.sh. Baseline posture DENIES every
# bash / edit / create / write tool call and every direct GitHub mutation
# command. A FAIL-CLOSED carve-out permits ONLY:
#   - `rr agent <op> ...`  (the in-session worker transport)
#   - `rr status|findings|sessions|search ... --robot`  (read-only robot surfaces)
# STRICT matcher: the command is rejected if it contains any of the shell
# chaining / quoting / expansion metacharacters  \ " ; & | ` $ < > ( )  and must
# start literally with `rr ` (leading env=VAL / `cd <dir> &&` prefixes rejected).
# gh-write denial is preserved. Both allow and deny decisions are recorded to
# ${RR_COPILOT_HOOK_AUDIT_DIR}/pre-tool-use-decisions.jsonl with the matched rule.

$inputJson = [Console]::In.ReadToEnd()
$auditDir = $env:RR_COPILOT_HOOK_AUDIT_DIR

if ($auditDir) {
  New-Item -ItemType Directory -Path $auditDir -Force | Out-Null
  Add-Content -Path (Join-Path $auditDir "pre-tool-use.jsonl") -Value $inputJson
}

$script:MatchedCommand = ""

function Emit-Decision([string]$decision, [string]$rule, [string]$tool) {
  if (-not $auditDir) { return }
  $record = @{ hook = "pre-tool-use"; decision = $decision; rule = $rule; tool = $tool; command = $script:MatchedCommand } |
    ConvertTo-Json -Compress
  Add-Content -Path (Join-Path $auditDir "pre-tool-use-decisions.jsonl") -Value $record
}

function Deny([string]$reason, [string]$rule, [string]$tool) {
  Emit-Decision "deny" $rule $tool
  @{ permissionDecision = "deny"; permissionDecisionReason = $reason } | ConvertTo-Json -Compress
  exit 0
}

function Allow([string]$rule, [string]$tool) {
  Emit-Decision "allow" $rule $tool
  @{ permissionDecision = "allow"; permissionDecisionReason = "Roger review_readonly policy permits the Roger worker transport / read-only robot surface" } |
    ConvertTo-Json -Compress
  exit 0
}

$toolName = ""
if ($inputJson -match '"toolName"\s*:\s*"bash"') { $toolName = "bash" }
elseif ($inputJson -match '"toolName"\s*:\s*"(edit|create|write)"') { $toolName = $Matches[1] }

if ($toolName -in @("edit", "create", "write")) {
  # Scoped exception: create/write under the Roger-owned worker inbox
  # (RR_WORKER_INBOX_DIR) is allowed so the worker can stage its stage-result
  # request JSON. Edits and everything outside the inbox stay denied.
  $inboxRoot = $env:RR_WORKER_INBOX_DIR
  if ($inboxRoot -and $toolName -ne "edit") {
    $targetPath = $null
    if ($inputJson -match '"(?:path|filePath|file_path)"\s*:\s*"([^"]+)"') { $targetPath = $Matches[1] }
    if ($targetPath -and ($targetPath -notmatch '\.\.') -and $targetPath.StartsWith("$inboxRoot/")) {
      Allow "worker_inbox_write" $toolName
    }
  }
  Deny "Roger review_readonly policy denies repository writes during Copilot review sessions (worker inbox excepted)" "repo_write_denied" $toolName
}

if ($toolName -eq "bash") {
  $command = $null
  try {
    $obj = $inputJson | ConvertFrom-Json
    if ($obj.toolArgs) {
      $toolArgs = $obj.toolArgs | ConvertFrom-Json
      $command = $toolArgs.command
    }
  } catch {
    $command = $null
  }

  if ([string]::IsNullOrEmpty($command)) {
    Deny "Roger review_readonly policy denies shell execution during Copilot review sessions" "bash_no_command" "bash"
  }
  $script:MatchedCommand = $command

  if ($command -match 'gh pr review|gh api|gh issue comment|gh pr comment') {
    Deny "Roger review policy forbids direct GitHub mutation commands" "gh_write_denied" "bash"
  }

  # Reject chaining / quoting / expansion metacharacters.
  if ($command -match '[\\";&|`$<>()]') {
    Deny "Roger review_readonly policy denies chained or quoted shell commands" "bash_metacharacter" "bash"
  }

  $trimmed = $command.TrimStart()

  if ($trimmed -like 'rr agent *') {
    Allow "rr_agent_worker_transport" "bash"
  }

  if ($trimmed -like 'rr status *' -or $trimmed -like 'rr findings *' -or
      $trimmed -like 'rr sessions *' -or $trimmed -like 'rr search *') {
    if ((" " + $trimmed + " ") -like '* --robot *') {
      Allow "rr_readonly_robot_surface" "bash"
    }
    Deny "Roger review_readonly policy allows read-only rr surfaces only with --robot" "rr_readonly_missing_robot" "bash"
  }

  Deny "Roger review_readonly policy denies shell execution during Copilot review sessions" "bash_not_allowlisted" "bash"
}

# Non-bash / non-write tools: honor gh-write denial on raw text, then allow.
if ($inputJson -match 'gh pr review|gh api|gh issue comment|gh pr comment') {
  Deny "Roger review policy forbids direct GitHub mutation commands" "gh_write_denied" $toolName
}

exit 0
