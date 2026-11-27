---
name: multi-agent-codex-orchestration
description: Use when coordinating requested Astra, Sol and Luna lanes from specification through independent review and verified handoff, including bounded Diffuser GPU or Columbus RunPod jobs.
---

# Orchestrate the main Codex lane and Diffuser

| Lane | Model and effort | Handoff |
| --- | --- | --- |
| Astra | `gpt-6.2-astra`, xhigh | Product intent, UI and creative design, accepted contract, acceptance cases, final integration |
| Sol | `gpt-6.2-sol`, xhigh | Systems implementation, candidate SHA, changed paths, regressions and rollback point |
| Luna | `gpt-6-luna`, high | Independent security review at the integrated SHA, coverage ledger, findings and gaps |
| Diffuser MCP | Scoped capability and budget | Durable job ID, assigned device, result and artifact digests, capacity release |

Track task ID, owners, base and candidate SHAs, paths, dependencies, acceptance cases, budget, job IDs and evidence privately. Assign one deliverable and check per lane.

## Sequence by dependency

1. Freeze the base. Run Astra's UI and Sol's systems specifications in parallel read-only sandboxes. Astra settles schemas, ownership, failures and acceptance cases before implementation.
2. Use a branch and worktree per writer, for example `git worktree add -b "$LANE_BRANCH" "$LANE_TREE" "$BASE_SHA"`. Assign disjoint write paths, individual build targets and one owner per shared contract. The integrator merges candidates and runs acceptance on the merged revision.
3. For a Luna Cloudflare audit, launch `codex exec -m gpt-6-luna -c 'model_reasoning_effort="high"'` with a read-only source checkout and a disposable offline copy for tests. Check the actual startup header for `reasoning effort: high`. Put `NO STOPS` and 100% active-unit coverage in the brief. Map changed units, callers, entry points and trust boundaries. Reconcile findings against independent source or tests and validate both `findings.json` and `coverage-ledger.json` with the method's validators. Outstanding units or unresolved findings stay open. Sol fixes confirmed defects, then Luna rechecks the new SHA. [Audit method](https://github.com/cloudflare/security-audit-skill)
4. Apply evidence-gate to handoff claims. Record exact checks, control and candidate results, and gaps. For skill edits run `python3 -B /srv/streamline/tools/skill-lint.py --strict "$skill_dir"` on each final folder. A missing checker or nonzero exit leaves lint open.
5. For an authorized rollout, record the prior SHA and rollback path. Verify the served build, UI and scoped MCP discovery and run a bounded idempotent smoke job through the deployed entry point. Match output, artifact and release receipt. Halt promotion and use the agreed rollback on failure. For staging, report staged paths and local evidence.

## Launch one lane without waiting on stdin

Use a host-key-pinned SSH alias with noninteractive auth, GNU `timeout` at both ends and `codex` on the remote PATH. Prepare the private worktree and secret-free contract at the paths below. Give each attempt a fresh directory.

```bash
timeout -k 15s 47m ssh -n -T -o BatchMode=yes -o StrictHostKeyChecking=yes \
  -o ConnectTimeout=10 -o ServerAliveInterval=15 -o ServerAliveCountMax=3 \
  "$CODEX_LANE" "$(cat <<'REMOTE'
set -eu
umask 077
root="$HOME/lane-runs/change-42"
mkdir "$root/sol-01"
cd "$root/sol-01"
cat > prompt.txt <<'PROMPT'
Implement the accepted backend contract below in the assigned worktree.
Respect its path list. Run its acceptance checks with CARGO_BUILD_JOBS=2.
Return changed paths, test outcomes and any unverified case.
PROMPT
cat "$root/accepted-spec.md" >> prompt.txt
export CARGO_BUILD_JOBS=2
export CARGO_TARGET_DIR="$root/sol-tree/target"
rc=0
timeout -k 15s 45m codex exec --color never --sandbox workspace-write \
  -C "$root/sol-tree" -m gpt-6.2-sol -c 'model_reasoning_effort="xhigh"' \
  -o "$root/sol-01/final.md" - < prompt.txt > lane.log 2>&1 || rc=$?
printf '%s\n' "$rc" > exit.status
awk '
  /^--------$/ { if (++edges == 2) exit; next }
  edges == 1 && $0 == "model: gpt-6.2-sol" { model = 1 }
  edges == 1 && $0 == "reasoning effort: xhigh" { effort = 1 }
  END { exit !(edges == 2 && model && effort) }
' lane.log || { echo "Unexpected Codex header" >&2; exit 65; }
exit "$rc"
REMOTE
)"
```

SSH closes stdin. Codex reads the prompt to EOF on the remote host. Match the actual startup header. For Astra or Luna change the model, effort, header check, sandbox and paths together.

For fan-out save each background PID (`$!`) and `wait` separately. Inspect its header, exit status, final response and diff. After a disconnect, check the remote process before retrying. Retain the prompt, log and receipt.

## Route and reconcile fleet jobs

Use scoped `fleet_discover` and positive resource minima. Prefer Echo through 12 GB, L4 for bursts above 12 through 24 GB and G4 for 96 GB-class video. Use `fleet_submit_job` with `async:true`, a saved `idempotency_key` and profile-bounded `timeout_secs`. Batch G4 shots. Set `pin_gpu_uuid` only for an exact device.

On a lost submit reply replay the frozen request with the same key. After an ID arrives, resume via `fleet_get_job`. Check terminal state and `output_sha256`, verify artifact bytes against their own digest and confirm capacity release. Reconcile `failed_uncertain` or held capacity before repeating compute. Continue independent CPU work during capacity gaps.

- **Colab controller.** On controller hosts set `enable_update_check` to `false` in `~/.config/colab-cli/settings.json`. The 0.7.5 update banner shares stdout with session data. Filter only known update-notice prefixes. Unknown output still means `sessions_ambiguous` and must block allocation until reconciled.
- **Columbus RunPod Pods.** A one-shot command can exit while `desiredStatus` stays `RUNNING`. Use ephemeral per-job ed25519 SSH (`PUBLIC_KEY`, TCP/22). Load the separate provider key file with `O_NOFOLLOW`. Reserve budget atomically across lanes. Allow cold pulls their own `startup_timeout_secs` (1 to 3 minutes observed) and use `idle_timeout_secs` for progress. Retry allocation once after backoff only if the failure is transient, spend is zero, the balance floor holds and no owned pod exists. Keep the same job reservation for that retry. Collect command exit and artifact, verify its digest, then confirm pod teardown and reservation release.

## Public release hygiene

Use private run directories, sanitized fixtures and secret-backed clients with tracing off. Keep raw transcripts and replay material out of prompts, exports and handoffs. Before an authorized public push, export the exact candidate to a clean directory, check outgoing history including deleted content, and follow the github-repo-design skill for mirror selection and metadata. Run both checks on the exported tree with a trusted scanner.

```bash
rg --hidden --no-ignore --pcre2 -l '[\x{2013}\x{2014}]' "$PUBLIC_TREE"
python3 -B "$SCANNER_DIR/crew-scan.py" "$PUBLIC_TREE" "$PACKAGE_ID"
```

The dash scan must exit 1 (no matches). Require scanner exit 0 and zero raw, unruled and namegrep hits. Re-export and repeat after any change.
