# Roger Reviewer

TUI with FrankenTUI for the default view. Backend should be a drop-in layer over an OpenCode session.

UX: Chrome/Brave extension injects a rich button into GitHub. Click opens actions plus a dropdown. Prompts can be added there directly. Add GitHub-specific keybinds in Chrome as well.

When adding extra config, default to additive behavior. Maintain a series of templates: global ones, plus either override or add mode for specific repos. Skills come from the parent. It will use worktrees. Setup needs to let it run its own isolated environment without conflicts. Add named instances so we can run two local copies of the whole app if needed, and it should smartly copy DB state from the main instance with fast diffing.

The review TUI should link to both repos and to specific artifacts, with local-schema, FrankenSQLite-backed storage. Add semantic search for reviews and index all PRs locally so lookup is extremely fast. Agents should be able to search this quickly, with `gh` CLI callbacks.

The TUI should show an itemized list of findings. Each finding can be marked, triggered for follow-up, and have clarifying questions attached in a schema-driven way. Lowest common denominator: a session must always be resumable in plain OpenCode. After loading or compacting, the system can reinsert needed context.

Normal review flow should copy Jeff's prompts: explore first, then go deep. It should automatically advance through the next prompt unless human review is needed, which the agent flags. You should be able to review all findings in one view, select only the ones you care about, and mark others ignored. This can build review-specific memory using Cass.

In general, it should work through the three reviews and prompts either automatically or recursively. It should only stop when there is no more value in continuing. When refreshing and pulling new changes, a fresh-eyes pass should still inject key prior findings so it does not need to start from zero. Finally, the PR page should have an extension-powered indicator showing whether there are unapplied or unaddressed findings. This should stay tool-agnostic: TUI-first, but with bidirectional integration into GitHub UI so findings can be actioned there too.

Key feature:

Everything maps back to sessions linked to a GitHub review, so I can always fall back to a vanilla session if needed.

Automatic worktree setup from a button click should enable running, but should not automatically run the whole platform; the agent can decide if the full platform is needed. Extend targeted export to use Keychain for credentials. Once loaded, credentials may be saved and then referenced later. Writing to dev and test should be disabled by default; do not let the agent do that. Keep it local per environment. The agent can request some FPs, but it should be warned about size. Resume needs to be better so it can load its own data. Lookup of a compressed local cache of FPs should be extremely fast, with another command able to refresh and preload enough data to request the right points. For example: "give me a few points from SA". Keep the data limited.

Out of scope:

Actually fixing bugs. However, if explicitly requested, `gh` CLI should be able to write suggestions back, but only as a prompt.
This should be configurable so I can test modes where I do want bugs fixed, but not by default and not without confirmation.
Add detailed style skills to copy Chris's style. Seek general confirmation before applying recommendations, do not post automatically, and build Roger-native approval flow so proposals can be reviewed and edited before mass posting. Use CLI as the posting method.

Architecture should support interaction from both the web extension and the TUI. Use an agnostic MVC or similar layer that can render to both. Add unit tests for the extension and TUI tests. Architecture should be standard and daemonless.

Reuse the same session for token efficiency after deciding to do the review, then inject everything needed. Provide native CLI commands that are session-aware. If invoked within a directory, it can assume the command relates to that branch/review if it is pushed remotely. Add commands like "mark everything as accepted".

One status should be "ask questions in GitHub", which formats the question into a GitHub review comment if marked. These should then be tracked as successfully applied. In practice they need rewriting for the audience.

Have periodic and explicit task triggers to search memory and meta-skill. If there are repeated failures in more complex areas, like generating suggestions and feedback, that should be captured and turned into well-scoped skills for that task.

If the architecture is right, we could later add a VSCode extension on top of the default GitHub extension so PR feedback can also be edited in VSCode, which is already a good interface. The CLI should also be able to run in that model.
