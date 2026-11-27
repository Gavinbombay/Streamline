---
name: diffuser-caller
description: Use when calling Diffuser to discover eligible compute, submit or recover an idempotent job, verify its durable receipt, or perform an authorized fleet administration action.
---

# Call the Diffuser fleet

## Workflow: Discover an eligible capability

Use a scoped bearer over HTTPS or a trusted loopback tunnel. Call `GET /v1/fleet/discover` or MCP `fleet_discover` with `{}`. Select from the current `capabilities[]` for this actor and project. Offers can repeat. Use the adapter's documented input contract and the timeout on the selected offer.

For an exact device, copy the full UUID from that offer's `gpu_uuid` or `gpus[].uuid` into `pin_gpu_uuid`. Omit the field for automatic placement. Discovery reports effective free VRAM in `free_vram_bytes`. With admin inventory access, join `workers[].gpus_live[]` to enrolled devices by UUID. Read `memory_free_bytes`, `memory_total_bytes` and `util_pct` from a fresh heartbeat. Missing telemetry means unknown. A host total cannot establish that a single GPU has room. For REST jobs with a memory minimum, send `min_vram_bytes` to enforce it at admission.

On `422 capability_unavailable`, refresh discovery and check the capability, resources and exact pin. On `429 capacity_exhausted`, wait for capacity within the job's budget. Preserve the requested pin through retries.

## Submit once and recover by ID

Before sending, privately save the exact request body and a fresh random idempotency key for this logical job. Keep them across client restarts. Diffuser looks up replays by actor, project and key. Reusing a key for an edited request can return the earlier job.

For `POST /v1/fleet/jobs`, use JSON and the `Idempotency-Key` header. Omit `async` or set it to `false` to wait synchronously. Use `"async": true` for long work or a short client deadline. `timeout_secs` accepts 1 through 21600 seconds (6 hours). Omission uses the matched profile's budget. The worker also enforces its profile limit. A synchronous client's deadline must leave room for cleanup and the reply.

For this echo example, save the body as `job.json`. Substitute the capability and input your offer supports.

```json
{"capability":"example.echo","input":{"ok":true},"async":true,"timeout_secs":30}
```

Here `FLEET_ORIGIN` is the trusted controller origin, `FLEET_AUTH_HEADER` points to an owner-only file containing the Authorization header, and `JOB_KEY` is the saved key. Use a private run directory.

```sh
umask 077
curl -q --silent --show-error --fail-with-body --max-time 45 \
  --header "@$FLEET_AUTH_HEADER" --header 'Content-Type: application/json' \
  --header "Idempotency-Key: $JOB_KEY" --data-binary @job.json \
  "$FLEET_ORIGIN/v1/fleet/jobs" > submit.json
```

For MCP, call `fleet_submit_job` with the same body fields plus `idempotency_key` in `arguments`. If the reply is lost before you have an ID, repeat the saved request with the same key and credentials for the same scope. A replay can still say `running`. As soon as a `job_id` is known, read `GET /v1/fleet/jobs/{job_id}` or call `fleet_get_job` with `{"job_id":"<job ID>"}`. HTTP 200 alone does not establish completion. Poll while running, starting around 2 seconds and backing off to 10 seconds. A disconnected client can resume watching the same ID.

## Verify the receipt and find earlier jobs

On durable readback, match the job ID, capability, assigned worker and any exact pin. Require `state == "succeeded"`, expected output, a full 64-hex `output_sha256` and `capacity_released == true`. Compare with the terminal submit or replay digest when available. For an independent check, hash the controller's exact output serialization (`serde_json::to_vec` of the output value, UTF-8, no trailing newline). An echo of `{"ok":true}` has digest `4062edaf750fb8074e7e83e0c9028c94e32468a8b6f1614774328ef045150f93`. Verify downloaded artifact bytes against their separate artifact digest.

For `failed`, report the job ID and `error_code`. For `failed_uncertain` or unreleased capacity, have the operator reconcile the worker and any side effects before starting new compute. Preserve the durable receipt in either case.

Use `GET /v1/fleet/jobs?state=succeeded&capability=example.echo&limit=20` or MCP `fleet_list_jobs`. Both accept optional exact `state`, `capability` and `worker_id` filters in combination. URL-encode REST values. `limit` defaults to 50 and ranges from 1 to 200. Read `jobs[]` newest first. Scoped callers see their actor/project pair and admins see fleet-wide history. Use detail by ID for output. On a legacy controller returning 404/405 for the list, recover known IDs through detail.

## Connect a UI or MCP client

Open `/v1/fleet/ui`, connect with the scoped bearer in the Token control, then inspect discovery, compose a job in the playground, or open its receipt in Jobs. Keep the bearer out of URLs, clipboard examples and logs. Worker inventory is admin-only.

Configure an HTTP MCP client for authenticated JSON-RPC POST to `/v1/fleet/mcp`, or use a locally provisioned stdio bridge to that endpoint. Send `initialize`, then `notifications/initialized`, then `tools/list` to see the installed schemas. This call reads discovery.

```json
{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"fleet_discover","arguments":{}}}
```

Check HTTP status, the RPC `error` and `result.isError` before parsing a successful tool's `result.content[0].text` as JSON.

## Admin worker and token lifecycle

For an authorized change, identify the worker with `GET /v1/fleet/workers` or `fleet_list_workers`. On an upgraded controller, `POST /v1/fleet/workers/{id}/drain` with JSON `{"accepting":false}` to drain or `{"accepting":true}` to resume. MCP `fleet_drain_worker` takes `worker_id` plus the same boolean. Drain closes admission while in-flight work finishes. Reread workers and discovery after the change or a lost reply. Resume requires a fresh heartbeat for admission. A `409 worker_quarantined` needs operator recovery.

On mixed versions, check that the colocated MCP tool advertises the boolean `accepting` argument before using resume. Older handlers drain on any body. Separately, to revoke an issued caller credential, call admin `fleet_revoke_token` with its `token_id`. Verify the returned ID and `revoked:true`. The credential's next authenticated request should receive 401.

## Multimodal Capabilities & One-Card-One-Service Lifecycle (Diffuser v2)

Local GPU workers (`echo-rtx3060-01`, `warp-rtx2070-01`, `a15-rtx4060-01`) enforce **One Card, One Service**: a GPU card never runs two conflicting GPU services at the same time.

### Turning Off / Parking a GPU Card

Before switching workloads or when you want 100% free VRAM on a card, call `gpu.service.control/v1` pinned to that card's `gpu_uuid`:

- **Check active service & live VRAM**: `{"action": "status"}`
- **Turn off / park card** (stops resident `llama-server` and unloads resident Ollama models until VRAM is free): `{"action": "park"}` (aliases: `"stop_llm"`, `"turn_off"`)
- **Start tuned LLM on demand**: `{"action": "start_llm"}`

All multimodal GPU adapters (`gen.image.sdxl/v1`, `audio.stt.whisper/v1`, `audio.tts.kokoro/v1`, `gen.music.musicgen/v1`, `render.blender.exec/v1`, `infer.llm.chat/v1`) also hold a per-card exclusive slot lock and automatically park conflicting resident services before running (and `infer.llm.chat/v1` auto-wakes the tuned `llama-server` if parked).

### Multimodal Capability Catalog

| Capability | Modality | Nodes | Input Contract |
|---|---|---|---|
| `infer.llm.chat/v1` | LLM Chat | Echo (Qwen3-8B), Warp (Qwen3-4B) | OpenAI `/v1/chat/completions` JSON (`model`, `messages`, `max_tokens`) |
| `gen.image.sdxl/v1` | Image Gen | Echo (3060), Warp (2070) | `{"prompt": "...", "width": 512, "height": 512, "steps": 4}` |
| `audio.stt.whisper/v1` | Voice STT | Echo, Warp, A15, NUC CPU (last) | `{"audio_b64": "...", "model": "small", "language": "en"}` |
| `audio.tts.kokoro/v1` | Voice TTS | Echo, Warp, A15, NUC CPU (last) | `{"text": "...", "voice": "af_heart", "speed": 1.0}` |
| `gen.music.musicgen/v1` | Music Gen | Echo (3060), Warp (2070) | `{"prompt": "...", "duration_secs": 5}` |
| `render.blender.exec/v1` | 3D Render | Echo (3060), Warp (2070) | Headless Blender 4.5.16 LTS job JSON |
| `media.nvenc.transcode/v1` | NVENC Video | A15 (RTX 4060 Ada) | `{"input_b64": "...", "codec": "h264_nvenc"}` |
| `media.ffmpeg.exec/v1` | Media CPU | NUC CPU (`cpu_fallback`) | `{"action": "version"|"loudnorm"|"to_mp3"|"to_wav_16k", "input_b64": "..."}` |
| `gpu.service.control/v1` | Card Lifecycle | Echo, Warp, A15 | `{"action": "status"|"park"|"start_llm"|"stop_llm"}` |

### Strict CPU-Last Routing (`worker_tier`)

Diffuser classifies every enrolled worker into `worker_tier`: `local_gpu` (rank 0) > `cloud_gpu` (rank 1) > `cpu_fallback` (rank 2). For shared capabilities like `audio.stt.whisper/v1` and `audio.tts.kokoro/v1`, local GPUs are always scheduled first. `nuc-cpu-01` (`cpu_fallback`) is only selected automatically when all matching GPU workers are drained, offline, or out of capacity.
