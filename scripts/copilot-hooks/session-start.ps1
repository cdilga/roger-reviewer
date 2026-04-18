$inputJson = [Console]::In.ReadToEnd()
$artifactPath = $env:RR_COPILOT_SESSION_START_ARTIFACT
$auditDir = $env:RR_COPILOT_HOOK_AUDIT_DIR
$worktreeRoot = if ($env:RR_COPILOT_WORKTREE_ROOT) { $env:RR_COPILOT_WORKTREE_ROOT } else { (Get-Location).Path }
$attemptNonce = $env:RR_COPILOT_ATTEMPT_ID
$policyDigest = $env:RR_COPILOT_POLICY_PROFILE_DIGEST
$copilotHome = if ($env:COPILOT_HOME) { $env:COPILOT_HOME } elseif ($env:USERPROFILE) { Join-Path $env:USERPROFILE ".copilot" } else { "" }
$stateRoot = if ($copilotHome) { Join-Path $copilotHome "session-state" } else { "" }

if ($auditDir) {
  New-Item -ItemType Directory -Path $auditDir -Force | Out-Null
  Add-Content -Path (Join-Path $auditDir "session-start.jsonl") -Value $inputJson
}

if (-not $artifactPath) {
  exit 0
}

$sessionId = ""
if ($inputJson) {
  try {
    $parsed = $inputJson | ConvertFrom-Json
    if ($parsed.PSObject.Properties.Name -contains "sessionId") {
      $sessionId = [string]$parsed.sessionId
    }
  } catch {
  }
}
if ($stateRoot -and (Test-Path $stateRoot)) {
  if (-not $sessionId) {
    for ($attempt = 0; $attempt -lt 20; $attempt++) {
      $dir = Get-ChildItem -Path $stateRoot -Directory | Sort-Object LastWriteTimeUtc -Descending | Select-Object -First 1
      if ($dir) {
        $sessionId = $dir.Name
        break
      }
      Start-Sleep -Milliseconds 250
    }
  }
}

New-Item -ItemType Directory -Path (Split-Path -Parent $artifactPath) -Force | Out-Null
$artifact = @{
  hook = "session-start"
  payload = @{
    provider = "copilot"
    session_id = $sessionId
    worktree_root = $worktreeRoot
    launch_profile_id = "profile-open-pr"
    attempt_nonce = $attemptNonce
    policy_digest = $policyDigest
  }
}
$artifact | ConvertTo-Json -Depth 4 | Set-Content -Path $artifactPath
