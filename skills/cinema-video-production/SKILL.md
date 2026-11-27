---
name: cinema-video-production
description: Use when producing multi-shot films or narrated storybook cutscenes with reference-locked keyframes, MiniMax H3 or FastH3 on Diffuser Colab G4, native dialogue and local ffmpeg mastering.
---

# Cinema video production

## Lock picture and dialogue before renting a GPU

Inspect and approve the first 16:9 still. Use it and subsequent approved anchors as references for wardrobe, faces, props, geometry and grade. Frame one primary speaker per dialogue shot and cut to the reverse for the reply. Hold the camera steady or use a short push. A 180-degree pivot can flip space and a pan across clapboard can make horizontal lines wobble. For a precise state change, make a pixel-matched end still and pass both `FIRST_FRAME` and `LAST_FRAME` to the Studio `scripts/video.sh`.

Use the three MiniMax H3 sections. Give the speaker an age, vocal texture, accent and stable speaker ID.

```text
integrated_multimodal_description: [Shot 1] Steady close-up in the barn. A seven-year-old boy (S1), in a soft coastal voice, says: <d>[English] She's home.</d> His mouth movements follow his speech.
overall_soundscape: Straw rustle and quiet hoofsteps.
non_diegetic_music: N/A
```

## Submit one batch and read its telemetry

1. Pack reference frames in one `frames.tar` to fit the four-file `files_in` limit. In the G4 or L4 lane's artifact directory, fix each staged input archive before submitting. A root-owned 0600 tar causes `artifact_path_unavailable`. Set `archive` to its staged path and set `artifact_owner` and `artifact_group` to the verified Diffuser artifact-reader account and group for that lane. Require the final line to report that owner and group with mode `600`.

```bash
sudo chown "$artifact_owner:$artifact_group" -- "$archive" && sudo chmod 0600 -- "$archive" &&
  stat -c '%U:%G %a' -- "$archive"
```

2. Cache the complete weight bundle once on persistent Drive or in a private bucket. For a cold fetch, use secret-backed Hub authentication if available, export `HF_HUB_DISABLE_XET=1` before setup and resume `bash oneshot-setup-spark.sh --no-render` on transient download errors. Allow four attempts total, 60 seconds between attempts. Check expected weight shards and sizes before the first render. Keep authentication out of traces.
3. Use async Diffuser `colab.g4.exec/v1` for heavy video, with positive resource minima and several shots per warm 96 GB session. For a POSIX `sh -c` wrapper, transport a secret-free Bash script as base64, decode to a job-local file, then invoke Bash. Start at `WIDTH=1344 HEIGHT=768 STEPS=16 SPARK_WEIGHTS=bf16 DELIVERY_AUDIO_RATE=48000`. Keep routed attention unless dense passes are required. Use frame counts `17*n + 5`, e.g. 141 or 192. Return raw clips together in `shots.tar`.
4. Budget from the observed G4 rate of about 10.05 CU/hour. Use the lane's allowed outer walltime (e.g. 5400 seconds), a progress watchdog (`idle_timeout_secs=1200`), explicit `max_cu` and the 200 CU reserve. If telemetry confirms transient `allocation_failed`, zero CU spent and zero live sessions, wait 30 to 60 seconds and retry once. Reconcile unknown spend or session state before resubmission.
5. If G4 stays unavailable for a storybook, keep testing with ElevenLabs narration and a local 720p ffmpeg Ken Burns cut from the approved stills. Mark it as the fallback cut. Replace it with H3 motion when capacity returns within budget. For ages 3 to 7, place voiced beats at the intro, between activities and at the finale, and let children replay chapters from the Barn Storybook.

## Master and watch the delivered file

Use `xfade` for 0.20 to 0.35 second dissolves within a location and about 0.65 seconds of `fadeblack` at act boundaries. Use `acrossfade` and continuous room tone or J-cut exterior beds under dialogue. Skip `loudnorm` on ambient-only shots to avoid lifting wind. For requested generated music use the Studio helper or Music3's `--model`, `--weights` and `--serve 1` interface. For a permitted regional voice swap, use ElevenLabs Speech-to-Speech (`eleven_multilingual_sts_v2`) or Instant Voice Cloning, then align mouths with LatentSync or MuseTalk 1.5 on CUDA.

Watch and listen to the final mastered MP4 before handoff. Verify lip-sync, age and cadence, spatial continuity, transitions and the audio bed at each cut. Record the master inspected, terminal job result, CU spent and released session.
