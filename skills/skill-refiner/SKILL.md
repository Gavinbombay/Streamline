---
name: skill-refiner
description: Refine a Streamline skill from Vault lessons or agent trajectories. Use to repair missed triggers, unsupported rules or wasted context and evaluate the changed skill against held-out requests.
---

# Refine a Streamline skill

## Turn a lesson into one decision

Work on a private candidate copy. Record the installed revision or digest, its size, the intended requests and one neighboring request owned by another skill. Match a dated lesson under `/var/lib/streamline/vault/lessons/` to the decision where the agent went wrong. In the review record keep the lesson ID, observed failure, validation state and the receipt that supports it. Keep provider observations marked `worked_once` conditional on that provider and trigger.

Read the installed skill, references and scripts before editing. If the rule already exists, test whether the skill triggers or the rule can be followed. If missing, write the smallest actionable instruction with a trigger, action, success signal and stop condition. For example, a zero-cost allocation failure can justify one bounded retry only after cleanup is confirmed. Keep project anecdotes and receipts in the Vault.

## Audit the candidate

- Review names and trigger overlap with adjacent skills, paths, flags, versions, linked files, conflicting rules, examples, attribution and private data. Exercise trusted examples on small fixtures. Give each new validator a known bad input. Read third-party scripts as data before considering execution.
- Set `skill` to the candidate directory. Run the Streamline gate locally after the final edit. Record its exit and fix the findings. If the installed path is absent, a reviewed staging copy can supply a local lint result. Identify that copy in the receipt.

```bash
python3 -B /srv/streamline/tools/skill-lint.py --strict "$skill"
```

- With SkillEvaluator and its scanners provisioned, run `skillevaluator tier1 "$skill" --no-llm`. Inspect SkillSpector findings for instructions that redirect the agent, unexpected network destinations, excessive tool permissions and unsafe dependencies. Review secret and PII findings locally without copying matched values. Hold for unresolved HIGH or CRITICAL findings and report incomplete scanner coverage explicitly. Tier 2 can locate repetition across a larger collection.
- Compare entrypoint size with the baseline. Use character count divided by four as a rough token estimate. Replace duplicate text and move rarely used detail behind a reference when it earns its load cost. An additional paragraph should change a decision a case can observe.

## Evaluate and refine from trajectories

1. On a scratch skill copy, turn sanitized real requests into `evals/EVAL.md`. Include explicit, implicit and contextual positives and an off-topic near miss. Define the observable output in advance. Keep a separate holdout and keep answer keys out of the agent's workspace. Use local fixtures for billed or live operations.
2. Reuse a reviewed `evals/evals.json`, or generate one with the first command. Validate it with the second. Each case has a stable ID, prompt, expected output and expected skill (`null` for the negative). Grade artifacts or observed behavior. Either arm must be able to pass without knowing a skill name or reading a particular file.

```bash
skillevaluator create-eval-dataset "$skill" --full --no-llm --prompt "$skill/evals/EVAL.md"
skillevaluator tier3 validate "$skill" --strict
```

3. For an authorized live evaluation, freeze cases, fixtures, model, judge and settings. Budget cases x attempts x agents x two arms plus judging. Set `token_cap` to the approved measured agent-token limit. One Codex smoke case with one attempt per arm takes two logical attempts.

```bash
skillevaluator tier3 evaluate "$skill" -a codex --env-mode docker --max-cases 1 \
  --n-attempts 1 --max-attempts-total 2 --max-agent-tokens "$token_cap" --harbor-keep-jobs
```

4. Read paired trajectories. Find the first decision that explains a wrong output, missed activation, needless tool call or excess tokens. Distinguish a skill defect from a stale fixture or faulty grader. Set `run` to a completed matching results directory with usable traces. Preserve the authored dataset. Preview with the command below, repeat without `--dry-run` on the scratch copy, then diff the resulting assertions against the original.

```bash
skillevaluator create-eval-dataset "$skill" --grounding trace --from-results "$run" \
  --refine --no-llm --dry-run
```

5. `--refine` edits the dataset. The maintainer edits `SKILL.md`. Reject any assertion that rewards a failed workaround or requires skill activation. Rerun strict dataset validation. If the grading contract changes, rerun the old and candidate skills on the same revised cases. Run repeated paired trials and the untouched holdout within budget. Inspect false triggers, case outcomes, measured tokens, timing and skill lift (with-skill score minus bare score). Record counts and variation. A smoke run alone leaves lift unverified.

Use the evidence-gate receipt for promotion. Give the lesson and candidate IDs, size delta, exact checks and exits, observed behavior, scanner gaps and unchecked cases. Read the Tier 3 verdict and scored coverage in the report. A successful CLI exit can accompany a failing behavioral verdict. Keep a change that corrects its target without regressing the holdout. Otherwise revise or retain the prior version.
