---
name: diffuser-voice-studio
description: Generate expressive character dialogue, cast or clone voices, audition multi-take prosody with QC, generate local HQ music and SFX, and render auto-ducked spatial Radio Play masters via Diffuser Voice Studio instead of ElevenLabs. Use when producing speech, character lines, narration, music beds, SFX, or timed scene audio for Imagination Studio, films, or games. Do not use for non-audio Diffuser jobs or Colab video rendering.
---

# Diffuser Voice Studio

Use Diffuser Voice Studio (`http://127.0.0.1:5885` on NUC, `https://voice.thaladex.app` via Beacon TLS) as the default AI speech, local music/SFX, and scene-audio engine instead of ElevenLabs.

## Engine selection and hard boundaries

- **Default Speech (`voice.tts.qwen/v1`)**: Qwen3-TTS 1.7B on Echo (RTX 3060 12 GB). Use for expressive dialogue, voice design, and cloning.
- **Fast Fallback (`voice.tts.kokoro/v1`)**: Use for quick scratch reads or when Echo is busy or draining (v1.1.0 contract shim alongside legacy `audio.tts.kokoro/v1`).
- **Local HQ Music (`voice.music.hq/v1`)**: ACE-Step 1.5 (`ACE-Step/Ace-Step1.5`: `acestep-v15-turbo` + `acestep-5Hz-lm-1.7B` CoT song planner, MIT, ungated) on Echo (RTX 3060 12 GB, `offload_to_cpu=True`), 48 kHz stereo WAV. Supports both clean instrumental scores (`instrumental: true`, zero phantom vocals) and full sung vocal songs (`instrumental: false`, structured `[verse]`/`[chorus]` lyrics). Default for `POST /api/v1/music/generate` (`backend="auto"`), with `voice.music.musicgen/v1` (`facebook/musicgen-stereo-medium`, 32 kHz true stereo) as a vocal-free A/B peer engine.
- **Local SFX (`voice.sfx.local/v1`)**: TangoFlux on Warp (RTX 2070 8 GB), 44.1 kHz stereo WAV. Default for `POST /api/v1/sfx/generate` (`backend="auto"`).
- **ElevenLabs boundary**: Never call `voice.tts.elevenlabs/v1`, `voice.music.elevenlabs/v1`, `voice.sfx.elevenlabs/v1`, or the external ElevenLabs API unless the user explicitly asks for ElevenLabs or a blind A/B comparison against it.
- **Scope and security**: Route non-audio fleet jobs through `diffuser-caller` and Colab G4 video renders through `cinema-video-production`. Never expose raw LAN ports, print bearer tokens or API keys, or emit >64 KB base64 audio to stdout.

## Workflow: Choose the right API surface

### 1. OpenAI-compatible drop-in (raw audio bytes)

Use when an agent or tool replaces ElevenLabs or OpenAI TTS with minimal changes:
- Discover versioned profiles: `GET /v1/audio/voices` (pre-seeded: `dad_v1`, `mom_v1`, `kit_v1`, `pip_v1`).
- Synthesize WAV or MP3 stream: `POST /v1/audio/speech` with `{"model":"voice.tts.qwen/v1","input":"Line text","voice":"kit_v1","response_format":"wav","speed":1.08,"instructions":"Light, bright 7-year-old child, crisp snappy deadpan."}`.

### 2. Expressive Studio API (casting, multi-take QC, music/SFX, Radio Play)

Use this sequence for scene dialogue, Imagination Studio, or film audio:
1. **Verify health and engines**: `GET /api/v1/health` must return `ok: true` and `ffmpeg_available: true`; `GET /api/v1/engines` confirms `voice.tts.qwen/v1`, `voice.music.hq/v1`, and `voice.sfx.local/v1` are `available: true`.
2. **Cast or update voice profile**: `GET/POST /api/v1/registry/voices` or `PATCH /api/v1/registry/voices/{voice_profile_id}` (also aliased at `/api/v1/voices`).
3. **Generate takes with QC**:
   - Single take with JSON metadata + `sha256`: `POST /api/v1/tts/generate`.
   - Multi-take audition (recommended for hero lines and punchlines): `POST /api/v1/prosody/takes` with `num_takes` (2 to 4), `voice_profile_id`, and a `preset` from `GET /api/v1/prosody/presets`:
     - `sitcom_deadpan_snappy` (speed 1.08; fixes sluggish/flat delivery)
     - `sitcom_kid_bright` (speed 1.08; fixes too-deep/sluggish 7-year-old timbre)
     - `sitcom_pip_mischievous` (speed 1.12; high-pitched giggly 4-year-old toddler energy)
     - `sitcom_warm_exasperated` (speed 1.02; fixes robotic mom tone)
     - `sitcom_panic_shout` (speed 1.12; high-energy panic without clipping)
     - `sitcom_conspiratorial_whisper` (speed 0.96; hushed close-mic aside)
     - `custom` (manual `speed`, `emotion_tags`, `instruct`)
   - Standalone QC on an existing clip: `POST /api/v1/prosody/qc`.
4. **Generate local Music and SFX**:
   - Clean instrumental score: `POST /api/v1/music/generate` with `{"prompt":"upbeat 90s TV sitcom instrumental underscore, warm Rhodes electric piano, playful slap bass, light acoustic drums","backend":"auto","instrumental":true,"bpm":112,"keyscale":"C Major","duration_secs":30.0,"steps":8,"seed":4201}` (routes to `voice.music.hq/v1`; never include negation phrases like `"no vocals"` in positive prompts; use `backend="musicgen"` for `facebook/musicgen-stereo-medium` A/B comparison).
   - Full sung vocal song: `POST /api/v1/music/generate` with `{"prompt":"upbeat 90s animated sitcom dad-rock theme song, energetic warm male lead vocals, punchy drums, bright rhythm guitar","instrumental":false,"vocal_language":"en","bpm":124,"keyscale":"A Major","lyrics":"[verse]\n...\n[chorus]\n...","duration_secs":40.0,"steps":8,"seed":4202}` (keep 8 to 10 short lines for 35 to 45s tracks; no stage directions inside lyrics; note that >30s synchronous calls should target `http://127.0.0.1:5885` on NUC to avoid Beacon 30s proxy timeout).
   - Foley/SFX cues: `POST /api/v1/sfx/generate` with `{"prompt":"...","backend":"auto","duration_secs":3.5,"seed":42}` (routes to `voice.sfx.local/v1`).
5. **Rate, promote, or export**: Record ratings via `POST /api/v1/eval/rate`, promote approved takes into character cloning references via `POST /api/v1/eval/promote`, or export filtered good clips via `POST /api/v1/eval/export-lora`.
6. **Render Radio Play master (mandatory pre-video gate)**: Assemble approved dialogue stems + local music + spatial SFX via `POST /api/v1/mix/render` (`auto_duck_music: true`, `duck_db: 9.0`, `comedy_safe_leveling: true`) to produce a true `-1.0 dBFS` limited stereo 48 kHz WAV + 192k MP3 master and `shot_timing_manifest[]` before spending Colab G4 video CU.

### 3. Direct Diffuser dispatch (bypassing Studio)

Submit to `POST http://127.0.0.1:5820/v1/fleet/jobs` with a fresh `Idempotency-Key` header, `capability: "voice.tts.qwen/v1"`, and the bearer read in memory from `/etc/diffuser/fleet-worker-nuc.json`. Echo writes durable WAVs to `$HOME/diffuser-cuda/artifacts/<job_id>.wav`; pull via `scp` and verify `sha256` before use.

See `references/api-contracts.md` for full payload schemas and `curl` examples.

## Voice design, cloning, and failure-mode rules

- **Voice Design (`mode: "voice_design"`, `Qwen3-TTS-12Hz-1.7B-VoiceDesign`)**: Default for casting and iterating character pitch, age, and cadence via `voice_description` and `instruct`.
- **Voice Cloning (`mode: "clone"`, `Qwen3-TTS-12Hz-1.7B-Base`)**:
  - Normalize references to **24 kHz mono PCM16 WAV** with **50 to 100 ms** breath padding.
  - Target **10 to 15 s** duration (`3 s` minimum, `30 s` maximum) and supply exact verbatim `ref_text` (no speaker labels like `Dad:` or `Pip:`).
  - Never apply aggressive AI denoisers to reference clips (phase artifacts clone into output).
  - Use per-emotion sub-references (`emotion_tag`: `neutral`, `panic`, `whisper`, `snappy`, `warm`, `deadpan`) for emote-by-example cloning.
- **Prevent recurring defects**:
  - *Too deep child voice*: specify `"light, higher-pitched bright 7-year-old child voice, never deep or adult-sounding"` and use `sitcom_kid_bright` (or `sitcom_pip_mischievous` for toddler Pip).
  - *Sluggish pacing*: set `speed` to `1.04` to `1.12` and reject takes flagged `sluggish_pacing` (`wpm < 90`).
  - *Robotic delivery*: audition 2 to 4 takes with explicit emphasis cues in `instruct`; never ship take 1 unreviewed.

## Spatial acoustics, automatic music ducking, and Radio Play gating

Apply per-track `spatial_preset` in `POST /api/v1/mix/render` (`GET /api/v1/mix/presets`):
- `dry`: 75 Hz high-pass clean dialogue
- `kitchen_room_tone`: warm domestic presence with prime-ratio early reflections (`23|37` ms)
- `tiled_bathroom_slap`: bright reflective slap-reverb with prime-ratio taps (`29|47|73` ms) and gentle HF damping (prevents comb-filter ringing)
- `phone_speaker_bandpass`: 320 Hz to 3.2 kHz bandpass + light compression
- `behind_locked_door_muffled`: intelligible muffle (150 Hz high-pass, soft 2.1 kHz and 3.6 kHz low-pass, +3 dB at 900 Hz, -3 dB at 2.6 kHz, gentle 2.5:1 compression, faint prime-ratio `17|31` ms reflection). The old 550 Hz low-pass removed the 1 to 3 kHz speech band by about 20 dB; the polished preset keeps it within about 6 dB so words stay followable while 5 kHz and above stays about 15 dB down

Keep `auto_duck_music: true` (`duck_db: 9.0` default) so the mixer builds a dedicated dialogue sidechain (`apad`-matched `sidechaincompress`, 35 ms attack, 320 ms release) that ducks music tracks by ~8 to 10 dB under speech and swells cleanly during gaps and outros. Use `fade_in_ms` and `fade_out_ms` (smooth triangular curves) to drop music cleanly before dead-silence punchlines, and keep `comedy_safe_leveling: true` (static gain + true `-1.0 dBFS` limiter at stereo 48 kHz, no `loudnorm` pumping). Audition and lock the Radio Play master before renting cloud video GPUs.

### Round 3 polish: music bed trim, timeline tips, and music/SFX dispatch

- **Level-aware music bed**: when music and dialogue coexist and `auto_duck_music` applies, the mixer trims the music bus by at least 12 dB (up to 30 dB for hot music) over merged dialogue windows (0.25 s pre-roll, 0.35 s post-roll, 0.35 s ramps, lines under 2 s apart share one bed), on top of the sidechain duck. Intros and outros stay at unity. Optional request fields: `music_bed_db` (fixed trim override, 0 to 30) and `bed_gap_db` (target gap between dialogue RMS and bed RMS, default 18). Response adds `music_bed_trim_db`, `music_bed_spans`, `dialogue_rms_db`, `music_rms_db`.
- **Timeline shape that works**: give a 4 to 5 s music-only intro before the first line, end each music cue with a 3 to 4 s `fade_out_ms`, restart a second music cue at the final act so the outro swells after the last line, and play musical stings as `kind: "sfx"` so they are never ducked. The master is limited for peak safety, not loudness (about -24 LUFS integrated with a -3.5 dBFS peak on the Round 3 master); apply a pure gain stage only if a louder listening copy is wanted.
- **Stable clip references**: clip ids registered in memory are lost when `diffuser-voice-studio.service` restarts (the mix returns 404 `Track clip not found`). Pass absolute `artifact_path` values under `/srv/voice-studio/data/artifacts/` as `clip_path` for long-lived timelines, and keep `artifact_path` in every saved result JSON.
- **Music dispatch**: `POST /api/v1/music/generate` accepts `instrumental`, `lyrics` (`"[Instrumental]"` for scores), `vocal_language` (`"unknown"` for scores, `"en"` for songs), `bpm`, `keyscale`, `style_tags`, `steps`, `seed`. A synchronous 30 to 45 s ACE-Step 1.5 render takes about 60 s, which exceeds the Beacon upstream timeout of roughly 30 s, so call it on `http://127.0.0.1:5885` from the NUC and stream the result over `https://voice.thaladex.app/api/v1/audio/<clip_id>`. The minimum ACE-Step output is about 5 s, so a requested 4 s sting returns about 5.1 s; start it early and use a 1.5 s `fade_out_ms`.
- **SFX dispatch**: TangoFlux on Warp runs in fp32 (fp16 corrupts the flan-t5 prompt embedding on Turing). Use one concrete foley event per prompt, 2 to 4 s, never musical words. Always check `peak_dbfs` and `duration_secs` in the response: a prompt can return a near-silent 0.26 s clip (peak about -73 dBFS); regenerate with a new seed or reworded prompt. Beacon can drop back-to-back POSTs with "Upstream request failed"; retry once.
- **Whisper QA gotcha**: run faster-whisper on Echo CPU (`$HOME/diffuser-audio-venv`, `small`, `int8`, 16 kHz mono numpy input, never GPU so VRAM stays idle). Music-only clips can hallucinate phrases such as "Music" or "subscribe"; treat a single stock phrase on music as non-speech and re-check the isolated clip.

## Verification, stop conditions, and troubleshooting

Verify every handoff against these checks:
1. **Take QC**: Require `qc_status == "pass"` (or inspected `warn`), `clipped == false`, `peak_dbfs <= -0.3`, `rms_dbfs >= -38.0`, and `90 <= wpm <= 260`. Hard reject `fail` or `empty_or_too_short` (`< 0.15 s`).
2. **Bounded retry stop condition**: If all takes in a batch fail QC or flag `sluggish_pacing`, adjust `preset`/`speed`/`instruct` once and rerun with a new `base_seed`. If the second batch still fails, stop retrying, report the `flags[]`, and either fall back to `voice.tts.kokoro/v1` or request direction.
3. **Artifact integrity**: Confirm the WAV exists under `/srv/voice-studio/data/artifacts/`, matches returned `sha256`, and streams via `GET /api/v1/audio/{clip_id}` (`200` or `206 Partial Content`).
4. **GPU VRAM slot lock & idle check**: `voice.tts.qwen/v1` and `voice.music.hq/v1` on Echo and `voice.sfx.local/v1` on Warp acquire `/tmp/diffuser-gpu-slot.lock` via `gpu-slotctl.py` and release VRAM on exit (`< 500 MiB`, `0%` util). On `429 capacity_exhausted` or timeout, check `GET /v1/fleet/workers` for active GPU jobs or fall back to `voice.tts.kokoro/v1`.
5. **Remote artifact sync**: If `scp` pull fails, verify `ssh -n echo true` (or `ssh -n warp true`) and confirm the artifact exists under `$HOME/diffuser-cuda/artifacts/` before retrying once.
