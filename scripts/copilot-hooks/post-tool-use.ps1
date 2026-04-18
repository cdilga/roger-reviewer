$inputJson = [Console]::In.ReadToEnd()
$auditDir = $env:RR_COPILOT_HOOK_AUDIT_DIR
if (-not $auditDir) {
  exit 0
}

New-Item -ItemType Directory -Path $auditDir -Force | Out-Null
Add-Content -Path (Join-Path $auditDir "post-tool-use.jsonl") -Value $inputJson
