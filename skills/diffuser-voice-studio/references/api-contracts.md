# Diffuser Voice Studio API Contracts Reference

Base URLs:
- On NUC (loopback only): `http://127.0.0.1:5885`
- External / TLS (Beacon): `https://voice.thaladex.app`
- Live OpenAPI docs: `http://127.0.0.1:5885/openapi.json` and `/docs`

## 1. OpenAI-Compatible Drop-In Speech

List voices:
```bash
curl -sS http://127.0.0.1:5885/v1/audio/voices
```

Synthesize WAV:
```bash
curl -sS -X POST http://127.0.0.1:5885/v1/audio/speech \
  -H "Content-Type: application/json" \
  -d '{
    "model": "voice.tts.qwen/v1",
    "input": "We do not have Snickers, Dad.",
    "voice": "kit_v1",
    "response_format": "wav",
    "speed": 1.08,
    "instructions": "Light, higher-pitched 7-year-old child, dry comedic timing."
  }' -o /tmp/kit_line.wav
```

## 2. Multi-Take Prosody Audition with QC

```bash
curl -sS -X POST http://127.0.0.1:5885/api/v1/prosody/takes \
  -H "Content-Type: application/json" \
  -d '{
    "text": "Here is the simple list. Green first, okay?",
    "voice_profile_id": "mom_v1",
    "preset": "sitcom_warm_exasperated",
    "num_takes": 3,
    "base_seed": 4201,
    "line_id": "scene01_mom_02"
  }'
```

Key QC fields returned per take: `duration_secs`, `wpm`, `peak_dbfs`, `rms_dbfs`, `clipped`, `leading_silence_ms`, `trailing_silence_ms`, `flags[]`, `qc_status` (`pass`, `warn`, `fail`).

## 3. Local HQ Music & Local SFX Generation

Generate HQ local music (`backend: "auto"` routes to `voice.music.hq/v1` ACE-Step on Echo, falling back to `voice.music.musicgen/v1`; never calls ElevenLabs unless explicitly requested):
```bash
curl -sS -X POST http://127.0.0.1:5885/api/v1/music/generate \
  -H "Content-Type: application/json" \
  -d '{
    "prompt": "upbeat 90s sitcom funk-rock instrumental groove, slap bass, crisp guitar, no vocals",
    "backend": "auto",
    "duration_secs": 35.0,
    "seed": 42
  }'
```

Generate local SFX (`backend: "auto"` routes to `voice.sfx.local/v1` TangoFlux on Warp; never calls ElevenLabs unless explicitly requested):
```bash
curl -sS -X POST http://127.0.0.1:5885/api/v1/sfx/generate \
  -H "Content-Type: application/json" \
  -d '{
    "prompt": "frantic knocking on a hollow wooden bathroom door",
    "backend": "auto",
    "duration_secs": 3.5,
    "seed": 42
  }'
```

Both endpoints return `clip_id`, `audio_url`, `backend`, `duration_secs`, and `sha256`.

## 4. Voice Registry & Cloning Reference

Create or patch a profile (`POST /api/v1/registry/voices` or `PATCH /api/v1/registry/voices/{id}`):
- Required on create: `voice_profile_id`, `character_name`.
- Key fields: `default_engine` (`voice.tts.qwen/v1`), `mode` (`voice_design`, `clone`, `custom_voice`), `model_id`, `voice_description`, `timbre_notes`, `default_prosody`.

Promote an approved clip into a cloning reference:
```bash
curl -sS -X POST http://127.0.0.1:5885/api/v1/eval/promote \
  -H "Content-Type: application/json" \
  -d '{
    "clip_id": "take_id_here",
    "voice_profile_id": "kit_v1",
    "emotion_tag": "snappy"
  }'
```

## 5. Auto-Ducked Spatial Radio Play Mix

```bash
curl -sS -X POST http://127.0.0.1:5885/api/v1/mix/render \
  -H "Content-Type: application/json" \
  -d '{
    "project_id": "scene_01_kitchen",
    "sample_rate": 48000,
    "comedy_safe_leveling": true,
    "auto_duck_music": true,
    "duck_db": 9.0,
    "tracks": [
      {
        "track_id": "music_bed",
        "kind": "music",
        "clip_path": "music_hq_clip_id",
        "start_ms": 0,
        "gain_db": -7.0,
        "fade_in_ms": 800,
        "fade_out_ms": 2000,
        "spatial_preset": "dry"
      },
      {
        "track_id": "t1",
        "kind": "dialogue",
        "clip_path": "dad_take_clip_id",
        "start_ms": 1500,
        "gain_db": 0.0,
        "pan": -0.15,
        "spatial_preset": "kitchen_room_tone",
        "shot_id": "shot_01",
        "speaker": "dad_v1"
      },
      {
        "track_id": "sfx1",
        "kind": "sfx",
        "clip_path": "sfx_loc_clip_id",
        "start_ms": 4200,
        "gain_db": -3.0,
        "pan": 0.2,
        "spatial_preset": "tiled_bathroom_slap",
        "fade_in_ms": 30,
        "fade_out_ms": 200
      }
    ]
  }'
```

Returns `master_wav_clip_id`, `master_mp3_clip_id`, `audio_url`, `wav_audio_url`, `duration_secs`, `auto_duck_music_applied`, `duck_db`, and `shot_timing_manifest[]`.

## 6. Direct Diffuser Job Submission (`voice.tts.qwen/v1`)

Read the bearer token in memory from `/etc/diffuser/fleet-worker-nuc.json` (`enroll_bearer`) and never print it. Submit to `http://127.0.0.1:5820/v1/fleet/jobs` with header `Idempotency-Key: <unique-key>`:
```json
{
  "capability": "voice.tts.qwen/v1",
  "async": false,
  "timeout_secs": 300,
  "input": {
    "text": "Please tell me that is a Snickers!",
    "mode": "voice_design",
    "voice_description": "Expressive late-30s American sitcom dad, warm baritone.",
    "instruct": "Escalating comedic panic, crisp syllables.",
    "speed": 1.1,
    "seed": 1001
  }
}
```
