# Project Risk Review Report

## Review Approach and Priority Scope

### Review approach
- First, understand the overall project structure, identify entrypoints, core service layers, and remote/plugin/tool execution paths, and determine system boundaries and privileged paths.
- Then drill into high-risk areas: authentication and authorization, configuration and environment variables, remote sessions, MCP/plugin extensibility, file handling, external APIs, logging and telemetry, and error handling.
- Finally assess engineering governance: test coverage, dependency and release materials, module coupling, observability, resilience controls, and future evolution risk.

### Key files and modules inspected
- Entry and orchestration: `src/main.tsx`
- Auth and session: `src/utils/auth.ts`, `src/utils/sessionIngressAuth.ts`, `src/services/oauth/*`
- Permission control: `src/utils/permissions/permissionSetup.ts`, `src/remote/remotePermissionBridge.ts`
- API and external calls: `src/services/api/client.ts`, `src/services/api/claude.ts`, `src/services/api/filesApi.ts`, `src/services/api/sessionIngress.ts`
- Remote connectivity: `src/remote/RemoteSessionManager.ts`, `src/remote/SessionsWebSocket.ts`, `src/server/createDirectConnectSession.ts`
- MCP / plugins: `src/services/mcp/client.ts`, `src/services/mcp/auth.ts`, `src/services/mcp/config.ts`, `src/services/mcp/channelPermissions.ts`, `src/services/mcp/headersHelper.ts`, `src/utils/plugins/pluginLoader.ts`
- Config and environment: `src/utils/config.ts`, `src/setup.ts`
- Logging and telemetry: `src/services/internalLogging.ts`, `src/utils/log.ts`, `src/services/analytics/*`
- History and persistence: `src/history.ts`
- Test and engineering artifacts: no standard test directories, dependency manifest files, Dockerfile, or CI workflows were present in the delivered snapshot

---

## 1. Executive Summary

- This review was performed against the delivered `src/` snapshot only. The repository root contains only `src/` and `src.zip`; `package.json`, lockfiles, Dockerfiles, and CI workflow files were not present. As a result, some dependency, deployment, and release conclusions can only be marked as potential risks.
- The most material issues are not classic SQL injection findings, but privileged capability boundary issues: remote sessions can explicitly skip permission prompts, MCP `headersHelper` can execute shell commands from repository-controlled config in non-interactive mode, and channel-based permission relay explicitly accepts the possibility of forged approvals from compromised services.
- From a stability and maintainability perspective, the project shows strong signs of core-module concentration: `src/main.tsx` is 4684 lines, `src/services/api/claude.ts` is 3420 lines, `src/services/mcp/client.ts` is 3349 lines, `src/utils/auth.ts` is 2003 lines, `src/utils/config.ts` is 1818 lines, and `src/utils/plugins/pluginLoader.ts` is 3303 lines. This increases architecture complexity and regression risk.
- From a quality perspective, no conventional test files were visible in the delivered snapshot. There are also issues with swallowed error semantics, telemetry collecting permission context, and file APIs with elevated memory ceilings, all of which reduce confidence for production readiness.
- Positive note: some governance controls do exist. For example, `zod` validation is used in `src/services/api/bootstrap.ts:19` and `src/server/createDirectConnectSession.ts:71`, and team-memory sync includes secret scanning in `src/services/teamMemorySync/secretScanner.ts`. However, these controls do not yet cover the most critical privileged boundaries.

## 2. Overall Risk Rating

- Rating: High
- Basis:
  - High-risk capabilities can bypass or materially weaken permission boundaries.
  - Repository-configured shell execution is possible in non-interactive scenarios.
  - A forged-approval trust model is explicitly accepted for channel-based permissions.
  - Security-critical logic is concentrated in very large files with no visible tests.
  - The delivered snapshot lacks dependency and deployment metadata required for a full supply-chain audit.

## 3. Key High-Risk Findings

- Direct-connect remote sessions support `dangerously_skip_permissions`, and the client passes this directly to the server, exposing a privileged bypass capability. Location: `src/server/createDirectConnectSession.ts:52`
- MCP `headersHelper` skips workspace trust validation in non-interactive mode and executes configured content with `shell: true`, creating a repository-configured command execution risk. Location: `src/services/mcp/headersHelper.ts:41`, `src/services/mcp/headersHelper.ts:61`
- The channel permission relay model explicitly accepts that a compromised allowlisted channel server can fabricate `yes <id>` approvals. This is a trust-boundary design issue, not merely an implementation bug. Location: `src/services/mcp/channelPermissions.ts:15`

## 4. Complete Risk Details

### Finding 1: Direct-connect sessions can bypass all permission prompts
- Severity: High
- Category: Authentication and Authorization
- Location: `src/server/createDirectConnectSession.ts:26`
- Evidence:
  - The request body directly includes `dangerously_skip_permissions: true`. `src/server/createDirectConnectSession.ts:52`
  - There is no visible additional server capability confirmation or secondary constraint on the client side; behavior is controlled by input parameters alone. `src/server/createDirectConnectSession.ts:30`
- Impact:
  - If misused, scripted, or exposed through weak control-plane protections, a remote session can lose its last human approval gate.
  - For production readiness, this is a privileged capability exposure issue, not a minor implementation detail.
- Remediation:
  - Move this to strict server-side enforcement with an explicit allowlist and limit it to controlled environments and principals.
  - Add audit logging, stronger confirmation, or short-lived capability tokens.
  - Separate debugging/experimental capabilities from production capabilities at the protocol level.

### Finding 2: `headersHelper` can execute shell from repository-controlled configuration in non-interactive mode
- Severity: High
- Category: Command Execution / Supply Chain / Workspace Trust
- Location: `src/services/mcp/headersHelper.ts:32`
- Evidence:
  - The code explicitly states that trust checks are skipped in non-interactive mode. `src/services/mcp/headersHelper.ts:41`
  - Execution uses `shell: true`. `src/services/mcp/headersHelper.ts:61`
  - The input source is config-driven via MCP `headersHelper`. `src/services/mcp/headersHelper.ts:36`
- Impact:
  - In CI/CD, automation, batch jobs, or service accounts, repository-local `.mcp.json` or project config can trigger command execution.
  - This is a classic engineering-chain risk: the attack surface is not external user input, but repository or supply-chain content.
- Remediation:
  - Disable project/local-scope `headersHelper` by default in non-interactive mode.
  - Remove `shell: true` and require explicit executable plus argv.
  - Apply allowlisting, signature verification, or fixed-directory validation for helper paths; require explicit `--allow-headers-helper` in CI.

### Finding 3: Channel permission relay accepts forged approvals from compromised services
- Severity: High
- Category: Permission Boundary / Trust Boundary
- Location: `src/services/mcp/channelPermissions.ts:15`
- Evidence:
  - The comments explicitly state that a compromised channel server “CAN fabricate yes <id>” and classify it as an accepted risk. `src/services/mcp/channelPermissions.ts:17`
  - Client-side resolution only matches `requestId`; it does not validate end-user proof. `src/services/mcp/channelPermissions.ts:228`
- Impact:
  - Any compromised or abused allowlisted channel service can bypass genuine user approval and participate in privileged action chains.
  - This is a systemic trust-model weakness rather than a localized bug.
- Remediation:
  - Upgrade approval from server assertion to user-signed confirmation or a one-time challenge-response.
  - Downgrade channel approval to auxiliary signaling; final approval should still occur in a trusted local or managed UI.
  - At minimum, disable channel approval for high-risk tool categories.

### Finding 4: Internal telemetry records full permission context and container identity
- Severity: Medium
- Category: Logging and Privacy
- Location: `src/services/internalLogging.ts:71`
- Evidence:
  - The event includes a fully serialized `toolPermissionContext`. `src/services/internalLogging.ts:84`
  - It also records namespace and container ID. `src/services/internalLogging.ts:82`, `src/services/internalLogging.ts:87`
- Impact:
  - Permission rules, workload location, and container identifiers together can create a highly sensitive internal operational profile.
  - If telemetry fields expand without governance, strategy details or environment information may also leak.
- Remediation:
  - Minimize collection to mode and rule counts instead of full JSON.
  - Hash or classify `containerId` and `namespace` before export.
  - Establish a telemetry field allowlist review process.

### Finding 5: Session log fetching swallows 401 semantics and degrades auth failures to `null`
- Severity: Medium
- Category: Error Handling / Reliability
- Location: `src/services/api/sessionIngress.ts:420`
- Evidence:
  - On 401, the code first throws “Please run /login to sign in again.” `src/services/api/sessionIngress.ts:462`
  - But the outer `catch` collapses the outcome to `null`, so callers cannot distinguish auth expiry from ordinary retrieval failure. `src/services/api/sessionIngress.ts:477`
  - In the same file, `getTeleportEvents()` throws directly on 401, creating inconsistent semantics. `src/services/api/sessionIngress.ts:355`
- Impact:
  - Callers may misinterpret the issue as “no logs” or “temporary failure,” reducing diagnostic clarity and misleading users.
  - Over time, this can produce hidden consistency issues and support confusion.
- Remediation:
  - Introduce structured error types such as `auth_expired`, `not_found`, and `network_error`.
  - Stop collapsing all outer-layer failures to `null`.
  - Standardize the error contract between session-ingress and teleport flows.

### Finding 6: Files API uses whole-buffer memory handling for large files
- Severity: Medium
- Category: Performance and Stability
- Location: `src/services/api/filesApi.ts:132`
- Evidence:
  - Downloads use `responseType: 'arraybuffer'` and then convert the full payload to a `Buffer`. `src/services/api/filesApi.ts:149`, `src/services/api/filesApi.ts:158`
  - Default download concurrency is 5. `src/services/api/filesApi.ts:269`, `src/services/api/filesApi.ts:320`
  - Uploads read the full file into memory before size validation; the single-file ceiling is 500MB. `src/services/api/filesApi.ts:396`, `src/services/api/filesApi.ts:413`
- Impact:
  - Multi-file operations can cause high memory spikes, especially in remote or low-memory environments, and may trigger OOM or severe process degradation.
  - This is especially risky for large files and containerized runtimes.
- Remediation:
  - Change downloads to streaming writes and uploads to streaming multipart.
  - Add Content-Length prechecks and hard size caps on download paths as well.
  - Dynamically reduce concurrency based on available memory instead of using a fixed value.

### Finding 7: Weak validation on critical message inputs
- Severity: Medium (Potential Risk)
- Category: Input and Output Validation
- Location: `src/remote/SessionsWebSocket.ts:46`, `src/services/api/filesApi.ts:722`
- Evidence:
  - WebSocket inbound messages are accepted as long as they are objects with a string `type` field. `src/remote/SessionsWebSocket.ts:46`
  - The code explicitly avoids an allowlist. `src/remote/SessionsWebSocket.ts:50`
  - File spec parsing only splits on the first `:` and whitespace, without stronger schema enforcement. `src/services/api/filesApi.ts:726`
- Impact:
  - Under backend drift, proxy corruption, or polluted upstream input, the client is more likely to mis-handle messages or behave unpredictably.
  - This is currently more of a robustness issue, but it can become a security issue when coupled with privileged flows.
- Remediation:
  - Introduce layered schema validation for control and session messages.
  - Replace string-concatenated file specs with explicit JSON or structured parameters.
  - Isolate and record unknown message types rather than allowing them into the main flow by default.

### Finding 8: No visible automated tests in the delivered snapshot
- Severity: Medium (Potential Risk)
- Category: Test Governance
- Location: whole `src/`
- Evidence:
  - No `src/**/*.{test,spec}.{ts,tsx,js,jsx}` files were found.
  - No `src/**/__tests__/**/*` directories were found.
  - The project contains many security-critical modules and complex permission state machines.
- Impact:
  - Production review cannot confirm regression coverage for permission modes, remote sessions, MCP, token refresh, or error recovery.
  - Security fixes are more likely to introduce new bypasses or regressions.
- Remediation:
  - Add at least unit and integration coverage for auth, permissions, remote sessions, MCP, files API, and error contracts.
  - Include dangerous modes, non-interactive mode, and 401/409/404 paths in mandatory regression suites.
  - If tests exist outside the delivered snapshot, provide the full repository before relying on this review for production sign-off.

### Finding 9: Configuration and environment governance is overly complex
- Severity: Medium
- Category: Architecture and Maintainability
- Location: `src/utils/config.ts:183`, `src/utils/auth.ts:120`, `src/main.tsx`
- Evidence:
  - `src/utils/config.ts` retains many deprecated or legacy fields such as `apiKeyHelper`, `env`, `cachedChangelog`, and legacy MCP fields. `src/utils/config.ts:185`, `src/utils/config.ts:237`
  - There are over 1514 `process.env` references across the codebase.
  - Core files are very large, including `src/main.tsx` at 4684 lines and `src/services/mcp/client.ts` at 3349 lines.
- Impact:
  - It is difficult to maintain clear dev/test/prod behavior boundaries, and the cognitive load for changes is high.
  - Future enhancements are likely to create configuration conflicts and broad regression surfaces.
- Remediation:
  - Introduce a unified config schema and layered environment model, and reduce direct `process.env` reads.
  - Split oversized files by concern: auth, transport, policy, persistence, telemetry.
  - Create a deprecation retirement plan and freeze new legacy-style config entry points.

### Finding 10: Missing dependency and deployment artifacts block full supply-chain review
- Severity: Medium (Potential Risk)
- Category: Dependency Governance / Release Governance
- Location: repository root
- Evidence:
  - The root contains only `src/` and `src.zip`.
  - No `package.json`, lockfile, Dockerfile, or workflow files were present.
- Impact:
  - Dependency versions, licenses, known vulnerabilities, build commands, release paths, environment isolation, and image baselines cannot be verified.
  - For production review, this means the supply-chain risk posture remains incomplete.
- Remediation:
  - Provide the full repository, dependency manifests, lockfiles, CI/CD configuration, and deployment IaC.
  - Add SCA, license scanning, SBOM generation, and reproducible build validation.
  - Include deployment configuration and runtime configuration in the formal review baseline.

## 5. Systemic Governance Recommendations

- Tier privileged capabilities: classify `bypassPermissions`, channel approvals, dynamic header helpers, and remote control as P0 capabilities requiring security design review.
- Consolidate configuration entry points: manage env, settings, project config, and managed config through a single schema and stop expanding legacy fields.
- Harden non-interactive mode: disable repository-configured execution by default unless explicitly allowlisted.
- Normalize error contracts: do not mix `null` and `false` to represent auth failures, business failures, and network failures.
- Build a regression gate: define mandatory test coverage and smoke checks for auth, permissions, remote flows, MCP, files, and telemetry.
- Complete release artifacts: dependency manifests, lockfiles, CI, deployment IaC, and runbooks should all be part of formal production review.

## 6. Phased Remediation Roadmap

### Phase 1 (1-2 weeks)
- Disable or strictly constrain non-interactive `headersHelper`
- Add server-side enforcement and audit controls for `dangerously_skip_permissions`
- Fix swallowed 401 semantics in session log fetching
- Lower default Files API concurrency and add download size protections

### Phase 2 (2-4 weeks)
- Add tests for remote sessions, permission modes, MCP auth, and file transfer flows
- Add schema validation for WebSocket and control messages
- Minimize and redact internal telemetry fields

### Phase 3 (1-2 releases)
- Split `main.tsx`, `mcp/client.ts`, `auth.ts`, and `config.ts`
- Retire deprecated config fields and establish a unified config layer
- Add dependency governance, SBOM, SCA, and release/deployment audit coverage

### Phase 4 (ongoing)
- Establish threat modeling, change review, and regression gates for privileged capabilities
- Create pre-release security and stability checklists
- Add alerting, error budgets, and operational postmortem loops

## 7. Management Summary

- The central management concern is not whether the code “works,” but whether privileged capability boundaries are adequately controlled. At present, they are not fully controlled.
- If a production launch is imminent, non-interactive `headersHelper`, remote permission bypass, and the channel forged-approval trust model should be treated as launch blockers.
- From a delivery-governance perspective, the current snapshot is missing dependency and deployment materials, so supply-chain and release risks have not been fully reviewed.
- Over the medium term, the project shows typical complex-system warning signs: oversized core modules, diffuse configuration, coexistence of legacy and new patterns, and no visible tests. If left untreated, maintenance cost and change-related incident probability will continue to rise.
- The recommended management sequence is: reduce privileged exposure first, add test coverage second, and then refactor core architecture, rather than continuing to layer new functionality on top.

## Additional Notes

- No confirmed production secrets, tokens, or passwords were found hardcoded in the current `src/` snapshot; however, because the repository is incomplete, this should be interpreted as “not found in reviewed scope,” not “proven absent.”
- No direct SQL or ORM query code was identified in the reviewed scope, so database injection risk was not a primary finding in this snapshot.
- Monitoring and telemetry capabilities do exist, for example in `src/services/analytics/datadog.ts`; however, no alert rules, dashboard IaC, or SLO/SLA materials were present, so operational observability cannot be considered fully governed.
