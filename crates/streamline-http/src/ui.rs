use axum::extract::{Query, State};
use axum::response::Html;
use serde::Deserialize;

use crate::state::AppState;

#[derive(Debug, Deserialize, Default)]
pub struct UiQueryParams {
    pub panel: Option<String>,
}

fn escape_html(raw: &str) -> String {
    raw.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn normalize_panel(panel: Option<&str>) -> &'static str {
    match panel.unwrap_or("read").trim().to_ascii_lowercase().as_str() {
        "history" => "history",
        "related" => "related",
        "passport" => "passport",
        _ => "read",
    }
}

/// Serves the Streamline dark-palette 3-pane server status and workspace shell.
pub async fn index_handler(
    State(state): State<AppState>,
    Query(query): Query<UiQueryParams>,
) -> Html<String> {
    let version = env!("CARGO_PKG_VERSION");
    let rules_sha = state
        .store
        .current_rules_sha256()
        .unwrap_or_else(|_| "unavailable".to_string());
    let short_sha: String = rules_sha.chars().take(12).collect();
    let safe_mode_label = if state.config.public_safe_mode {
        "Public-Safe Mode"
    } else {
        "Private Local Mode"
    };
    let active_panel = normalize_panel(query.panel.as_deref());
    let has_presence_plugin = state.config.presence_plugin_path().is_some();
    let presence_script_tag = if has_presence_plugin {
        r#"<script type="module" src="/_streamline/presence-plugin/presence.js"></script>"#
    } else {
        ""
    };

    let html = format!(
        r##"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Streamline Governance and Trio+ Workspace</title>
  <style>
    :root {{
      --background: #050817;
      --foreground: #edf5ff;
      --card: #101a35;
      --popover: #121d39;
      --primary: #3969f8;
      --primary-foreground: #ffffff;
      --secondary: #172646;
      --muted: #15223d;
      --muted-foreground: #b4c5e0;
      --accent: #1a2d52;
      --border: #3f577c;
      --ring: #74d8ff;
      --cyan: #74d8ff;
      --green: #94ead1;
      --text: #edf5ff;
      --dim: #b4c5e0;
      --radius: 18px;
    }}

    *, *::before, *::after {{
      box-sizing: border-box;
    }}

    html, body {{
      margin: 0;
      padding: 0;
      background-color: var(--background);
      color: var(--foreground);
      font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
      font-size: 16px;
      line-height: 1.6;
      min-height: 100vh;
    }}

    a {{
      color: var(--cyan);
      text-decoration: none;
    }}

    a:hover {{
      text-decoration: underline;
    }}

    :focus-visible {{
      outline: 3px solid #4ccfff;
      outline-offset: 4px;
    }}

    .skip-link {{
      position: absolute;
      left: 16px;
      top: -64px;
      background: var(--primary);
      color: var(--primary-foreground);
      padding: 12px 18px;
      border-radius: 12px;
      font-size: 16px;
      font-weight: 600;
      z-index: 1000;
    }}

    .skip-link:focus {{
      top: 16px;
    }}

    .shell {{
      display: grid;
      grid-template-columns: 264px minmax(0, 1fr) 360px;
      min-height: 100vh;
    }}

    .sidebar {{
      background: var(--card);
      border-right: 1px solid var(--border);
      padding: 20px 16px;
      display: flex;
      flex-direction: column;
      gap: 18px;
    }}

    .brand-row {{
      display: flex;
      align-items: center;
      gap: 12px;
    }}

    #streamline-presence-slot {{
      width: 48px;
      height: 48px;
      display: inline-flex;
      align-items: center;
      justify-content: center;
      border-radius: 14px;
      background: var(--secondary);
      border: 1px solid var(--border);
      flex-shrink: 0;
    }}

    .presence-ring {{
      transition: transform 0.25s ease;
    }}

    body.motion-enabled .presence-core {{
      animation: pulseCore 2.6s ease-in-out infinite;
    }}

    @keyframes pulseCore {{
      0%, 100% {{ opacity: 0.78; }}
      50% {{ opacity: 1; }}
    }}

    @media (prefers-reduced-motion: reduce) {{
      body.motion-enabled .presence-core {{
        animation: none;
      }}
    }}

    .brand-title {{
      font-size: 20px;
      font-weight: 700;
      margin: 0;
      color: var(--text);
    }}

    .meta-floor {{
      font-size: 14px;
      color: var(--dim);
      margin: 0;
    }}

    .footer-stamp {{
      font-size: 16px;
      color: var(--dim);
    }}

    .status-box {{
      background: var(--muted);
      border: 1px solid var(--border);
      border-radius: var(--radius);
      padding: 14px;
      display: flex;
      flex-direction: column;
      gap: 8px;
    }}

    .badge {{
      display: inline-flex;
      align-items: center;
      gap: 6px;
      padding: 6px 12px;
      border-radius: 999px;
      font-size: 14px;
      font-weight: 600;
      border: 1px solid var(--border);
      background: var(--secondary);
      color: var(--foreground);
      width: fit-content;
    }}

    .badge-green {{
      border-color: var(--green);
      color: var(--green);
    }}

    .badge-cyan {{
      border-color: var(--cyan);
      color: var(--cyan);
    }}

    .nav-list {{
      display: flex;
      flex-direction: column;
      gap: 8px;
      margin: 0;
      padding: 0;
      list-style: none;
    }}

    .nav-item {{
      width: 100%;
      min-height: 48px;
      padding: 10px 14px;
      border-radius: 12px;
      border: 1px solid transparent;
      background: transparent;
      color: var(--foreground);
      font-size: 16px;
      font-weight: 500;
      text-align: left;
      cursor: pointer;
      display: flex;
      align-items: center;
      justify-content: space-between;
    }}

    .nav-item:hover, .nav-item.active {{
      background: var(--accent);
      border-color: var(--border);
    }}

    .main-region {{
      max-width: 1480px;
      width: 100%;
      margin: 0 auto;
      padding: 24px 28px 40px;
      display: flex;
      flex-direction: column;
      gap: 20px;
    }}

    .top-bar {{
      display: flex;
      flex-wrap: wrap;
      align-items: center;
      justify-content: space-between;
      gap: 12px;
      background: var(--card);
      border: 1px solid var(--border);
      border-radius: var(--radius);
      padding: 16px 20px;
    }}

    .card {{
      background: var(--card);
      border: 1px solid var(--border);
      border-radius: var(--radius);
      padding: 20px;
    }}

    .grid-cards {{
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
      gap: 16px;
    }}

    .btn {{
      min-height: 44px;
      min-width: 44px;
      padding: 10px 16px;
      border-radius: 12px;
      border: 1px solid var(--border);
      background: var(--secondary);
      color: var(--foreground);
      font-size: 16px;
      font-weight: 600;
      cursor: pointer;
      display: inline-flex;
      align-items: center;
      justify-content: center;
      gap: 8px;
    }}

    .btn-primary {{
      background: var(--primary);
      border-color: var(--primary);
      color: var(--primary-foreground);
    }}

    .input-control {{
      min-height: 44px;
      padding: 10px 14px;
      border-radius: 12px;
      border: 1px solid var(--border);
      background: var(--popover);
      color: var(--foreground);
      font-size: 16px;
      min-width: 220px;
    }}

    .right-panel {{
      background: var(--card);
      border-left: 1px solid var(--border);
      padding: 20px;
      display: flex;
      flex-direction: column;
      gap: 16px;
    }}

    .panel-tabs {{
      display: flex;
      flex-wrap: wrap;
      gap: 8px;
    }}

    .panel-tab {{
      min-height: 44px;
      padding: 8px 12px;
      border-radius: 10px;
      border: 1px solid var(--border);
      background: var(--secondary);
      color: var(--foreground);
      font-size: 14px;
      font-weight: 600;
      cursor: pointer;
    }}

    .panel-tab.active {{
      background: var(--primary);
      border-color: var(--primary);
      color: var(--primary-foreground);
    }}

    .code-inline {{
      font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
      font-size: 14px;
      background: var(--popover);
      border: 1px solid var(--border);
      border-radius: 8px;
      padding: 2px 8px;
      color: var(--cyan);
    }}

    .workspace-output {{
      background: var(--popover);
      border: 1px solid var(--border);
      border-radius: 14px;
      padding: 16px;
      font-size: 16px;
      overflow-x: auto;
      white-space: pre-wrap;
      word-break: break-word;
    }}

    @media (max-width: 1100px) {{
      .shell {{
        grid-template-columns: 1fr;
      }}
      .sidebar {{
        border-right: none;
        border-bottom: 1px solid var(--border);
      }}
      .right-panel {{
        border-left: none;
        border-top: 1px solid var(--border);
      }}
    }}
  </style>
</head>
<body class="motion-enabled">
  <a href="#main-content" class="skip-link">Skip to main content</a>
  <div class="shell">
    <aside class="sidebar" aria-label="Workspace navigation">
      <div class="brand-row">
        <div id="streamline-presence-slot" data-state="idle" aria-label="Server status mark">
          <svg width="32" height="32" viewBox="0 0 32 32" role="img" aria-label="Streamline status">
            <circle cx="16" cy="16" r="13" fill="none" stroke="#74d8ff" stroke-width="2.5"></circle>
            <circle class="presence-core" cx="16" cy="16" r="6" fill="#94ead1"></circle>
          </svg>
        </div>
        <div>
          <h1 class="brand-title">Streamline</h1>
          <p class="meta-floor">v{version}</p>
        </div>
      </div>

      <div class="status-box">
        <span class="badge badge-green" id="server-health-badge">Ready: {safe_mode}</span>
        <p class="meta-floor">Rules digest: <span class="code-inline" data-rules-sha256="{full_sha}">{short_sha}</span></p>
        <p class="meta-floor" id="rule-gate-status">Rule Gate: connect credential to inspect</p>
        <button type="button" class="btn" id="toggle-motion-btn" aria-pressed="false">Pause animation</button>
      </div>

      <nav aria-label="Primary destinations">
        <ul class="nav-list">
          <li><button type="button" class="nav-item active" data-view="home">Home</button></li>
          <li><button type="button" class="nav-item" data-view="projects">Projects (Trio+)</button></li>
          <li><button type="button" class="nav-item" data-view="shelves">Shelves (Governance)</button></li>
          <li><button type="button" class="nav-item" data-view="skills">My Profile &amp; Skills</button></li>
          <li><button type="button" class="nav-item" data-view="inbox">Approval Inbox</button></li>
          <li><button type="button" class="nav-item" data-view="apps">Connected Apps</button></li>
        </ul>
      </nav>
    </aside>

    <main id="main-content" class="main-region" tabindex="-1">
      <header class="top-bar">
        <div>
          <h2 style="margin:0;font-size:22px;">Governance, Central Lessons, and Trio+ Server</h2>
          <p class="meta-floor">Endpoints: <span class="code-inline">/ready</span>, <span class="code-inline">/mcp</span>, <span class="code-inline">/api/tools/:tool_name</span>, <span class="code-inline">/downloads/streamline-skills.zip</span></p>
        </div>
        <div style="display:flex;flex-wrap:wrap;gap:10px;align-items:center;">
          <label for="cred-input" class="meta-floor">Grant credential</label>
          <input id="cred-input" class="input-control" type="text" autocomplete="off" placeholder="Enter X-Streamline-Credential">
          <button type="button" class="btn btn-primary" id="connect-btn">Connect</button>
          <a class="btn" href="/downloads/streamline-skills.zip">Download Skills ZIP</a>
        </div>
      </header>

      <section class="grid-cards" aria-label="Server overview">
        <article class="card">
          <h3 style="margin-top:0;">Rule Gate Status</h3>
          <p>Agents must call <span class="code-inline">bootstrap</span> and <span class="code-inline">ack_rules</span> with the active digest before any write.</p>
          <button type="button" class="btn" id="ack-rules-btn">Acknowledge Active Rules</button>
        </article>
        <article class="card">
          <h3 style="margin-top:0;">3-Part Closeout Gate</h3>
          <p>Every version enforces cleanup verification, central <span class="code-inline">lessons_learned</span> entries, and plain-English <span class="code-inline">HOW-IT-WORKS.md</span> approval.</p>
          <button type="button" class="btn" id="load-lessons-btn">Query Central Lessons</button>
        </article>
        <article class="card">
          <h3 style="margin-top:0;">Redaction &amp; Tag Guard</h3>
          <p>All writes pass through <span class="code-inline">streamline-safe</span> before SQLite persistence and respect tag-scoped grants.</p>
          <a class="btn" href="/ready" target="_blank" rel="noopener">Inspect /ready JSON</a>
        </article>
      </section>

      <section class="card" aria-live="polite">
        <div style="display:flex;justify-content:space-between;align-items:center;gap:12px;flex-wrap:wrap;">
          <h3 id="workspace-view-title" style="margin:0;">Workspace Overview</h3>
          <span class="badge badge-cyan" id="workspace-status-pill">Idle</span>
        </div>
        <div id="workspace-output" class="workspace-output" style="margin-top:14px;">Connect with a grant credential to load active projects, shelves, approval queue items, and central forward lessons.</div>
      </section>

      <footer class="footer-stamp">
        Last updated: <span id="last-updated-stamp">Ready at startup</span> | Apache-2.0 Streamline Server
      </footer>
    </main>

    <aside class="right-panel" aria-label="Docked inspection panel">
      <div class="panel-tabs" role="tablist" aria-label="Right panel tabs">
        <a href="?panel=read" class="panel-tab {read_active}" data-panel="read">Read</a>
        <a href="?panel=history" class="panel-tab {history_active}" data-panel="history">History</a>
        <a href="?panel=related" class="panel-tab {related_active}" data-panel="related">Related</a>
        <a href="?panel=passport" class="panel-tab {passport_active}" data-panel="passport">Passport</a>
      </div>
      <div id="right-panel-body" class="workspace-output">Active panel: {active_panel}. Select a project or resource after connecting to inspect markdown content, revision history, Trio+ trace rows, or Source Passport metadata.</div>
    </aside>
  </div>

  <script>
    (function () {{
      let activeCredential = "";
      let activeRulesSha = "{full_sha}";
      const outputEl = document.getElementById("workspace-output");
      const statusPill = document.getElementById("workspace-status-pill");
      const ruleStatusEl = document.getElementById("rule-gate-status");
      const viewTitleEl = document.getElementById("workspace-view-title");
      const updatedEl = document.getElementById("last-updated-stamp");
      const motionBtn = document.getElementById("toggle-motion-btn");

      motionBtn.addEventListener("click", function () {{
        const paused = document.body.classList.toggle("motion-enabled") === false;
        motionBtn.setAttribute("aria-pressed", paused ? "true" : "false");
        motionBtn.textContent = paused ? "Resume animation" : "Pause animation";
      }});

      async function callTool(toolName, args) {{
        const headers = {{ "Content-Type": "application/json" }};
        if (activeCredential) {{
          headers["X-Streamline-Credential"] = activeCredential;
        }}
        statusPill.textContent = "Loading " + toolName;
        const resp = await fetch("/api/tools/" + encodeURIComponent(toolName), {{
          method: "POST",
          headers: headers,
          body: JSON.stringify(args || {{}})
        }});
        const payload = await resp.json();
        updatedEl.textContent = new Date().toISOString();
        statusPill.textContent = resp.ok ? "Ready" : ("Error " + resp.status);
        return {{ ok: resp.ok, status: resp.status, payload: payload }};
      }}

      async function loadBootstrap() {{
        const res = await callTool("bootstrap", {{}});
        if (!res.ok) {{
          outputEl.textContent = JSON.stringify(res.payload, null, 2);
          return;
        }}
        const data = res.payload.data || {{}};
        activeRulesSha = data.rules_sha256 || activeRulesSha;
        ruleStatusEl.textContent = data.rules_acked ? "Rule Gate: Acknowledged" : "Rule Gate: Ack Required";
        outputEl.textContent = JSON.stringify(data, null, 2);
      }}

      document.getElementById("connect-btn").addEventListener("click", function () {{
        activeCredential = document.getElementById("cred-input").value.trim();
        loadBootstrap();
      }});

      document.getElementById("ack-rules-btn").addEventListener("click", async function () {{
        const res = await callTool("ack_rules", {{ rules_sha256: activeRulesSha }});
        outputEl.textContent = JSON.stringify(res.payload, null, 2);
        if (res.ok) {{
          ruleStatusEl.textContent = "Rule Gate: Acknowledged";
        }}
      }});

      document.getElementById("load-lessons-btn").addEventListener("click", async function () {{
        viewTitleEl.textContent = "Central Lessons Learned";
        const res = await callTool("query_lessons", {{ limit: 25 }});
        outputEl.textContent = JSON.stringify(res.payload, null, 2);
      }});

      document.querySelectorAll(".nav-item").forEach(function (btn) {{
        btn.addEventListener("click", async function () {{
          document.querySelectorAll(".nav-item").forEach(function (n) {{ n.classList.remove("active"); }});
          btn.classList.add("active");
          const view = btn.getAttribute("data-view");
          viewTitleEl.textContent = btn.textContent;
          if (!activeCredential && view !== "apps") {{
            outputEl.textContent = "Enter a valid grant credential and click Connect to view " + btn.textContent + ".";
            return;
          }}
          if (view === "home") {{
            loadBootstrap();
          }} else if (view === "projects") {{
            const res = await callTool("list_trio_projects", {{}});
            outputEl.textContent = JSON.stringify(res.payload, null, 2);
          }} else if (view === "shelves") {{
            const res = await callTool("list_shelves", {{}});
            outputEl.textContent = JSON.stringify(res.payload, null, 2);
          }} else if (view === "skills") {{
            const res = await callTool("list_skills", {{}});
            outputEl.textContent = JSON.stringify(res.payload, null, 2);
          }} else if (view === "inbox") {{
            const res = await callTool("list_trio_projects", {{}});
            if (res.ok && Array.isArray(res.payload.data)) {{
              const pending = res.payload.data.filter(function (p) {{ return p.state === "DOC_PENDING_OWNER"; }});
              outputEl.textContent = pending.length ? JSON.stringify(pending, null, 2) : "No projects currently awaiting owner approval in DOC_PENDING_OWNER.";
            }} else {{
              outputEl.textContent = JSON.stringify(res.payload, null, 2);
            }}
          }} else {{
            outputEl.textContent = "MCP HTTP endpoint: POST /mcp\nREST mirror: POST /api/tools/:tool_name\nStdio mode: streamline mcp-stdio\nPortable skills archive: GET /downloads/streamline-skills.zip";
          }}
        }});
      }});
    }})();
  </script>
  {presence_script_tag}
</body>
</html>
"##,
        version = escape_html(version),
        safe_mode = escape_html(safe_mode_label),
        full_sha = escape_html(&rules_sha),
        short_sha = escape_html(&short_sha),
        active_panel = escape_html(active_panel),
        read_active = if active_panel == "read" { "active" } else { "" },
        history_active = if active_panel == "history" {
            "active"
        } else {
            ""
        },
        related_active = if active_panel == "related" {
            "active"
        } else {
            ""
        },
        passport_active = if active_panel == "passport" {
            "active"
        } else {
            ""
        },
        presence_script_tag = presence_script_tag,
    );

    Html(html)
}
