---
name: blender-compositing-lookdev-45
description: Use when a shot's look (color management, bloom and glare, denoise, grain, final grade) has to be set by bpy script in Blender 4.5.14 rather than by hand, for example the Ember Dusk house look, so every worker renders the identical look headless.
---

# Compositing and lookdev by script (Blender 4.5.14)

Goal: the look lives in a script that's checked in with the shot, not in someone's GUI session. Then every render-blender worker reproduces it byte for byte. Palette and tokens come from the shot's look skill (for example ember-dusk-look). This skill covers only the bpy mechanics.

## 4.5 vs 5.x: the first trap
- In 4.5 the compositor is `scene.use_nodes = True` plus `scene.node_tree`. Blender 5.x moved to a compositing node group (`scene.compositing_node_group` does not exist on 4.5). Snippets written for 5.x (including upstream skill packs) won't run on our pin.
- EEVEE's old bloom checkbox was removed in 4.2. Bloom is done with the compositor **Glare** node.
- Some compositor node options moved from properties to input sockets in recent 4.x releases. Don't guess: print `[(i.name, i.type) for i in node.inputs]` and `dir(node)` once on the pinned build and code against that.
- **Glare in 4.5 (verified):** the only real properties are `glare_type` (`BLOOM`, `GHOSTS`, `STREAKS`, `FOG_GLOW`, `SIMPLE_STAR`) and `quality` (`HIGH`, `MEDIUM`, `LOW`). Everything else is an input socket, by name: `Threshold` (1.0), `Smoothness` (0.1), `Clamp` (False), `Maximum` (10.0), `Strength` (1.0), `Saturation` (1.0), `Tint`, `Size` (factor 0..1, default 0.5), `Streaks` (4), `Streaks Angle`, `Iterations` (3), `Fade` (0.9), `Color Modulation` (0.25), `Diagonal` (True). Outputs: `Image`, `Glare`, `Highlights`.
- The old properties (`threshold`, `mix`, `size`, `iterations`, `streaks`, `fade`, `angle_offset`, `color_modulation`, `use_rotate_45`) are still there but marked Deprecated. **Don't use them.** On 4.5.15, `node.threshold = 0.5` left the `Threshold` socket at 1.0 and printed `RNA_float_set: NodeSocket.default_value not found`. Set `node.inputs["Threshold"].default_value` instead.
- A new Glare node comes up as `STREAKS` with quality `MEDIUM`, not the RNA default. Always set `glare_type` and `quality` explicitly.

## Skeleton
```python
import bpy
sc = bpy.context.scene
sc.view_settings.view_transform = "AgX"      # verified 4.5.15: exact id "AgX"
sc.view_settings.look = "None"               # AgX looks are "AgX - Punchy", "AgX - High Contrast", ... (set from the look skill)
sc.view_settings.exposure = 0.0
sc.render.image_settings.color_management = "FOLLOW_SCENE"
sc.use_nodes = True
nt = sc.node_tree
nt.nodes.clear()
rl  = nt.nodes.new("CompositorNodeRLayers")
gl  = nt.nodes.new("CompositorNodeGlare")    # bloom or streaks; settings from the look skill
gl.glare_type = "BLOOM"; gl.quality = "HIGH"  # new nodes default to STREAKS / MEDIUM
gl.inputs["Threshold"].default_value = 1.0     # sockets, not the deprecated gl.threshold
out = nt.nodes.new("CompositorNodeComposite")
nt.links.new(rl.outputs["Image"], gl.inputs["Image"])
nt.links.new(gl.outputs["Image"], out.inputs["Image"])
print("LOOK", sc.view_settings.view_transform, sc.view_settings.look, [n.bl_idname for n in nt.nodes])
```
- Denoise: either Cycles render denoise (`sc.cycles.use_denoising`, `sc.cycles.denoiser = "OPENIMAGEDENOISE"`) or a compositor Denoise node fed by the Denoising Data passes. Pick one per shot. Proofs keep denoise off for determinism (blender-worker-recipe).
- Selective bloom (for example only the lamp and beam): render those objects to their own view layer or Cryptomatte matte, glare that pass only, then mix it over the beauty pass. Don't glare the whole frame.
- Grain and haze are better in post (ffmpeg or the video tool) than in Blender, so you can tune them without re-rendering. Keep the Blender output clean and linear-ish (EXR or 16-bit PNG).

## Gotchas
- Output format changes the look. 8-bit PNG plus AgX bands in dark gradients. Use 16-bit PNG or EXR for frames that will be graded.
- `view_transform` "Standard" and "AgX" differ hugely. A missing line falls back to the .blend's saved setting, which may be someone's test value. Always set it.
- The compositor runs per frame, after rendering. A broken node tree can still output a black frame with exit 0, so check the pixel stats.
- Clipping: the v11 blowout fix measured the % of clipped pixels. Keep that gate for any shot with bright lights or water reflections.

## Validation
1. The `LOOK` line is in the log and matches the shot's look skill.
2. Render one probe frame. Mean luminance and clipped % are inside the shot's gates (ImageMagick `identify -verbose` or a small numpy check).
3. Diff the probe against the approved reference still for the same frame. A large diff means the look drifted.
4. Commit the look script next to the shot and record its sha256 in the render manifest.

## Verified
- **Box, bpy 4.5.15 LTS (pip wheel, headless), 2026-10-26** (probe `../../probe_api_45.py`, output `../../probe_box_4.5.15.json`). Confirmed: `view_transform` items `Standard`, `Khronos PBR Neutral`, `AgX`, `Filmic`, `Filmic Log`, `False Color`, `Raw` (`AgX` is the exact id and the factory default). The look list for AgX is `None` plus the `AgX - ...` set. `image_settings.color_management` is `FOLLOW_SCENE`/`OVERRIDE`. `scene.use_nodes` and `scene.node_tree` exist. `CompositorNodeRLayers`, `CompositorNodeComposite`, `CompositorNodeDenoise` and `CompositorNodeGlare` exist. The Glare sockets and properties are as listed above. `scene.eevee.use_bloom` is gone (it's not present on 4.5). `cycles.denoiser` has `OPENIMAGEDENOISE` (on a GPU host, `OPTIX` may also appear, since the enum is dynamic). `use_denoising` exists.
- **Corrected:** Glare settings were assumed to be node properties. In 4.5 they are input sockets, and the deprecated property setters are unreliable. Added the explicit `glare_type`/`quality` lines.
- **Pending:** the same probe on the pinned `blender-4.5.14-cycles` container (HOST_PROBE.md). Not verified: the 16-bit/EXR banding claim, the selective-bloom recipe, the pixel-stat gates.

Sources: ra100/blender-claude-plugin `blender-compositing-nodes` and `blender-shader-nodes` skills (MIT) for the coverage outline, re-targeted from Blender 5.x to 4.5. Blender 4.5 manual, Compositing and Color Management sections (docs.blender.org). Summarised; nothing copied verbatim at length.
