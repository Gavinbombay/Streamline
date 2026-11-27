---
name: bpy-lint-before-render
description: "Use when a coding lane hands over a bpy script for the render-blender lane, as a cheap QC gate before any GPU time: type-check the script against Blender 4.5 API stubs to catch 5.x-only or misspelled API names, then run a 1-frame smoke test."
---

# Lint bpy scripts before rendering

Why: lane models often write Blender 5.x API, or invent attribute names. Each mistake costs a container start, a bake or a render slot before it fails. A stub type-check takes seconds and needs no Blender.

## Tool
- `fake-bpy-module-4.5` (PyPI, MIT, by nutti). It's type stubs for bpy, bmesh, mathutils and friends generated from Blender 4.5. Version seen on PyPI on 2026-10-26: `20260730`. Pin it exactly.
- Install it only in a throwaway venv on a build host, never into the Blender container and never globally. (Installing is a separate step that needs its own go. This skill only documents it.)
```bash
python3 -m venv /tmp/bpylint && . /tmp/bpylint/bin/activate
pip install 'fake-bpy-module-4.5==20260730' pyright   # or mypy; mypy 2.4.0 verified
```

## Run
```bash
. /tmp/bpylint/bin/activate
pyright --pythonversion 3.11 job.py        # Blender 4.5 bundles Python 3.11
python -m py_compile job.py                # syntax only, as a fallback
# verified alternative (no Node needed):
mypy --python-version 3.11 --check-untyped-defs --ignore-missing-imports job.py
```
- **`import bpy` at module level.** If the script passes `bpy` around as an untyped function parameter (`def f(bpy, ...)`), the checker sees `Any` and checks almost nothing. The p3a scripts do this. Lint them via a scratch copy that swaps in the module, or ask the lane to annotate.
- Expect false positives: the stubs type `modifiers.new()`, `lights.new()` and `constraints.new()` as the base class, so subclass attributes (`domain_settings`, `node_group`, `shape`, `target`, ...) show as `attr-defined`. `keyframe_insert(data_path=...)` shows as `call-arg`. Triage these against a runtime attribute check on the pin before calling them blocking.
- Treat `reportAttributeAccessIssue`, `reportCallIssue` and unknown import symbols on `bpy.*` as **blocking**.
- Treat `Optional` and `None` complaints (`bpy.context.object` may be None) as warnings, but fix them when they're cheap: headless contexts really do give None.
- Stubs can't see dynamic properties (custom props, add-on registered types, `node.inputs["Name"]` socket names). Those are checked by the smoke test below, not by lint.

## Smoke test (still before the real job)
1. Run the script once on 1 frame at the lowest samples and resolution through the normal worker path (or a CPU proof run): `--python-exit-code 1`, so any traceback fails it.
2. Required log lines: `DEVICES` (blender-headless-bpy), `FLUID` and `LOOK` where they apply, and `JOB_OK`.
3. The output file exists, isn't empty, and isn't all one colour.

## Verification gates for the brief
- A job brief that ships a bpy script names this skill in its done-when test: lint clean (no blocking codes) plus the smoke test passing.
- The QC lane that runs the lint must not be the lane that wrote the script (crew rule: a lane never QCs its own work).
- Record the stub package version and the pyright version in the job manifest.

## Verified
- **Box, 2026-10-26:** `fake-bpy-module-4.5==20260730` installs (pip normalises the name to `fake-bpy-module-4-5`) with mypy 2.4.0 in a Python 3.11 venv. Run against the v11.3 p3a scripts, it found no real API errors. The flags were Optional warnings plus the base-class false positives above, each confirmed at runtime on bpy 4.5.15. Report: `../../LINT_p3a_fake-bpy-4.5.md`.
- **Corrected:** I added the mypy route, the untyped-`bpy`-parameter blind spot and the false-positive triage.
- **Not verified:** pyright (not run; it needs Node), the exact pyright rule names (`reportAttributeAccessIssue`, `reportCallIssue`), and the smoke-test log lines on the pinned container.

Sources: nutti/fake-bpy-module (MIT; github.com/nutti/fake-bpy-module, PyPI package `fake-bpy-module-4.5`). Blender 4.5 Python API reference (docs.blender.org/api/4.5/). Nothing copied verbatim.
