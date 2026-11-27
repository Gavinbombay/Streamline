---
name: 3d-asset-production-multi-gpu
description: Use when producing game-ready GLBs with Hunyuan3D-2 and Blender 4.5 across the Diffuser fleet or checking PBR assets in the target Three.js game.
---

# Multi-GPU 3D asset production

## Assign the sprint before launching workers

Match Blender 4.5.16 LTS on Echo and Warp. Give each heavy lane one GPU. Use Diffuser Colab L4 (24 GB) for image-to-3D, Echo RTX 3060 for character retopo and turntables, Warp RTX 2070 for environments and procedural props, and A15 RTX 4060 for the NVENC review reel. Inject the installed `blender-headless-bpy`, `bpy-lint-before-render`, `cycles-gpu-multi-worker-render` and `blender-compositing-lookdev-45` skills into the worker briefs before spawning. For noninteractive Cursor runs in a new trusted worktree, use the `cursor-nuc-run.sh` wrapper or pass `--trust` to `cursor-agent` so an invisible prompt cannot stall the lane.

## Hunyuan3D-2 on Colab L4, Python 3.13

1. Start an organic character from a clean, square single-subject studio still with neutral lighting, a solid warm backdrop and no text. Stage input archives with the verified Diffuser artifact-reader account and group as owner and mode `0600`. Submit shape and PBR paint as an async Diffuser `colab.l4.exec/v1` job.
2. Export `HF_HUB_DISABLE_XET=1` and `PYTHONUNBUFFERED=1`. Pin `transformers==4.46.3`, `diffusers==0.31.0`, `huggingface-hub>=0.25,<1.0`, `tokenizers<0.21` and `pillow<13.0`. Transformers 5.x renames the DINOv2 attention keys expected by `SingleImageEncoder`.
3. Install `pybind11`. Run `python3 setup.py install` in `hy3dgen/texgen/custom_rasterizer` and `bash compile_mesh_painter.sh` in `hy3dgen/texgen/differentiable_renderer`. On the tested Python 3.13 path without a pymeshlab wheel, use this import shim only where no mesh operation needs that library. A dynamic `__getattr__` returning `object` breaks source inspection.

```python
import sys
import types

pm = types.ModuleType("pymeshlab")
pm.__file__ = "/tmp/pymeshlab_stub.py"
pm.MeshSet = pm.Mesh = pm.PercentageValue = pm.PureValue = object
sys.modules["pymeshlab"] = pm
```

4. Save the working directory before entering build directories. In Bash enable `set -o pipefail` and stream `run_main 2>&1 | tee "$LOG"` for the idle watchdog. Put `colab_run.log`, GLBs and `run_summary.json` in a returned `files_out` tarball under 64 MiB. If the pipeline fails, stop and inspect the returned log before paying for another run.

## Blender export and browser acceptance

Run Blender with `--background --factory-startup --python-exit-code 1` and enumerate CUDA/OptiX devices explicitly. Center X/Y, put feet at Z >= 0 and face +Y. Verify the game's triangle budget (pony <= 14,500, child <= 19,000 in the sanctuary), pack JPEG PBR with `export_image_quality=88` and inspect four AgX Cycles turntable angles.

- **Embedded textures, Three.js r170.** For the offline GLB loader using fetch on embedded images, allow `connect-src 'self' blob:` in the applicable CSP. A white asset calls for a console and texture-load check. For dark organic characters inspect the imported material. An omitted glTF metallic factor defaults to 1. Set `metalness = 0.0` and `roughness = 0.85` on the character surfaces, use PMREMGenerator for environment lighting and add warm stall fill. Preserve intentionally metallic props.
- **Storybook scene.** Give snow and motes a shared 64x64 radial CanvasTexture with `depthWrite: false`. For winter use blue-hour sky and snow, warm window and lantern PointLights, and hide meadow grass and flowers. Mirror ground textures, match directional shadow-camera bounds and extend doorway backdrops below y = 0.
- **Live review.** Use a fresh port or unregister the old service worker and reload. `?nosw` can leave an installed worker serving mixed builds. Capture the tested build at 1280x800. Inspect UV orientation, color, silhouettes, seams, thresholds and collisions. For storybook games, play every story, activity, season and panel, including Back, title selection, camera recovery, winter stalls and replay. Exercise the Grown-ups parchment panel's live season switch, comfort controls and collapsible save/transfer. Keep screenshots and the path taken with the handoff.
