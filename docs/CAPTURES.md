# Terminal Captures

Every terminal capture in `docs/captures/` is generated from real loopback test output and rendered deterministically into SVG using `docs/tools/render-captures.py`.

## Capture Index

### 1. MCP Policy and Rule Gate (`mcp-policy`)

- **Command**: `cargo test --locked -p streamline-http` (filtered to `running`, `test `, and `test result:` lines)
- **Plain-text transcript**: [`docs/captures/mcp-policy.txt`](captures/mcp-policy.txt)
- **Rendered SVG**: [`docs/captures/mcp-policy.svg`](captures/mcp-policy.svg)
- **Text SHA-256**: `b6dd80edb6b68467cc5557f2fb10f4b8d29788a65c2ad3d48a7e24d036672a4c`
- **Scope**: Verifies `/ready`, `/downloads/streamline-skills.zip`, 3-pane Web UI rendering, MCP initialization and tool discovery, tag isolation, stdio MCP notifications, redaction blocking (`422 REDACTION_BLOCKED`), rule acknowledgement (`412 RULES_ACK_REQUIRED` -> `ack_rules`), the 3-part closeout gate, and conveyor trajectory export.

### 2. Trio+ Conveyor Closeout Gate (`conveyor-closeout`)

- **Command**: `cargo test --locked -p streamline-conveyor` (filtered to `running`, `test `, and `test result:` lines)
- **Plain-text transcript**: [`docs/captures/conveyor-closeout.txt`](captures/conveyor-closeout.txt)
- **Rendered SVG**: [`docs/captures/conveyor-closeout.svg`](captures/conveyor-closeout.svg)
- **Text SHA-256**: `78b2293e8660e9527cc7a8d6216efaec8245b66143bf4bf9757e77707bdb5ef2`
- **Scope**: Verifies cross-family acceptor enforcement (`R2` and above), read-only staging manifests, two-strike builder impasse escalation, dirty work-tree quarantine, scrubbed environment execution, redaction impasse protection, stray-artifact cleanup checks, and verified JSONL trajectory export.

## Regenerating Captures

Run the test suites and deterministic SVG renderer from the repository root:

```sh
cargo test --locked -p streamline-http | grep -E "^(running |test |test result:)" > docs/captures/mcp-policy.txt
cargo test --locked -p streamline-conveyor | grep -E "^(running |test |test result:)" > docs/captures/conveyor-closeout.txt
python3 -B docs/tools/render-captures.py
```

Running `docs/tools/render-captures.py` twice on identical `.txt` inputs produces byte-identical `.svg` files.
