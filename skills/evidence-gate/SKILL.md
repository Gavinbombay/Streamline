---
name: evidence-gate
description: Verify a fix, refactor, artifact or release claim before handoff. Use when Codex, Hermes or Antigravity is asked for proof, or when another lane will rely on a claim that work passes or is ready to ship.
---

# Evidence gate

1. **Name the claim.** Identify the candidate revision or artifact digest, the user's entry point, one input and the observable result. Predict what the broken case will do. A worker reporting success is evidence that the worker finished. Check the returned artifact for claims about its contents.
2. **Reproduce first.** For a fix, exercise the original failure on the base in a disposable checkout or fixture. Bring the regression assertion into both checkouts. Keep inputs, dependencies, settings and command identical. The base must fail for the predicted reason. A missing dependency, timeout during setup or zero tests collected leaves the reproduction open.
3. **Prove fail-before-pass.** Run the unchanged check on the candidate and observe the corrected result. For an unchanged contract, compare base and candidate outputs, errors and side effects. Plant one specific wrong result in a scratch copy, watch the assertion reject it, then run it on the clean candidate. For an artifact, use a known bad fixture, then exercise the delivered bytes in their intended viewer. Keep each conclusion within the cases exercised.
4. **Check the handoff tree.** For a release, identify the exact staged tree, commit or packaged bytes. Read the diff for scope, changed callers, conflict markers, debug leftovers and exposed private data. Rerun the affected checks on that tree after the last edit. Inspect the user's actual surface when it matters, such as playing the mastered MP4 with sound. A lint pass supports a lint claim.
5. **Give the verdict first.** Use **Proven**, **Partial** or **Unverified** for the named behavior. For a ship decision use **SHIP** only when the agreed release checks ran on this candidate and passed. Use **HOLD** for a blocker, a missing required check or an ineffective control. Ship readiness conveys no publication authority.

## Verification receipt

For each material claim, leave a short receipt another lane can replay.

| Claim and candidate | Exact command and working directory | Base or faulty control | Candidate | Unchecked |
| --- | --- | --- | --- | --- |
| Observable behavior and revision or digest | Command actually run using secret-free arguments | Exit status and distinguishing observation | Exit status and observed output or artifact | Missing case, reason and next check |

For a manual check, record the steps, artifact and observation. Use safe status lines or counts and suppress sensitive output. Label supplied logs as reported, code inspection as read, and proposed commands as unrun. Say **fixed** only if you executed the reproduction and observed the candidate pass. When execution is blocked, report the narrower evidence and the exact check still needed.
