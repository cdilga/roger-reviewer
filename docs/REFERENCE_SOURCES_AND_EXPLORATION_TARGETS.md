# Reference Sources and Exploration Targets

This document is the reference index for external standards, official docs, and
approved prior-art targets that may inform Roger architecture or later spikes.

It exists so future agents do not have to rediscover the same references from
scratch.

## Usage Rules

- treat these sources as references, not authority
- the authority order is still user instructions, `AGENTS.md`, the canonical
  plan, and accepted ADRs
- if a reference changes Roger's direction, record the decision in the canonical
  plan and an ADR rather than leaving the reference as ambient truth
- do not import code directly from reference repos into Roger Reviewer
- use `_exploration/` for stable long-lived local clones and
  `/tmp/roger-reference-projects/` for larger or temporary spike clones

## Official External Sources

### Adopt or actively use

#### Chrome Native Messaging

- Status: adopted for the serious `0.1.x` browser bridge
- Why it matters: one-shot daemonless request/response bridge from extension to
  local Roger process
- Source: <https://developer.chrome.com/docs/extensions/develop/concepts/native-messaging>

#### JSON Schema Draft 2020-12

- Status: adopt for Roger-owned external contracts and validation fixtures
- Why it matters: stable schema publication and validation for findings packs,
  bridge envelopes, and other machine-facing contracts
- Source: <https://json-schema.org/draft/2020-12/draft-bhutton-json-schema-00>

#### OpenAI Structured Outputs

- Status: capability reference, not Roger-owned truth
- Why it matters: supports schema-constrained output with a JSON Schema subset
- Source: <https://platform.openai.com/docs/guides/structured-outputs?api-mode=chat>

#### Gemini Structured Output

- Status: capability reference, not Roger-owned truth
- Why it matters: supports schema-constrained output with a provider-specific
  JSON Schema subset
- Source: <https://ai.google.dev/gemini-api/docs/structured-output>

### Defer to edge spikes only

#### Model Context Protocol (MCP)

- Status: defer
- Why it matters: plausible future tool/context edge for exposing Roger
  resources, tools, and bounded review context
- Why it is not core: it does not solve Roger's findings ledger, approval
  gates, repair loop, or session durability
- Source: <https://modelcontextprotocol.io/specification/2024-11-05/index>

#### Agent Communication Protocol (ACP)

- Status: reject as a `0.1.x` core architecture; possible future edge only if
  Roger later needs remote agent-to-agent service interoperability
- Why it matters: remote reusable-agent protocol reference
- Why it is not core: Roger is not trying to become an HTTP-first agent service
  platform in `0.1.x`
- Sources:
  - <https://agentcommunicationprotocol.dev/about/mission-and-team>
  - <https://agentcommunicationprotocol.dev/how-to/wrap-existing-agent>

### Reference only, partial fit

#### SARIF 2.1.0 and GitHub SARIF support

- Status: reference only in `0.1.x`; candidate future one-way export adapter
- Why it matters: strong prior art for result identity, locations, fingerprints,
  rule/result metadata, and suppression/export thinking
- Why it is not a full fit: Roger findings may also carry runtime evidence,
  clarification threads, approval state, outbound draft lineage, and repair
  states that exceed static-analysis interchange scope
- Sources:
  - <https://docs.oasis-open.org/sarif/sarif/v2.1.0/sarif-v2.1.0.html>
  - <https://docs.github.com/en/enterprise-cloud@latest/code-security/reference/code-scanning/sarif-support-for-code-scanning>

#### Language Server Protocol (LSP)

- Status: defer unless an editor-surface spike starts
- Why it matters: reference for later editor-facing surfaces, not for Roger's
  core review model
- Why it is not core: LSP is editor-centric and diagnostic-centric; Roger is a
  review-session system with approval and durable findings lineage
- Source: <https://microsoft.github.io/language-server-protocol/>

## Existing Local Exploration References

These are the current stable local reference clones under `_exploration/`.

### `frankentui`

- Role: Rust TUI runtime and model constraints
- Why it matters: confirms the Rust TUI and sync foreground event-loop posture

### `cass`

- Role: local-first search and indexing prior art
- Why it matters: Tantivy/FastEmbed patterns and retrieval ergonomics

### `asupersync`

- Role: daemonless bridge/runtime reference ideas
- Why it matters: reference for bridge and runtime edges, not a blueprint

### `pi_agent_rust`

- Role: deferred future harness-admission reference
- Why it matters: gives Roger a concrete local prior-art target for evaluating a
  post-`0.1.0` Pi-Agent adapter without widening current support claims

## Candidate `/tmp` Spike Queue

Use `/tmp/roger-reference-projects/` for these when a focused spike is approved.

### `sarif-reference-pack`

- Goal: study result fingerprints, locations, suppression semantics, and export
  shape without forcing Roger into a static-analysis result model
- Inputs: SARIF spec, GitHub SARIF support docs, small example producers or
  consumers

### `editor-surface-pack`

- Goal: study later VS Code / GitHub Copilot / editor-hosted integration paths
  only when an editor-surface spike is active
- Inputs: minimal editor/client protocol references rather than giant IDE code
  bases by default

### `tool-context-edge-pack`

- Goal: study MCP or similar edge exposure only after Roger's own external
  contracts are defined
- Inputs: protocol specs, small example clients/servers, and Roger-owned
  contract adapters

## Roger-Specific Interpretation Rules

- Roger is agent-first and may gather runtime evidence when the active safety
  posture allows it
- Roger is not limited to static-analysis-only result shapes
- the default PR output should still collapse down to a small number of
  GitHub-compliant comments, questions, or suggestion blocks
- every external reference is subordinate to Roger-owned domain rules, approval
  gates, and the plain-OpenCode fallback requirement
