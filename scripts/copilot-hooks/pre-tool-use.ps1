$inputJson = [Console]::In.ReadToEnd()
$auditDir = $env:RR_COPILOT_HOOK_AUDIT_DIR
if ($auditDir) {
  New-Item -ItemType Directory -Path $auditDir -Force | Out-Null
  Add-Content -Path (Join-Path $auditDir "pre-tool-use.jsonl") -Value $inputJson
}

if ($inputJson -match '"toolName"\s*:\s*"bash"') {
  @{ permissionDecision = "deny"; permissionDecisionReason = "Roger review_readonly policy denies shell execution during Copilot review sessions" } |
    ConvertTo-Json -Compress
  exit 0
}

if ($inputJson -match '"toolName"\s*:\s*"(edit|create|write)"') {
  @{ permissionDecision = "deny"; permissionDecisionReason = "Roger review_readonly policy denies repository writes during Copilot review sessions" } |
    ConvertTo-Json -Compress
  exit 0
}

if ($inputJson -match 'gh pr review|gh api|gh issue comment|gh pr comment') {
  @{ permissionDecision = "deny"; permissionDecisionReason = "Roger review policy forbids direct GitHub mutation commands" } |
    ConvertTo-Json -Compress
  exit 0
}
