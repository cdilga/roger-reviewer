#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

usage() {
  cat <<'EOF'
Usage:
  scripts/extension/launch_preloaded_browser.sh \
    --browser <edge|chrome|brave> \
    [--package-dir <path>] \
    [--profile-root <path>] \
    [--start-url <url>] \
    [--browser-binary <path-or-command>] \
    [--close-existing] \
    [--reset-profile] \
    [--dismiss-edge-prompts] \
    [--dry-run]

Notes:
  - If --package-dir is omitted, the script rebuilds Roger's unpacked extension
    by running `cargo run -q -p roger-cli --bin rr -- bridge pack-extension --robot`.
  - On macOS the default path launches a fresh app instance with `open -na`.
  - On Linux the browser binary is launched directly in the background.
  - This script is intended for local Roger setup/reload loops where agents need
    a deterministic way to preload the unpacked extension into a dedicated profile.
  - The helper seeds a minimal Chromium-style profile to suppress first-run and
    sync-promo surfaces in the dedicated Roger testing profile. For Edge on
    macOS it also seeds Edge-specific sign-in/FRE preferences and can attempt a
    soft dismissal of the known "Got it" sync modal.
EOF
}

shell_join() {
  local joined=""
  local arg
  for arg in "$@"; do
    if [[ -n "$joined" ]]; then
      joined+=" "
    fi
    joined+="$(printf '%q' "$arg")"
  done
  printf '%s\n' "$joined"
}

browser=""
package_dir=""
profile_root=""
start_url=""
browser_binary_override=""
close_existing=0
reset_profile=0
dismiss_edge_prompts=-1
dry_run=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --browser)
      browser="${2:-}"
      shift 2
      ;;
    --package-dir)
      package_dir="${2:-}"
      shift 2
      ;;
    --profile-root)
      profile_root="${2:-}"
      shift 2
      ;;
    --start-url)
      start_url="${2:-}"
      shift 2
      ;;
    --browser-binary)
      browser_binary_override="${2:-}"
      shift 2
      ;;
    --close-existing)
      close_existing=1
      shift
      ;;
    --reset-profile)
      reset_profile=1
      shift
      ;;
    --dismiss-edge-prompts)
      dismiss_edge_prompts=1
      shift
      ;;
    --dry-run)
      dry_run=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "error: unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ -z "$browser" ]]; then
  echo "error: --browser is required" >&2
  usage >&2
  exit 2
fi

case "$browser" in
  edge)
    browser_extensions_url="edge://extensions"
    browser_default_name="Microsoft Edge"
    browser_bundle_id="com.microsoft.edgemac"
    linux_candidates=("microsoft-edge" "microsoft-edge-stable" "microsoft-edge-beta" "microsoft-edge-dev")
    ;;
  chrome)
    browser_extensions_url="chrome://extensions"
    browser_default_name="Google Chrome"
    browser_bundle_id="com.google.Chrome"
    linux_candidates=("google-chrome" "google-chrome-stable" "chromium" "chromium-browser")
    ;;
  brave)
    browser_extensions_url="brave://extensions"
    browser_default_name="Brave Browser"
    browser_bundle_id="com.brave.Browser"
    linux_candidates=("brave-browser" "brave")
    ;;
  *)
    echo "error: unsupported browser '$browser'" >&2
    exit 2
    ;;
esac

profile_root="${profile_root:-${RR_EXTENSION_PROFILE_ROOT:-${HOME}/.roger/bridge/browser-profiles/${browser}}}"
start_url="${start_url:-${browser_extensions_url}}"

platform="$(uname -s)"
if [[ "$dismiss_edge_prompts" -lt 0 ]]; then
  if [[ "$browser" == "edge" && "$platform" == "Darwin" ]]; then
    dismiss_edge_prompts=1
  else
    dismiss_edge_prompts=0
  fi
fi

resolve_package_dir() {
  if [[ -n "$package_dir" ]]; then
    return 0
  fi

  local pack_json
  pack_json="$(cd "$ROOT_DIR" && cargo run -q -p roger-cli --bin rr -- bridge pack-extension --robot)"
  package_dir="$(python3 - "$pack_json" <<'PY'
import json
import sys

payload = json.loads(sys.argv[1])
package_dir = payload.get("data", {}).get("package_dir", "")
if not package_dir:
    raise SystemExit("rr bridge pack-extension did not return data.package_dir")
print(package_dir)
PY
)"
}

resolve_macos_app_bundle() {
  local candidate=""
  if [[ -n "$browser_binary_override" ]]; then
    if [[ -d "$browser_binary_override" && "$browser_binary_override" == *.app ]]; then
      printf '%s\n' "$browser_binary_override"
      return 0
    fi
    if [[ -x "$browser_binary_override" ]]; then
      printf '%s\n' "$browser_binary_override"
      return 0
    fi
    echo "error: --browser-binary does not point to an app bundle or executable: $browser_binary_override" >&2
    exit 2
  fi

  for candidate in \
    "/Applications/${browser_default_name}.app" \
    "${HOME}/Applications/${browser_default_name}.app"
  do
    if [[ -d "$candidate" ]]; then
      printf '%s\n' "$candidate"
      return 0
    fi
  done

  if command -v mdfind >/dev/null 2>&1; then
    candidate="$(mdfind "kMDItemCFBundleIdentifier == '${browser_bundle_id}'" | head -n 1 || true)"
    if [[ -n "$candidate" && -d "$candidate" ]]; then
      printf '%s\n' "$candidate"
      return 0
    fi
  fi

  echo "error: could not resolve macOS app bundle for ${browser_default_name}; install it in /Applications or pass --browser-binary <path-to-app-or-binary>" >&2
  exit 2
}

resolve_linux_browser_binary() {
  local candidate=""
  if [[ -n "$browser_binary_override" ]]; then
    if [[ "$browser_binary_override" == */* ]]; then
      if [[ -x "$browser_binary_override" ]]; then
        printf '%s\n' "$browser_binary_override"
        return 0
      fi
      echo "error: --browser-binary is not executable: $browser_binary_override" >&2
      exit 2
    fi
    if command -v "$browser_binary_override" >/dev/null 2>&1; then
      command -v "$browser_binary_override"
      return 0
    fi
    echo "error: --browser-binary command not found: $browser_binary_override" >&2
    exit 2
  fi

  for candidate in "${linux_candidates[@]}"; do
    if command -v "$candidate" >/dev/null 2>&1; then
      command -v "$candidate"
      return 0
    fi
  done

  echo "error: could not resolve a ${browser} browser binary; pass --browser-binary <path-or-command>" >&2
  exit 2
}

resolve_package_dir

if [[ ! -d "$package_dir" ]]; then
  echo "error: package directory does not exist: $package_dir" >&2
  exit 2
fi

mkdir -p "$profile_root"

close_profile_browser_processes() {
  local pattern="--user-data-dir=${profile_root}"
  local matches=""
  matches="$(ps ax -o pid=,command= | rg --fixed-strings -- "$pattern" || true)"
  if [[ -z "$matches" ]]; then
    return 0
  fi

  local pid=""
  while IFS= read -r line; do
    pid="$(awk '{print $1}' <<<"$line")"
    if [[ -n "$pid" ]]; then
      kill "$pid" >/dev/null 2>&1 || true
    fi
  done <<<"$matches"

  sleep 1

  matches="$(ps ax -o pid=,command= | rg --fixed-strings -- "$pattern" || true)"
  if [[ -z "$matches" ]]; then
    return 0
  fi

  while IFS= read -r line; do
    pid="$(awk '{print $1}' <<<"$line")"
    if [[ -n "$pid" ]]; then
      kill -9 "$pid" >/dev/null 2>&1 || true
    fi
  done <<<"$matches"
}

seed_profile_state() {
  local default_profile="${profile_root}/Default"
  mkdir -p "$default_profile"

  # Chromium-family browsers use a sentinel in the user-data-dir to track
  # whether first-run onboarding should still be shown.
  : > "${profile_root}/First Run"

  cat > "${profile_root}/Local State" <<'EOF'
{
  "browser": {
    "check_default_browser": false,
    "has_seen_welcome_page": true
  },
  "fre": {
    "has_user_seen_new_profile_fre": true
  },
  "new_device_experimentation": {
    "initial_signin_state": 3,
    "initial_sync_browser_usage_segment": 13,
    "initial_sync_ntp_feed_pref_state": 5,
    "initial_sync_session_startup_pref_state": 0
  },
  "profile": {
    "last_used": "Default"
  },
  "profiles": {
    "current_profile_name": "Default",
    "edge": {
      "guided_switch_pref": [],
      "implicit_signin": {
        "telemetry_result": 1
      }
    }
  }
}
EOF

  cat > "${default_profile}/Preferences" <<'EOF'
{
  "browser": {
    "check_default_browser": false,
    "has_seen_welcome_page": true
  },
  "distribution": {
    "import_bookmarks": false,
    "import_history": false,
    "import_home_page": false,
    "import_search_engine": false,
    "show_welcome_page": false,
    "skip_first_run_ui": true
  },
  "edge": {
    "profile_matches_os_primary_account": false
  },
  "profile": {
    "has_seen_signin_fre": true,
    "name": "Default",
    "signin_fre_seen_time": "11644473600000000",
    "using_default_avatar": true,
    "using_gaia_avatar": false
  },
  "signin": {
    "allowed": false,
    "signin_with_explicit_browser_signin_on": false
  },
  "sync": {
    "has_been_enabled": false,
    "has_setup_completed": false,
    "keep_everything_synced": false
  },
  "sync_profile_info": {
    "edge_ci_is_option_explicitly_selectedby_user": true,
    "edge_san_is_option_explicitly_selectedby_user": true
  }
}
EOF
}

maybe_dismiss_edge_prompts() {
  if [[ "$dismiss_edge_prompts" -eq 0 ]]; then
    return 0
  fi
  if [[ "$browser" != "edge" || "$platform" != "Darwin" ]]; then
    return 0
  fi
  if ! command -v osascript >/dev/null 2>&1; then
    return 0
  fi

  osascript >/dev/null 2>&1 <<EOF || true
tell application "${browser_default_name}" to activate
delay 1
tell application "System Events"
  if not (exists process "${browser_default_name}") then return
  tell process "${browser_default_name}"
    repeat 12 times
      try
        if exists (button "Got it" of front window) then
          click button "Got it" of front window
          return
        end if
      end try
      try
        key code 53
      end try
      delay 0.5
    end repeat
  end tell
end tell
EOF
}

launch_args=(
  "--no-first-run"
  "--no-default-browser-check"
  "--disable-sync"
  "--user-data-dir=${profile_root}"
  "--disable-extensions-except=${package_dir}"
  "--load-extension=${package_dir}"
  "--remote-debugging-port=0"
  "${start_url}"
)
launch_mode=""
resolved_target=""
launch_command=""

case "$platform" in
  Darwin)
    resolved_target="$(resolve_macos_app_bundle)"
    if [[ "$resolved_target" == *.app ]]; then
      launch_mode="macos_open_app_bundle"
      launch_command="$(shell_join open -na "$resolved_target" --args "${launch_args[@]}")"
    else
      launch_mode="direct_binary"
      launch_command="$(shell_join "$resolved_target" "${launch_args[@]}")"
    fi
    ;;
  Linux)
    resolved_target="$(resolve_linux_browser_binary)"
    launch_mode="direct_binary"
    launch_command="$(shell_join "$resolved_target" "${launch_args[@]}")"
    ;;
  *)
    echo "error: unsupported host platform for this helper: $platform" >&2
    exit 2
    ;;
esac

if [[ "$dry_run" -eq 1 ]]; then
  cat <<EOF
browser=${browser}
platform=${platform}
launch_mode=${launch_mode}
launch_target=${resolved_target}
package_dir=${package_dir}
profile_root=${profile_root}
start_url=${start_url}
close_existing=${close_existing}
reset_profile=${reset_profile}
dismiss_edge_prompts=${dismiss_edge_prompts}
launch_command=${launch_command}
EOF
  exit 0
fi

if [[ "$close_existing" -eq 1 ]]; then
  close_profile_browser_processes
fi

if [[ "$reset_profile" -eq 1 ]]; then
  rm -rf "$profile_root"
  mkdir -p "$profile_root"
fi

seed_profile_state

case "$launch_mode" in
  macos_open_app_bundle)
    open -na "$resolved_target" --args "${launch_args[@]}"
    ;;
  direct_binary)
    "$resolved_target" "${launch_args[@]}" >/dev/null 2>&1 &
    ;;
  *)
    echo "error: internal launch mode resolution failed" >&2
    exit 1
    ;;
esac

maybe_dismiss_edge_prompts

echo "launched ${browser} with Roger unpacked extension from ${package_dir} using profile ${profile_root}"
