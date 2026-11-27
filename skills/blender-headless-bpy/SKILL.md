---
name: blender-headless-bpy
description: "Use when writing or reviewing a bpy job script that runs headless in the pinned Blender 4.5.14 render-blender container (any lane brief that ships a .py to Blender): CLI shape, argument passing, GPU setup inside the script, context-free API patterns, and the checks a script must pass before it goes to the router."
---

# Headless bpy job scripts (Blender 4.5.14)

Scope: how to write the script. Where it runs is in compute-lane-access (router, workers, pin). The container itself is in blender-worker-recipe. Keep to the one pin: cartridge `blender-4.5.14-cycles`. Never mix Blender versions within a shot.

## CLI shape
```bash
blender --background --factory-startup --python-exit-code 1 \
  [scene.blend] --python job.py -- --out /work/out --frames 1-10
```
- Put `--background` first. With `--factory-startup` you skip user prefs and add-ons, which makes runs reproducible, but see the GPU gotcha below.
- `--python-exit-code 1` makes a Python exception exit non-zero, so the adapter's exit codes (0 done, 1 failed) mean something. Without it, Blender can exit 0 after a traceback.
- The order of arguments matters: Blender handles them left to right. Load the .blend before `--python`. Render flags (`-s`, `-e`, `-a`, `-f`) come after the file and the output settings.
- Everything after `--` is for your script only:
```python
import sys, argparse
argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
args = argparse.ArgumentParser().parse_args(argv)  # add your args
```

## GPU setup belongs in the script
`--factory-startup` (and any fresh container) resets Cycles to **CPU**: compute device None, no devices enabled. Flamenco's FAQ calls this out. Always set the device explicitly:
```python
import bpy
prefs = bpy.context.preferences.addons["cycles"].preferences
prefs.compute_device_type = "OPTIX"   # or "CUDA"; fall back to CUDA if OPTIX gives 0 devices
prefs.get_devices()                    # 4.5: refreshes prefs.devices in place, returns None
# alt: prefs.refresh_devices(); prefs.get_devices_for_type("OPTIX") -> list for one type
gpus = [d for d in prefs.devices if d.type == prefs.compute_device_type]
for d in prefs.devices:
    d.use = d in gpus
if not gpus:
    raise SystemExit("no GPU device for " + prefs.compute_device_type)  # refuse, don't silently CPU-render
bpy.context.scene.cycles.device = "GPU"
print("DEVICES", [(d.name, d.type, d.use) for d in prefs.devices])
```
Instead you can pass `-- --cycles-device OPTIX` (or CUDA) on the CLI, but the script approach logs what was actually used. Known issue (compute-lane-access): on the v11 splash, OptiX failed on Colab L4 and fell back to CUDA. Log the device and treat a CPU fallback as a failure.

## Context-free patterns
- Prefer `bpy.data` and `bmesh` over `bpy.ops`. Operators depend on selection, mode and area, and in background mode there is no 3D view.
- If you must call an operator, override its context and check `poll()`:
```python
with bpy.context.temp_override(active_object=obj, object=obj, selected_objects=[obj]):
    if not bpy.ops.object.modifier_apply.poll():
        raise RuntimeError("poll failed: modifier_apply")
    bpy.ops.object.modifier_apply(modifier=mod.name)
```
- Don't use edit-mode operator chains. Build meshes with `bmesh.new()`, `bm.to_mesh(me)` and `me.update()`.
- Paths: keep them relative inside the .blend (`//`). Call `bpy.path.abspath()` before any file I/O. Echo jobs read inputs from `jobs/<id>/inputs` (env `E3_INPUTS`), not `/work/inputs`.
- Determinism: set `scene.cycles.seed` to a fixed value (the recipe pins seed 0), set sample counts explicitly, and record them in the manifest.
- The worker runs with `--network none`. A script must never fetch anything (no pip, no asset downloads), and every asset ships in the job's inputs.
- Don't call `sys.exit()` mid-script with frames half-written. Raise instead, so `--python-exit-code` reports it.

## Gotchas
- Blender 5.x snippets from the web or from model memory often break on 4.5 (for example the compositor node-group API and renamed nodes). Check against https://docs.blender.org/api/4.5/ and run bpy-lint-before-render.
- `bpy.context.scene` in background mode is the scene that was saved active. Name the scene explicitly if the .blend has more than one.
- xvfb-run can exit 1 even when Blender succeeded. Judge success by Blender's exit code plus the output checks (compute-lane-access).
- Data-block names change on collision (`Cube.001`). Look up by stored reference, not by guessed name.

## Validation before submit
1. Lint passes (bpy-lint-before-render).
2. A local or CPU dry run on 1 frame at low samples: Blender exit 0, the `DEVICES` line is logged, the output file exists and isn't empty.
3. The script prints a final `JOB_OK <n_frames>` line that the adapter can grep.
4. The manifest records the Blender version (`bpy.app.version_string`), seed, samples, device and the SHA-256 of each output.

## Verified
- **Box, bpy 4.5.15 LTS (pip wheel, headless, no GPU), 2026-10-26** (probe `../../probe_api_45.py`, output `../../probe_box_4.5.15.json`). Confirmed on `CyclesPreferences`: `get_devices()` exists, refreshes `prefs.devices` and returns None. `refresh_devices()`, `get_devices_for_type(type)`, `get_device_types(context)`, `get_num_gpu_devices()` and `has_active_device()` also exist. The `compute_device_type` items are `NONE`, `CUDA`, `OPTIX`, `HIP` and `ONEAPI`. A factory start leaves it at `NONE`, which confirms the CPU reset. `scene.cycles.device` is `CPU`/`GPU`. `scene.cycles.seed` and `use_animated_seed` exist. `bpy.context.temp_override` exists. The Python version is 3.11.
- **Corrected:** none. The names were right. I added the note on what `get_devices()` returns and the `get_devices_for_type` alternative.
- **Pending (pinned 4.5.14 container, HOST_PROBE.md):** real GPU enumeration (the box has no GPU, so the device list was CPU only), the CLI flags (`--python-exit-code`, argument order), and `-- --cycles-device`. The pip wheel has no CLI.

Sources: remiehneppo/blender-skills `blender-headless` skill (MIT; skill docs) for the CLI and context-override structure. Blender 4.5 manual "Command Line Arguments" and API "Gotchas" and "Tips and Tricks" (docs.blender.org). Flamenco FAQ (flamenco.blender.org) for the factory-startup GPU reset. Adapted and summarised; nothing copied verbatim at length.
