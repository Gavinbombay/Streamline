---
name: diffuser-onboard
description: Use when enrolling a CPU, GPU or service as a Diffuser worker, connecting its adapter over a private path, and proving routing, telemetry and drain/resume with a durable smoke receipt.
---

# Onboard a Diffuser worker

## Inspect the target and choose a profile

Record a unique worker ID, an operator-chosen host label, usable CPU in millicores (1000 per core) and usable RAM in bytes. Reserve headroom for the host. On Linux, `nproc` and `free -b` give a starting point. On a host with NVIDIA devices, inspect inventory and live telemetry under the worker service account.

```sh
nvidia-smi --query-gpu=uuid,name,memory.free,memory.total,utilization.gpu --format=csv
```

Match devices by full UUID. The CLI reports memory in MiB and utilization in percent. The worker publishes bytes. Enable `auto_detect_gpus` for supported local GPUs. For CPU-only operation set it to `false` with `vram_bytes: 0`. For other accelerators or cloud services, advertise the adapter's actual capability and verify placement in its output.

## Provision the worker locally

Create `fleet-worker.json` owned by the service account, mode `0600`, under a private directory (mode `0700`). Set `umask 077` before creating it. Render the enrollment admin bearer from the deployment's secret store and generate a fresh per-worker 32-byte random value encoded as 64 hex characters for `rpc_secret_hex`. Replace the two markers locally before launch. Keep the resulting file outside the repository and out of transcripts.

This POSIX echo profile gives a deterministic, CPU-only smoke target. Set a unique capability for the enrollment check so another worker cannot satisfy it. Adjust the sample capacities, ports and executable to the inspected host.

```json
{
  "worker_id": "worker-example",
  "host": "worker-example",
  "bind": "127.0.0.1:5835",
  "advertise_url": "http://127.0.0.1:15835",
  "controller_url": "http://127.0.0.1:15820",
  "enroll_bearer": "<provision on worker>",
  "rpc_secret_hex": "<generate on worker>",
  "cpu_millicores": 1000,
  "ram_bytes": 536870912,
  "vram_bytes": 0,
  "auto_detect_gpus": false,
  "profiles": [
    {"capability":"example.echo","adapter":{"kind":"exec","command":["/bin/cat"],"timeout_secs":30}}
  ]
}
```

- **Exec.** Supply a fixed argv array. The process receives the job's `input` as JSON on stdin. Emit one bounded JSON value on stdout and exit zero on success. The worker supplies `DIFFUSER_JOB_ID`, `DIFFUSER_CAPABILITY`, and the time budget in `DIFFUSER_TIMEOUT_SECS`. For a selected GPU it also sets `CUDA_VISIBLE_DEVICES` and `DIFFUSER_GPU_UUID`. Put an optional profile `gpu_uuid` inside `adapter`.
- **HTTP.** Diffuser sends the job input as a JSON body. Point `bearer_file` at an owner-only file containing the upstream service's raw token. Verify it is readable and nonempty as the worker user. An unreadable or empty file can cause an unauthenticated request. Omit the field for a service that needs no bearer. For a GPU-backed service, verify actual placement in the service. Use a worker exposing only devices the service can honor for exact pins.

```json
{"capability":"example.transform","adapter":{"kind":"http","url":"http://127.0.0.1:8080/transform","method":"POST","timeout_secs":60,"bearer_file":"/run/secrets/adapter-bearer"}}
```

## Connect both directions over SSH

For a remote worker, run this tunnel **from the worker to the controller** using a verified SSH alias. Keep the tunnel supervised. Pick a free controller-side RPC port for each worker.

```sh
ssh -nNT -o BatchMode=yes -o StrictHostKeyChecking=yes -o ExitOnForwardFailure=yes \
  -o ServerAliveInterval=30 -o ServerAliveCountMax=3 \
  -L 127.0.0.1:15820:127.0.0.1:5820 \
  -R 127.0.0.1:15835:127.0.0.1:5835 "$CONTROLLER_SSH_ALIAS"
```

The worker reaches `controller_url` on its local port 15820 for enrollment and heartbeats. Diffuser reaches `advertise_url` on its local port 15835 for signed RPC to the worker's `bind` port 5835. Ensure SSH keeps both listeners on loopback. On a single host use direct loopback ports for the same two paths.

Start the installed Diffuser executable with `"$DIFFUSER_BIN" --fleet-worker "$WORKER_CONFIG"`, where `WORKER_CONFIG` is the absolute config path. Confirm a fresh heartbeat, then exercise the reverse path with a real smoke job.

## Admit intermittently available workers

- **Shared or gaming laptop.** Have the idle guard drain on a foreground game or foreign GPU activity. Exclude the worker's own jobs when interpreting utilization. Let running jobs finish before stopping the worker. Require continuous idle through a cooldown and healthy telemetry before resuming a guard-owned drain. Preserve operator maintenance holds. An older guard that only restarts or re-enrolls the worker needs an explicit resume step on controllers with persistent drain.
- **Ephemeral cloud or Colab adapter.** Start with the adapter's status/health action on the provisioning host. Keep provider credentials there. Advertise a cloud execution capability with its own slot, spend and wall-time bounds. Allocate only within the authorized budget. Check the actually allocated device and reserve time to collect outputs, stop the owned session and verify its absence. Keep the lane parked or quarantined if termination is uncertain.

## Prove admission, execution and release

1. With an admin bearer, read `GET /v1/fleet/workers`. Find the exact ID, expected capability, capacities, `alive:true` and `accepting:true`. Check the next heartbeat advances. For a telemetry-enabled GPU worker, match `gpus_live[].uuid` to inventory and inspect `memory_free_bytes`, `memory_total_bytes` and `util_pct`. Missing or stale readings are unknown. Open `/v1/fleet/ui`, connect through the Token control and cross-check that worker's topology card. Confirm `GET /v1/fleet/discover` offers the capability to the intended scoped caller.
2. As that caller, send the actual profile a low-cost, known input at `POST /v1/fleet/jobs` with a saved `Idempotency-Key`. For the echo profile use `{"capability":"example.echo","input":{"ok":true},"async":true}`. For a GPU profile add the discovered `pin_gpu_uuid`. Read `GET /v1/fleet/jobs/{job_id}` through completion. Check the worker ID, any pin, expected output, `succeeded` and `capacity_released:true`. For the echo, `output_sha256` must equal `4062edaf750fb8074e7e83e0c9028c94e32468a8b6f1614774328ef045150f93`. Replay the exact request and key once. Require the original job ID and `idempotent_replay:true`.
3. Verify the controller supports resume via MCP `tools/list` (`fleet_drain_worker` advertises boolean `accepting`). For the new, healthy worker only, admin POST `{"accepting":false}` to `/v1/fleet/workers/{id}/drain`. Reread workers and discovery for `accepting:false` and withdrawal of its unique offer. POST `{"accepting":true}` to the same path and verify a fresh heartbeat, `accepting:true` and renewed discovery. A legacy controller can interpret a resume body as another drain.
4. Record the worker ID, capability, job ID, assigned GPU if any, digest, release flag and final admission state. If the smoke receipt or telemetry required for the profile fails validation, keep the new worker drained and diagnose the failing direction or adapter. A quarantined resume requires cleanup review before return to service.

## Enforce One-Card-One-Service on GPU Workers

Every local GPU worker must prevent two GPU services from competing for VRAM on the same card:

1. Wrap GPU exec adapters with the node's exclusive slot manager (`gpu-slotctl.py` on Linux nodes or `GPU_SLOT_LOCK` on Windows A15).
2. Expose `gpu.service.control/v1` supporting `status`, `park` (cleanly stops resident LLM services and unloads Ollama models until VRAM is free), and `start_llm` (starts and health-checks the tuned LLM).
3. Wrap `infer.llm.chat/v1` with auto-wake (`run-llm-chat.py`) so an LLM request after an image, voice, or music job cleanly restarts the tuned `llama-server` without returning 502.
4. Register CPU-only fallback workers with `vram_bytes: 0` so Diffuser assigns `worker_tier: "cpu_fallback"` and routes to them strictly last.
