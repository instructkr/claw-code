# ROADMAP.md

# Clawable Coding Harness Roadmap

## Goal

Turn claw-code into the most **clawable** coding harness:
- no human-first terminal assumptions
- no fragile prompt injection timing
- no opaque session state
- no hidden plugin or MCP failures
- no manual babysitting for routine recovery

This roadmap assumes the primary users are **claws wired through hooks, plugins, sessions, and channel events**.

## Definition of "clawable"

A clawable harness is:
- deterministic to start
- machine-readable in state and failure modes
- recoverable without a human watching the terminal
- branch/test/worktree aware
- plugin/MCP lifecycle aware
- event-first, not log-first
- capable of autonomous next-step execution

## Current Pain Points

### 1. Session boot is fragile
- trust prompts can block TUI startup
- prompts can land in the shell instead of the coding agent
- "session exists" does not mean "session is ready"

### 2. Truth is split across layers
- tmux state
- clawhip event stream
- git/worktree state
- test state
- gateway/plugin/MCP runtime state

### 3. Events are too log-shaped
- claws currently infer too much from noisy text
- important states are not normalized into machine-readable events

### 4. Recovery loops are too manual
- restart worker
- accept trust prompt
- re-inject prompt
- detect stale branch
- retry failed startup
- classify infra vs code failures manually

### 5. Branch freshness is not enforced enough
- side branches can miss already-landed main fixes
- broad test failures can be stale-branch noise instead of real regressions

### 6. Plugin/MCP failures are under-classified
- startup failures, handshake failures, config errors, partial startup, and degraded mode are not exposed cleanly enough

### 7. Human UX still leaks into claw workflows
- too much depends on terminal/TUI behavior instead of explicit agent state transitions and control APIs

## Product Principles

1. **State machine first** — every worker has explicit lifecycle states.
2. **Events over scraped prose** — channel output should be derived from typed events.
3. **Recovery before escalation** — known failure modes should auto-heal once before asking for help.
4. **Branch freshness before blame** — detect stale branches before treating red tests as new regressions.
5. **Partial success is first-class** — e.g. MCP startup can succeed for some servers and fail for others, with structured degraded-mode reporting.
6. **Terminal is transport, not truth** — tmux/TUI may remain implementation details, but orchestration state must live above them.
7. **Policy is executable** — merge, retry, rebase, stale cleanup, and escalation rules should be machine-enforced.

## Roadmap

## Phase 1 — Reliable Worker Boot

### 1. Ready-handshake lifecycle for coding workers
Add explicit states:
- `spawning`
- `trust_required`
- `ready_for_prompt`
- `prompt_accepted`
- `running`
- `blocked`
- `finished`
- `failed`

Acceptance:
- prompts are never sent before `ready_for_prompt`
- trust prompt state is detectable and emitted
- shell misdelivery becomes detectable as a first-class failure state

### 2. Trust prompt resolver
Add allowlisted auto-trust behavior for known repos/worktrees.

Acceptance:
- trusted repos auto-clear trust prompts
- events emitted for `trust_required` and `trust_resolved`
- non-allowlisted repos remain gated

### 3. Structured session control API
Provide machine control above tmux:
- create worker
- await ready
- send task
- fetch state
- fetch last error
- restart worker
- terminate worker

Acceptance:
- a claw can operate a coding worker without raw send-keys as the primary control plane

## Phase 2 — Event-Native Clawhip Integration

### 4. Canonical lane event schema
Define typed events such as:
- `lane.started`
- `lane.ready`
- `lane.prompt_misdelivery`
- `lane.blocked`
- `lane.red`
- `lane.green`
- `lane.commit.created`
- `lane.pr.opened`
- `lane.merge.ready`
- `lane.finished`
- `lane.failed`
- `branch.stale_against_main`

Acceptance:
- clawhip consumes typed lane events
- Discord summaries are rendered from structured events instead of pane scraping alone

### 5. Failure taxonomy
Normalize failure classes:
- `prompt_delivery`
- `trust_gate`
- `branch_divergence`
- `compile`
- `test`
- `plugin_startup`
- `mcp_startup`
- `mcp_handshake`
- `gateway_routing`
- `tool_runtime`
- `infra`

Acceptance:
- blockers are machine-classified
- dashboards and retry policies can branch on failure type

### 6. Actionable summary compression
Collapse noisy event streams into:
- current phase
- last successful checkpoint
- current blocker
- recommended next recovery action

Acceptance:
- channel status updates stay short and machine-grounded
- claws stop inferring state from raw build spam

## Phase 3 — Branch/Test Awareness and Auto-Recovery

### 7. Stale-branch detection before broad verification
Before broad test runs, compare current branch to `main` and detect if known fixes are missing.

Acceptance:
- emit `branch.stale_against_main`
- suggest or auto-run rebase/merge-forward according to policy
- avoid misclassifying stale-branch failures as new regressions

### 8. Recovery recipes for common failures
Encode known automatic recoveries for:
- trust prompt unresolved
- prompt delivered to shell
- stale branch
- compile red after cross-crate refactor
- MCP startup handshake failure
- partial plugin startup

Acceptance:
- one automatic recovery attempt occurs before escalation
- the attempted recovery is itself emitted as structured event data

### 9. Green-ness contract
Workers should distinguish:
- targeted tests green
- package green
- workspace green
- merge-ready green

Acceptance:
- no more ambiguous "tests passed" messaging
- merge policy can require the correct green level for the lane type
- a single hung test must not mask other failures: enforce per-test
  timeouts in CI (`cargo test --workspace`) so a 6-minute hang in one
  crate cannot prevent downstream crates from running their suites
- when a CI job fails because of a hang, the worker must report it as
  `test.hung` rather than a generic failure, so triage doesn't conflate
  it with a normal `assertion failed`
- recorded pinpoint (2026-04-08): `be561bf` swapped the local
  byte-estimate preflight for a `count_tokens` round-trip and silently
  returned `Ok(())` on any error, so `send_message_blocks_oversized_*`
  hung for ~6 minutes per attempt; the resulting workspace job crash
  hid 6 *separate* pre-existing CLI regressions (compact flag
  discarded, piped stdin vs permission prompter, legacy session layout,
  help/prompt assertions, mock harness count) that only became
  diagnosable after `8c6dfe5` + `5851f2d` restored the fast-fail path

## Phase 4 — Claws-First Task Execution

### 10. Typed task packet format
Define a structured task packet with fields like:
- objective
- scope
- repo/worktree
- branch policy
- acceptance tests
- commit policy
- reporting contract
- escalation policy

Acceptance:
- claws can dispatch work without relying on long natural-language prompt blobs alone
- task packets can be logged, retried, and transformed safely

### 11. Policy engine for autonomous coding
Encode automation rules such as:
- if green + scoped diff + review passed -> merge to dev
- if stale branch -> merge-forward before broad tests
- if startup blocked -> recover once, then escalate
- if lane completed -> emit closeout and cleanup session

Acceptance:
- doctrine moves from chat instructions into executable rules

### 12. Claw-native dashboards / lane board
Expose a machine-readable board of:
- repos
- active claws
- worktrees
- branch freshness
- red/green state
- current blocker
- merge readiness
- last meaningful event

Acceptance:
- claws can query status directly
- human-facing views become a rendering layer, not the source of truth

## Phase 5 — Plugin and MCP Lifecycle Maturity

### 13. First-class plugin/MCP lifecycle contract
Each plugin/MCP integration should expose:
- config validation contract
- startup healthcheck
- discovery result
- degraded-mode behavior
- shutdown/cleanup contract

Acceptance:
- partial-startup and per-server failures are reported structurally
- successful servers remain usable even when one server fails

### 14. MCP end-to-end lifecycle parity
Close gaps from:
- config load
- server registration
- spawn/connect
- initialize handshake
- tool/resource discovery
- invocation path
- error surfacing
- shutdown/cleanup

Acceptance:
- parity harness and runtime tests cover healthy and degraded startup cases
- broken servers are surfaced as structured failures, not opaque warnings

## Immediate Backlog (from current real pain)

Priority order: P0 = blocks CI/green state, P1 = blocks integration wiring, P2 = clawability hardening, P3 = swarm-efficiency improvements.

**P0 — Fix first (CI reliability)**
1. Isolate `render_diff_report` tests into tmpdir — **done**: `render_diff_report_for()` tests run in temp git repos instead of the live working tree, and targeted `cargo test -p rusty-claude-cli render_diff_report -- --nocapture` now stays green during branch/worktree activity
2. Expand GitHub CI from single-crate coverage to workspace-grade verification — **done**: `.github/workflows/rust-ci.yml` now runs `cargo test --workspace` plus fmt/clippy at the workspace level
3. Add release-grade binary workflow — **done**: `.github/workflows/release.yml` now builds tagged Rust release artifacts for the CLI
4. Add container-first test/run docs — **done**: `Containerfile` + `docs/container.md` document the canonical Docker/Podman workflow for build, bind-mount, and `cargo test --workspace` usage
5. Surface `doctor` / preflight diagnostics in onboarding docs and help — **done**: README + USAGE now put `claw doctor` / `/doctor` in the first-run path and point at the built-in preflight report
6. Automate branding/source-of-truth residue checks in CI — **done**: `.github/scripts/check_doc_source_of_truth.py` and the `doc-source-of-truth` CI job now block stale repo/org/invite residue in tracked docs and metadata
7. Eliminate warning spam from first-run help/build path — **done**: current `cargo run -q -p rusty-claude-cli -- --help` renders clean help output without a warning wall before the product surface
8. Promote `doctor` from slash-only to top-level CLI entrypoint — **done**: `claw doctor` is now a local shell entrypoint with regression coverage for direct help and health-report output
9. Make machine-readable status commands actually machine-readable — **done**: `claw --output-format json status` and `claw --output-format json sandbox` now emit structured JSON snapshots instead of prose tables
10. Unify legacy config/skill namespaces in user-facing output — **done**: skills/help JSON/text output now present `.claw` as the canonical namespace and collapse legacy roots behind `.claw`-shaped source ids/labels
11. Honor JSON output on inventory commands like `skills` and `mcp` — **done**: direct CLI inventory commands now honor `--output-format json` with structured payloads for both skills and MCP inventory
12. Audit `--output-format` contract across the whole CLI surface — **done**: direct CLI commands now honor deterministic JSON/text handling across help/version/status/sandbox/agents/mcp/skills/bootstrap-plan/system-prompt/init/doctor, with regression coverage in `output_format_contract.rs` and resumed `/status` JSON coverage

**P1 — Next (integration wiring, unblocks verification)**
2. Add cross-module integration tests — **done**: 12 integration tests covering worker→recovery→policy, stale_branch→policy, green_contract→policy, reconciliation flows
3. Wire lane-completion emitter — **done**: `lane_completion` module with `detect_lane_completion()` auto-sets `LaneContext::completed` from session-finished + tests-green + push-complete → policy closeout
4. Wire `SummaryCompressor` into the lane event pipeline — **done**: `compress_summary_text()` feeds into `LaneEvent::Finished` detail field in `tools/src/lib.rs`

**P2 — Clawability hardening (original backlog)**
5. Worker readiness handshake + trust resolution — **done**: `WorkerStatus` state machine with `Spawning` → `TrustRequired` → `ReadyForPrompt` → `PromptAccepted` → `Running` lifecycle, `trust_auto_resolve` + `trust_gate_cleared` gating
6. Prompt misdelivery detection and recovery — **done**: `prompt_delivery_attempts` counter, `PromptMisdelivery` event detection, `auto_recover_prompt_misdelivery` + `replay_prompt` recovery arm
7. Canonical lane event schema in clawhip — **done**: `LaneEvent` enum with `Started/Blocked/Failed/Finished` variants, `LaneEvent::new()` typed constructor, `tools/src/lib.rs` integration
8. Failure taxonomy + blocker normalization — **done**: `WorkerFailureKind` enum (`TrustGate/PromptDelivery/Protocol/Provider`), `FailureScenario::from_worker_failure_kind()` bridge to recovery recipes
9. Stale-branch detection before workspace tests — **done**: `stale_branch.rs` module with freshness detection, behind/ahead metrics, policy integration
10. MCP structured degraded-startup reporting — **done**: `McpManager` degraded-startup reporting (+183 lines in `mcp_stdio.rs`), failed server classification (startup/handshake/config/partial), structured `failed_servers` + `recovery_recommendations` in tool output
11. Structured task packet format — **done**: `task_packet.rs` module with `TaskPacket` struct, validation, serialization, `TaskScope` resolution (workspace/module/single-file/custom), integrated into `tools/src/lib.rs`
12. Lane board / machine-readable status API — **done**: Lane completion hardening + `LaneContext::completed` auto-detection + MCP degraded reporting surface machine-readable state
13. **Session completion failure classification** — **done**: `WorkerFailureKind::Provider` + `observe_completion()` + recovery recipe bridge landed
14. **Config merge validation gap** — **done**: `config.rs` hook validation before deep-merge (+56 lines), malformed entries fail with source-path context instead of merged parse errors
15. **MCP manager discovery flaky test** — **done**: `manager_discovery_report_keeps_healthy_servers_when_one_server_fails` now runs as a normal workspace test again after repeated stable passes, so degraded-startup coverage is no longer hidden behind `#[ignore]`

16. **Commit provenance / worktree-aware push events** — **done**: `LaneCommitProvenance` now carries branch/worktree/canonical-commit/supersession metadata in lane events, and `dedupe_superseded_commit_events()` is applied before agent manifests are written so superseded commit events collapse to the latest canonical lineage
17. **Orphaned module integration audit** — **done**: `runtime` now keeps `session_control` and `trust_resolver` behind `#[cfg(test)]` until they are wired into a real non-test execution path, so normal builds no longer advertise dead clawability surface area.
18. **Context-window preflight gap** — **done**: provider request sizing now emits `context_window_blocked` before oversized requests leave the process, using a model-context registry instead of the old naive max-token heuristic.
19. **Subcommand help falls through into runtime/API path** — **done**: `claw doctor --help`, `claw status --help`, `claw sandbox --help`, and nested `mcp`/`skills` help are now intercepted locally without runtime/provider startup, with regression tests covering the direct CLI paths.
20. **Session state classification gap (working vs blocked vs finished vs truly stale)** — **done**: agent manifests now derive machine states such as `working`, `blocked_background_job`, `blocked_merge_conflict`, `degraded_mcp`, `interrupted_transport`, `finished_pending_report`, and `finished_cleanable`, and terminal-state persistence records commit provenance plus derived state so downstream monitoring can distinguish quiet progress from truly idle sessions.
21. **Resumed `/status` JSON parity gap** — dogfooding shows fresh `claw status --output-format json` now emits structured JSON, but resumed slash-command status still leaks through a text-shaped path in at least one dispatch path. Local CI-equivalent repro fails `rust/crates/rusty-claude-cli/tests/resume_slash_commands.rs::resumed_status_command_emits_structured_json_when_requested` with `expected value at line 1 column 1`, so resumed automation can receive text where JSON was explicitly requested. **Action:** unify fresh vs resumed `/status` rendering through one output-format contract and add regression coverage so resumed JSON output is guaranteed valid.
22. **Opaque failure surface for session/runtime crashes** — **done**: `safe_failure_class()` in `error.rs` classifies all API errors into 8 user-safe classes (`provider_auth`, `provider_internal`, `provider_retry_exhausted`, `provider_rate_limit`, `provider_transport`, `provider_error`, `context_window`, `runtime_io`). `format_user_visible_api_error` in `main.rs` attaches session ID + request trace ID to every user-visible error. Coverage in `opaque_provider_wrapper_surfaces_failure_class_session_and_trace` and 3 related tests.
23. **`doctor --output-format json` check-level structure gap** — **done**: `claw doctor --output-format json` now keeps the human-readable `message`/`report` while also emitting structured per-check diagnostics (`name`, `status`, `summary`, `details`, plus typed fields like workspace paths and sandbox fallback data), with regression coverage in `output_format_contract.rs`.
24. **Plugin lifecycle init/shutdown test flakes under workspace-parallel execution** — dogfooding surfaced that `build_runtime_runs_plugin_lifecycle_init_and_shutdown` can fail under `cargo test --workspace` while passing in isolation because sibling tests race on tempdir-backed shell init script paths. This is test brittleness rather than a code-path regression, but it still destabilizes CI confidence and wastes diagnosis cycles. **Action:** isolate temp resources per test robustly (unique dirs + no shared cwd assumptions), audit cleanup timing, and add a regression guard so the plugin lifecycle test remains stable under parallel workspace execution.
26. **Resumed local-command JSON parity gap** — **done**: direct `claw --output-format json` already had structured renderers for `sandbox`, `mcp`, `skills`, `version`, and `init`, but resumed `claw --output-format json --resume <session> /…` paths still fell back to prose because resumed slash dispatch only emitted JSON for `/status`. Resumed `/sandbox`, `/mcp`, `/skills`, `/version`, and `/init` now reuse the same JSON envelopes as their direct CLI counterparts, with regression coverage in `rust/crates/rusty-claude-cli/tests/resume_slash_commands.rs` and `rust/crates/rusty-claude-cli/tests/output_format_contract.rs`.

41. **Phantom completions root cause: global session store has no per-worktree isolation** —

    **Root cause.** The session store under `~/.local/share/opencode` is global to the host. Every `opencode serve` instance — including the parallel lane workers spawned per worktree — reads and writes the same on-disk session directory. Sessions are keyed only by id and timestamp, not by the workspace they were created in, so there is no structural barrier between a session created in worktree `/tmp/b4-phantom-diag` and one created in `/tmp/b4-omc-flat`. Whichever serve instance picks up a given session id can drive it from whatever CWD that serve happens to be running in.

    **Impact.** Parallel lanes silently cross wires. A lane reports a clean run — file edits, builds, tests — and the orchestrator marks the lane green, but the writes were applied against another worktree's CWD because a sibling `opencode serve` won the session race. The originating worktree shows no diff, the *other* worktree gains unexplained edits, and downstream consumers (clawhip lane events, PR pushes, merge gates) treat the empty originator as a successful no-op. These are the "phantom completions" we keep chasing: success messaging without any landed changes in the lane that claimed them, plus stray edits in unrelated lanes whose own runs never touched those files. Because the report path is happy, retries and recovery recipes never fire, so the lane silently wedges until a human notices the diff is empty.

    **Proposed fix.** Bind every session to its workspace root + branch at creation time and refuse to drive it from any other CWD.

    - At session creation, capture the canonical workspace root (resolved git worktree path) and the active branch and persist them on the session record.
    - On every load (`opencode serve`, slash-command resume, lane recovery), validate that the current process CWD matches the persisted workspace root before any tool with side effects (file_ops, bash, git) is allowed to run. Mismatches surface as a typed `WorkspaceMismatch` failure class instead of silently writing to the wrong tree.
    - Namespace the on-disk session path under the workspace fingerprint (e.g. `<session_store>/<workspace_hash>/<session_id>`) so two parallel `opencode serve` instances physically cannot collide on the same session id.
    - Forks inherit the parent's workspace root by default; an explicit re-bind is required to move a session to a new worktree, and that re-bind is itself recorded as a structured event so the orchestrator can audit cross-worktree handoffs.
    - Surface a `branch.workspace_mismatch` lane event so clawhip stops counting wrong-CWD writes as lane completions.

    **Status.** A `workspace_root` field has been added to `Session` in `rust/crates/runtime/src/session.rs` (with builder, accessor, JSON + JSONL round-trip, fork inheritance, and given/when/then test coverage in `persists_workspace_root_round_trip_and_forks_inherit_it`). The CWD validation, the namespaced on-disk path, and the `branch.workspace_mismatch` lane event are still outstanding and tracked under this item.

**P3 — Swarm efficiency**
13. Swarm branch-lock protocol — **done**: `branch_lock::detect_branch_lock_collisions()` now detects same-branch/same-scope and nested-module collisions before parallel lanes drift into duplicate implementation
14. Commit provenance / worktree-aware push events — **done**: lane event provenance now includes branch/worktree/superseded/canonical lineage metadata, and manifest persistence de-dupes superseded commit events before downstream consumers render them

## Suggested Session Split

### Session A — worker boot protocol
Focus:
- trust prompt detection
- ready-for-prompt handshake
- prompt misdelivery detection

### Session B — clawhip lane events
Focus:
- canonical lane event schema
- failure taxonomy
- summary compression

### Session C — branch/test intelligence
Focus:
- stale-branch detection
- green-level contract
- recovery recipes

### Session D — MCP lifecycle hardening
Focus:
- startup/handshake reliability
- structured failed server reporting
- degraded-mode runtime behavior
- lifecycle tests/harness coverage

### Session E — typed task packets + policy engine
Focus:
- structured task format
- retry/merge/escalation rules
- autonomous lane closure behavior

## MVP Success Criteria

We should consider claw-code materially more clawable when:
- a claw can start a worker and know with certainty when it is ready
- claws no longer accidentally type tasks into the shell
- stale-branch failures are identified before they waste debugging time
- clawhip reports machine states, not just tmux prose
- MCP/plugin startup failures are classified and surfaced cleanly
- a coding lane can self-recover from common startup and branch issues without human babysitting

## Short Version

claw-code should evolve from:
- a CLI a human can also drive

to:
- a **claw-native execution runtime**
- an **event-native orchestration substrate**
- a **plugin/hook-first autonomous coding harness**

## Deployment Architecture Gap (filed from dogfood 2026-04-08)

### WorkerState is in the runtime; /state is NOT in opencode serve

**Root cause discovered during batch 8 dogfood.**

`worker_boot.rs` has a solid `WorkerStatus` state machine (`Spawning → TrustRequired → ReadyForPrompt → Running → Finished/Failed`). It is exported from `runtime/src/lib.rs` as a public API. But claw-code is a **plugin** loaded inside the `opencode` binary — it cannot add HTTP routes to `opencode serve`. The HTTP server is 100% owned by the upstream opencode process (v1.3.15).

**Impact:** There is no way to `curl localhost:4710/state` and get back a JSON `WorkerStatus`. Any such endpoint would require either:
1. Upstreaming a `/state` route into opencode's HTTP server (requires a PR to sst/opencode), or
2. Writing a sidecar HTTP process that queries the `WorkerRegistry` in-process (possible but fragile), or
3. Writing `WorkerStatus` to a well-known file path (`.claw/worker-state.json`) that an external observer can poll.

**Recommended path:** Option 3 — emit `WorkerStatus` transitions to `.claw/worker-state.json` on every state change. This is purely within claw-code's plugin scope, requires no upstream changes, and gives clawhip a file it can poll to distinguish a truly stalled worker from a quiet-but-progressing one.

**Action item:** Wire `WorkerRegistry::transition()` to atomically write `.claw/worker-state.json` on every state transition. Add a `claw state` CLI subcommand that reads and prints this file. Add regression test.

**Prior session note:** A previous session summary claimed commit `0984cca` landed a `/state` HTTP endpoint via axum. This was incorrect — no such commit exists on main, axum is not a dependency, and the HTTP server is not ours. The actual work that exists: `worker_boot.rs` with `WorkerStatus` enum + `WorkerRegistry`, fully wired into `runtime/src/lib.rs` as public exports.

## Startup Friction Gap: No Default trusted_roots in Settings (filed 2026-04-08)

### Every lane starts with manual trust babysitting unless caller explicitly passes roots

**Root cause discovered during direct dogfood of WorkerCreate tool.**

`WorkerCreate` accepts a `trusted_roots: Vec<String>` parameter. If the caller omits it (or passes `[]`), every new worker immediately enters `TrustRequired` and stalls — requiring manual intervention to advance to `ReadyForPrompt`. There is no mechanism to configure a default allowlist in `settings.json` or `.claw/settings.json`.

**Impact:** Batch tooling (clawhip, lane orchestrators) must pass `trusted_roots` explicitly on every `WorkerCreate` call. If a batch script forgets the field, all workers in that batch stall silently at `trust_required`. This was the root cause of several "batch 8 lanes not advancing" incidents.

**Recommended fix:**
1. Add a `trusted_roots` field to `RuntimeConfig` (or a nested `[trust]` table), loaded via `ConfigLoader`.
2. In `WorkerRegistry::spawn_worker()`, merge config-level `trusted_roots` with any per-call overrides.
3. Default: empty list (safest). Users opt in by adding their repo paths to settings.
4. Update `config_validate` schema with the new field.

**Action item:** Wire `RuntimeConfig::trusted_roots()` → `WorkerRegistry::spawn_worker()` default. Cover with test: config with `trusted_roots = ["/tmp"]` → spawning worker in `/tmp/x` auto-resolves trust without caller passing the field.

## Observability Transport Decision (filed 2026-04-08)

### Canonical state surface: CLI/file-based. HTTP endpoint deferred.

**Decision:** `claw state` reading `.claw/worker-state.json` is the **blessed observability contract** for clawhip and downstream tooling. This is not a stepping-stone — it is the supported surface. Build against it.

**Rationale:**
- claw-code is a plugin running inside the opencode binary. It cannot add HTTP routes to `opencode serve` — that server belongs to upstream sst/opencode.
- The file-based surface is fully within plugin scope: `emit_state_file()` in `worker_boot.rs` writes atomically on every `WorkerStatus` transition.
- `claw state --output-format json` gives clawhip everything it needs: `status`, `is_ready`, `seconds_since_update`, `trust_gate_cleared`, `last_event`, `updated_at`.
- Polling a local file has lower latency and fewer failure modes than an HTTP round-trip to a sidecar.
- An HTTP state endpoint would require either (a) upstreaming a route to sst/opencode — a multi-week PR cycle with no guarantee of acceptance — or (b) a sidecar process that queries `WorkerRegistry` in-process, which is fragile and adds an extra failure domain.

**What downstream tooling (clawhip) should do:**
1. After `WorkerCreate`, poll `.claw/worker-state.json` (or run `claw state --output-format json`) in the worker's CWD at whatever interval makes sense (e.g. 5s).
2. Trust `seconds_since_update > 60` in `trust_required` status as the stall signal.
3. Call `WorkerResolveTrust` tool to unblock, or `WorkerRestart` to reset.

**HTTP endpoint tracking:** Not scheduled. If a concrete use case emerges that file polling cannot serve (e.g. remote workers over a network boundary), open a new issue to upstream a `/worker/state` route to sst/opencode at that time. Until then: file/CLI is canonical.

## Provider Routing: Model-Name Prefix Must Win Over Env-Var Presence (fixed 2026-04-08, `0530c50`)

### `openai/gpt-4.1-mini` was silently misrouted to Anthropic when ANTHROPIC_API_KEY was set

**Root cause:** `metadata_for_model` returned `None` for any model not matching `claude` or `grok` prefix.
`detect_provider_kind` then fell through to auth-sniffer order: first `has_auth_from_env_or_saved()` (Anthropic), then `OPENAI_API_KEY`, then `XAI_API_KEY`.

If `ANTHROPIC_API_KEY` was present in the environment (e.g. user has both Anthropic and OpenRouter configured), any unknown model — including explicitly namespaced ones like `openai/gpt-4.1-mini` — was silently routed to the Anthropic client, which then failed with `missing Anthropic credentials` or a confusing 402/auth error rather than routing to OpenAI-compatible.

**Fix:** Added explicit prefix checks in `metadata_for_model`:
- `openai/` prefix → `ProviderKind::OpenAi`
- `gpt-` prefix → `ProviderKind::OpenAi`

Model name prefix now wins unconditionally over env-var presence. Regression test locked in: `providers::tests::openai_namespaced_model_routes_to_openai_not_anthropic`.

**Lesson:** Auth-sniffer fallback order is fragile. Any new provider added in the future should be registered in `metadata_for_model` via a model-name prefix, not left to env-var order. This is the canonical extension point.
