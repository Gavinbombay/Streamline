---
name: cycles-gpu-multi-worker-render
description: "Use when splitting a Cycles render across more than one GPU worker (echo-3060, the 2070 worker, colab-l4) through the render-blender lane: frame sharding and chunk size, OptiX vs CUDA per card, per-frame checksums, and merging shards into one verified frame set."
---

# Cycles across several GPU workers

Pick the lane with compute-lane-access. This skill covers how to split and stitch the job. Every worker must run the same pin (`blender-4.5.14-cycles`), because the router refuses a mismatch. Never mix versions within a shot.

## Plan the split
- Shard by **frame range**, not by tiles. Each worker renders whole frames, so shards are independent and easy to re-run.
- Chunk size: small for heavy or fluid scenes (1 to 5 frames). Long chunks hung on Colab L4 during the v11 splash while single frames rendered fine. Use larger chunks (10 to 30) for light scenes to cut startup overhead.
- Run a 1-frame probe per worker first and use its seconds-per-frame to weight the split. As a guide, the 2070 and 3060 are both about 0.5 to 0.8 s for a 720p, 16-sample still. Real shots are far slower, so measure.
- Same .blend and inputs on every worker (hash them in the job spec). If the shot uses a baked sim, every worker must get the identical baked cache (see mantaflow-liquid-bake-headless).

## Commands
Through the router (default): one job per range. The router chunks and dispatches to a free, proven worker:
```bash
{ printf 'Authorization: Bearer '; cat ~/.config/service-lanes/render-blender.token; } | \
  curl -sS -H @- -H 'Content-Type: application/json' \
  -X POST "$LANE_ROUTER_URL"/v1/lanes/render-blender/jobs -d @job.json
```
(Run this on the router host. Never put the token in args or logs.) Colab directly: `colab-l4-run` with spec fields `frames A-B`, `chunk`, `engine CYCLES`, `device OPTIX`. Add `watchdog: true` for heavy scenes.

Raw Blender inside a worker, for reference only. Order matters, and `-a` comes last:
```bash
blender -b scene.blend --python-exit-code 1 --python gpu_setup.py \
  -o /work/out/frame_#### -F PNG -x 1 -s 101 -e 120 -a
```

## Device per card
- RTX 3060, RTX 2070 and L4 all support OptiX and CUDA. Default to OPTIX, and fall back to CUDA when OptiX reports no device or rc 1 (seen on L4).
- Set the device in the script (blender-headless-bpy). `--factory-startup` resets it to CPU.
- Inside Docker, the worker needs `--runtime=nvidia --gpus all --security-opt apparmor=unconfined` (cuInit 304 otherwise). Pick one GPU per container with `--gpus device=N` or `CUDA_VISIBLE_DEVICES`, rather than Blender's device list, whose order isn't reliable across identical cards (Flamenco FAQ).
- Keep the 2048 MiB VRAM floor on a card shared with a local LLM server. Never unload it for a render.
- Pin `scene.cycles.seed` and the samples. Keep `use_animated_seed` the same across shards, or the noise pattern jumps at shard edges.
- Denoising: OIDN on GPU and on CPU can differ slightly. For one shot, use the same denoiser and device class on every worker, or denoise later in one compositing pass.

## Merge and verify
1. Each worker's `out/` has the frames, SHA256SUMS and manifest.json. Run `sha256sum -c SHA256SUMS` on each shard after download.
2. Check coverage: every frame A to B is present exactly once (`ls | sort` against `seq -w`), with no zero-byte files.
3. Check that every manifest has the same Blender version, cartridge, seed, samples and resolution. Reject the shard if any differ.
4. Watch seams at shard boundaries: compare the last frame of one shard with the first of the next (mean abs diff). A jump beyond what the motion explains means mismatched settings.
5. Write one combined SHA256SUMS plus a merge manifest listing which worker rendered which frames.

## Verified
- **Box, bpy 4.5.15 LTS (pip wheel, no GPU), 2026-10-26** (probe `../../probe_api_45.py`, output `../../probe_box_4.5.15.json`). Confirmed: the `compute_device_type` ids `OPTIX` and `CUDA` (plus `HIP`, `ONEAPI`, `NONE`), `scene.cycles.device` `GPU`, `scene.cycles.seed` and `scene.cycles.use_animated_seed`, and `OPEN_EXR` as a file format. `--factory-startup` leaves the device type at `NONE`, so set it in the script.
- **Corrected:** none.
- **Pending / not verified:** real GPU device lists and OptiX vs CUDA on each card (pinned 4.5.14 container per HOST_PROBE.md), the raw CLI render line (`-o ... -F PNG -x 1 -s -e -a`), the router endpoint, `colab-l4-run` spec fields, the timing guide and the Docker flags. Those are lane facts, not API, and this probe doesn't cover them.

Sources: Flamenco docs and FAQ (flamenco.blender.org; GPL-3.0 project, used only for its patterns: frame chunking, one worker per GPU, factory-startup GPU reset). Blender 4.5 manual "Command Line Rendering" and "GPU Rendering". Crew lane facts from compute-lane-access. Nothing copied verbatim.
