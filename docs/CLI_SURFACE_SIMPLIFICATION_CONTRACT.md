# CLI Surface Simplification Contract (v2)

This contract defines the target Roger CLI shape. It is an
implementation-facing support contract and the grammar authority for
Track 1 of `docs/PRODUCT_SURFACE_RECOVERY_AND_RELEASE_PLAN.md`.

## Goal

Roger's CLI should feel like one local review product, not a bag of plumbing
verbs. The operator learns seven words:

- `rr doctor`: check whether Roger can run
- `rr queue`: choose review work
- `rr review`: start or re-enter review work
- `rr open`: use the local cockpit
- `rr findings`: inspect and search review output
- `rr send`: explicitly triage/draft/edit/approve/post outbound communication
- `rr setup`: install, update, and repair local integrations

Two machine surfaces stay explicit and out of the operator's way:

- `rr api docs <topic>`: machine-contract documentation (robot-docs)
- `rr agent <op>`: the in-session worker transport (unchanged)

Existing commands remain supported as quiet compatibility aliases. Help,
README, and guided repair text prefer the simple names.

## Target grammar (final)

```sh
rr doctor [--provider opencode|codex|gemini|claude|copilot]
rr queue [--repo owner/repo] [--limit n]
rr review [--pr n] [--repo owner/repo] [--provider p] [--interactive]
rr review --resume [--pr n | --session id]
rr open [--pr n | --session id]
rr findings [--pr n | --session id]
rr findings --query <text> [--repo owner/repo]      # prior-review search
rr findings --sessions [--repo owner/repo]          # session listing
rr send triage --finding <id>... --state <state>
rr send draft (--finding <id>... | --all-findings)
rr send edit --draft <id> (--body-file <f> | --editor)
rr send approve --batch <id>
rr send post --batch <id>
rr setup extension [--browser edge|chrome|brave]
rr setup doctor [--browser b] [--live]
rr setup fetch [--version YYYY.MM.DD]
rr setup update [--dry-run | --yes]
rr setup assets install|status|verify
rr setup uninstall
rr api docs guide|commands|schemas|workflows
rr agent <operation> [--task-file ...]
```

## Compatibility mapping (all old names stay routable)

| Old | New preferred | Routing rule |
| --- | --- | --- |
| `rr prs` | `rr queue` | same enum variant (landed) |
| `rr tui` | `rr open` | same enum variant (landed) |
| `rr resume` | `rr review --resume` | both route to the resume handler |
| `rr return` | `rr return` (unchanged) | stays top-level; it is a deliberate, narrow verb |
| `rr search --query q` | `rr findings --query q` | routes to search handler |
| `rr sessions` | `rr findings --sessions` | routes to sessions handler |
| `rr triage/draft/approve/post` | `rr send <sub>` | container routes to the same fail-closed handlers |
| `rr extension <sub>` | `rr setup <sub>` | `setup extension` = `extension setup`; `setup doctor/fetch/uninstall` map 1:1 |
| `rr update` | `rr setup update` | same handler |
| `rr assets <sub>` | `rr setup assets <sub>` | same handlers |
| `rr init` | (absorbed) | store auto-bootstraps; `init` stays as hidden compat |
| `rr bridge <sub>` | (hidden) | dev/repair surface; removed from operator help, kept routable |
| `rr robot-docs` | `rr api docs` | same handler |

## Parser and help requirements

1. **Container parsing** follows the established `bridge`/`extension`/`assets`
   positional-subcommand pattern (`parse_args`, `packages/cli/src/lib.rs`).
2. **Per-command help**: `--help`/`-h` at any argv position prints usage for
   the named command (or the global help for bare `rr --help`). The current
   behavior (`--help` recognized only as the first token; `rr review --help`
   → "unknown flag") is a bug this contract fixes.
3. **Positive flag whitelists for every command.** The ten commands that
   currently lack one (`review, resume, return, sessions, search, status,
   findings, bridge, extension, robot-docs`) gain explicit "rr X only
   supports ..." rejection messages, same style as the nine that have one.
4. `--dry-run` is rejected (not silently ignored) by every command that does
   not implement it.
5. `rr --help` leads with the seven verbs and the primary flow; machine and
   repair surfaces live in a short trailing section; `bridge` disappears from
   help entirely.

## Robot schema stability

- Aliases emit the **underlying** command's schema id (e.g. `rr send post
  --robot` emits `rr.robot.post.v1`; `rr findings --query --robot` emits
  `rr.robot.search.v1`; `rr setup update --robot` emits `rr.robot.update.v1`).
  No new schema ids in this slice; `rr api docs schemas` documents the
  alias→schema mapping truthfully.
- `rr agent` remains a separate transport and continues to reject `--robot`.
- Exit-code contract unchanged (`docs/ROBOT_CLI_CONTRACT.md`).

## Non-negotiables

- `rr send post` remains visibly elevated and bound to an exact locally
  approved draft batch.
- `rr send edit` on an approved draft invalidates the approval token and
  fails closed toward re-approval; it never edits a posted batch.
- No alias may bypass stale-state, target-binding, approval-token, or
  provider capability checks — containers route to the same handlers, they do
  not reimplement them.
- Browser setup and native-messaging repair are never presented as ordinary
  review actions.
- Machine surfaces do not crowd `rr --help`.

## Landed so far

- `rr queue` / `rr open` aliases (commit `0297076`).
- Help text leads with simplified vocabulary (partial; still names `send`
  and `setup` before they exist — this contract closes that gap).

## Acceptance

A slice claiming Track 1 completion must prove:

- every grammar line above parses and routes to the correct handler
  (parser-level unit tests, alias × handler matrix)
- per-command `--help` works at any argv position for all commands
- every command has a positive flag whitelist with actionable rejection text
- `rr --help` output matches the seven-verb structure; README uses preferred
  names first; robot docs describe the alias→schema mapping
- outbound and setup mutation flows remain visibly explicit
- full workspace test suite green
