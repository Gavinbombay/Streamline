# Terminal Captures

Every terminal capture in `docs/captures/` is generated from real loopback test output and rendered deterministically into SVG using `docs/tools/render-captures.py`.

## Capture Index

### 1. MCP Policy and Rule Gate (`mcp-policy`)

- **Command**: `cargo test --locked -p streamline-http` (filtered to `running`, `test `, and `test result:` lines)
- **Plain-text transcript**: [`docs/captures/mcp-policy.txt`](captures/mcp-policy.txt)
- **Rendered SVG**: [`docs/captures/mcp-policy.svg`](captures/mcp-policy.svg)
- **Text SHA-256**: `cf04c5eb138a90c142a972d7237cd1519cd51f64192747620243753b2904a777`
- **Scope**: Verifies `/ready`, `/downloads/streamline-skills.zip`, 3-pane Web UI rendering, MCP initialization and tool discovery, tag isolation, stdio MCP notifications, redaction blocking (`422 REDACTION_BLOCKED`), rule acknowledgement (`412 RULES_ACK_REQUIRED` -> `ack_rules`), the 3-part closeout gate, and conveyor trajectory export.

### 2. Trio+ Conveyor Closeout Gate (`conveyor-closeout`)

- **Command**: `cargo test --locked -p streamline-conveyor` (filtered to `running`, `test `, and `test result:` lines)
- **Plain-text transcript**: [`docs/captures/conveyor-closeout.txt`](captures/conveyor-closeout.txt)
- **Rendered SVG**: [`docs/captures/conveyor-closeout.svg`](captures/conveyor-closeout.svg)
- **Text SHA-256**: `7aea89ffdb6cce96ab96208be2b9fab82b5993f200f8cd9bf19f9aef2c5d6b70`
- **Scope**: Verifies cross-family acceptor enforcement (`R2` and above), read-only staging manifests, two-strike builder impasse escalation, dirty work-tree quarantine, scrubbed environment execution, redaction impasse protection, stray-artifact cleanup checks, and verified JSONL trajectory export.

## Regenerating Captures

Run the test suites and deterministic SVG renderer from the repository root:

```sh
cargo test --locked -p streamline-http | grep -E "^(running |test |test result:)" > docs/captures/mcp-policy.txt
cargo test --locked -p streamline-conveyor | grep -E "^(running |test |test result:)" > docs/captures/conveyor-closeout.txt
python3 -B docs/tools/render-captures.py
```

Running `docs/tools/render-captures.py` twice on identical `.txt` inputs produces byte-identical `.svg` files.
