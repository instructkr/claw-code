# Merge Checklist — claw-code

**Purpose:** Streamline merging of the 17 review-ready branches by grouping them into safe clusters and providing per-cluster merge order + validation steps.

**Generated:** Cycle #70 (2026-04-23 03:55 Seoul)

---

## Merge Strategy

**Recommended order:** P0 → P1 → P2 → P3 (by priority tier from REVIEW_DASHBOARD.md).

**Batch strategy:** Merge by cluster, not individual branches. Each cluster shares the same fix pattern, so reviewers can validate one cluster and merge all members together.

**Estimated throughput:** 2-3 clusters per merge session. At current cycle velocity (~1 cluster per 15 min), full queue → merged main in ~2 hours.

---

## Cluster Merge Order

### Cluster 1: Typed-Error Threading (P0) — 3 branches

**Members:**
- `feat/jobdori-249-resumed-slash-kind` (commit `eb4b1eb`, 61 lines)
- `feat/jobdori-248-unknown-verb-option-classify` (commit `6c09172`)
- `feat/jobdori-251-session-dispatch` (commit `dc274a0`)

**Merge prerequisites:**
- [ ] All three branches built and tested locally (181 tests pass)
- [ ] All three have only changes in `rust/crates/rusty-claude-cli/src/main.rs` (no cross-crate impact)
- [ ] No merge conflicts between them (all edit non-overlapping regions)

**Merge order (within cluster):** 
1. #249 (smallest, lowest risk)
2. #248 (medium)
3. #251 (largest, but depends on #249/#248 patterns)

**Post-merge validation:**
- Rebuild binary: `cargo build -p rusty-claude-cli`
- Run: `./target/debug/claw version` (should work)
- Run: `cargo test -p rusty-claude-cli` (should pass 181 tests)

**Commit strategy:** Rebase all three, squash into single "typed-error: thread kind+hint through 3 families" commit, OR merge individually preserving commit history for bisect clarity.

---

### Cluster 2: Diagnostic-Strictness (P1) — 3 branches

**Members:**
- `feat/jobdori-122-doctor-stale-base` (commit `5bb9eba`)
- `feat/jobdori-122b-doctor-broad-cwd` (commit `0aa0d3f`)
- `fix/jobdori-161-worktree-git-sha` (commit `c5b6fa5`)

**Merge prerequisites:**
- [ ] #122 and #122b are binary-level changes, #161 is build-system change
- [ ] All three pass `cargo build`
- [ ] No cross-crate merge conflicts

**Why these three together:** All share the diagnostic-strictness principle. #122 and #122b extend `doctor`, #161 fixes `version`. Merging as a cluster signals the principle to future reviewers.

**Post-merge validation:**
- Rebuild binary
- Run: `claw doctor` (should now check stale-base + broad-cwd)
- Run: `claw version` (should report correct SHA even in worktrees)
- Run: `cargo test` (full suite)

**Commit strategy:** Merge individually preserving history, then add ROADMAP commit explaining the cluster principle. This makes the doctrine visible in git log.

---

### Cluster 3: Help-Parity (P1) — 4 branches

**Members:**
- `feat/jobdori-130b-filesystem-context` (commit `d49a75c`)
- `feat/jobdori-130c-diff-help` (commit `83f744a`)
- `feat/jobdori-130d-config-help` (commit `19638a0`)
- `feat/jobdori-130e-dispatch-help` + `feat/jobdori-130e-surface-help` (commits `0ca0344`, `9dd7e79`)

**Merge prerequisites:**
- [ ] All four branches edit help-topic routing in the same regions
- [ ] Verify no merge conflicts (should be sequential, non-overlapping edits)
- [ ] `cargo build` passes

**Why these four together:** All address help-parity (verbs in `--help` → correct help topics). This cluster is the most "batch-like" — identical fix pattern repeated.

**Post-merge validation:**
- Rebuild binary
- Run: `claw diff --help` (should route to help topic, not crash)
- Run: `claw config --help` (ditto)
- Run: `claw --help` (should list all verbs)

**Merge strategy:** Can be fast-forwarded or squashed as a unit since they're all the same pattern.

---

### Cluster 4: Suffix-Guard (P2) — 2 branches

**Members:**
- `feat/jobdori-152-init-suffix-guard` (commit `860f285`)
- `feat/jobdori-152-bootstrap-plan-suffix-guard` (commit `3a533ce`)

**Merge prerequisites:**
- [ ] Both branches add `rest.len() > 1` check to no-arg verbs
- [ ] No conflicts

**Post-merge validation:**
- `claw init extra-arg` (should reject)
- `claw bootstrap-plan extra-arg` (should reject)

**Merge strategy:** Merge together.

---

### Cluster 5: Verb-Classification (P2) — 1 branch

**Member:**
- `feat/jobdori-160-verb-classification` (commit `5538934`)

**Merge prerequisites:**
- [ ] Binary tested (23-line change to parser)
- [ ] `cargo test` passes 181 tests

**Post-merge validation:**
- `claw resume bogus-id` (should emit slash-command guidance, not missing_credentials)
- `claw explain this` (should still route to Prompt)

**Note:** Can merge solo or batch with #4. No dependencies.

---

### Cluster 6: Doc-Truthfulness (P3) — 2 branches

**Members:**
- `docs/parity-update-2026-04-23` (commit `92a79b5`)
- `docs/jobdori-162-usage-verb-parity` (commit `48da190`)

**Merge prerequisites:**
- [ ] Both are doc-only (no code risk)
- [ ] USAGE.md sections match verbs in `--help`
- [ ] PARITY.md stats are current

**Post-merge validation:**
- `claw --help` (all verbs listed)
- `grep "dump-manifests\|bootstrap-plan" USAGE.md` (should find sections)
- Read PARITY.md (should cite current date + stats)

**Merge strategy:** Can merge in any order.

---

## Merge Conflict Risk Assessment

**High-risk clusters (potential conflicts):**
- Cluster 1 (Typed-error) — all edit `main.rs` dispatch/error arms, but in different methods (likely non-overlapping)
- Cluster 3 (Help-parity) — all edit help-routing, but different verbs (should sequence cleanly)

**Low-risk clusters (isolated changes):**
- Cluster 2 (Diagnostic-strictness) — #122 and #122b both edit `check_workspace_health()`, could conflict. #161 edits `build.rs` (no overlap).
- Cluster 4 (Suffix-guard) — two independent verbs, no conflict
- Cluster 5 (Verb-classification) — solo, no conflict
- Cluster 6 (Doc-truthfulness) — doc-only, no conflict

**Conflict mitigation:** Merge Cluster 2 sub-groups: (#122 → #122b → #161) to avoid simultaneous edits to `check_workspace_health()`.

---

## Post-Merge Validation Checklist

**After all clusters are merged to main:**

- [ ] `cargo build --all` (full workspace build)
- [ ] `cargo test -p rusty-claude-cli` (181 tests pass)
- [ ] `cargo fmt --all --check` (no formatting regressions)
- [ ] `./target/debug/claw version` (correct SHA, not stale)
- [ ] `./target/debug/claw doctor` (stale-base + broad-cwd warnings work)
- [ ] `./target/debug/claw --help` (all verbs listed)
- [ ] `grep -c "### \`" USAGE.md` (all 12 verbs documented, not 8)
- [ ] Fresh dogfood run: `./target/debug/claw prompt "test"` (works)

---

## Timeline Estimate

| Phase | Time | Action |
|---|---|---|
| Merge Cluster 1 (P0 typed-error) | ~15 min | Merge 3 branches, test, validate |
| Merge Cluster 2 (P1 diagnostic-strictness) | ~15 min | Merge 3 branches (mind #122/#122b conflict) |
| Merge Cluster 3 (P1 help-parity) | ~20 min | Merge 4 branches (batch-friendly) |
| Merge Cluster 4–6 (P2–P3, low-risk) | ~10 min | Fast merges |
| **Total** | **~60 min** | **All 17 branches → main** |

---

## Notes for Reviewer

**Branch-last protocol validation:** All 17 branches here represent work that was:
1. Pinpoint filed (with repro + fix shape)
2. Implemented in scratch/worktree (not directly on main)
3. Verified to build + pass tests
4. Only then branched for review

This artifact provides the final step: **validated merge order + per-cluster risks.**

**Integration-support artifact:** This checklist reduces reviewer cognitive load by pre-answering "which merge order is safest?" and "what could go wrong?" questions.

---

**Checklist source:** Cycle #70 (2026-04-23 03:55 Seoul)
