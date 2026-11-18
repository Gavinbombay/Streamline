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

const UI_CSS: &str = r##"
    :root {
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
      --warn-border: #f5b971;
      --warn-text: #ffe0b2;
      --danger-border: #ff7b89;
      --danger-text: #ffd0d5;
    }

    *, *::before, *::after {
      box-sizing: border-box;
    }

    html, body {
      margin: 0;
      padding: 0;
      background-color: var(--background);
      color: var(--foreground);
      font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
      font-size: 16px;
      line-height: 1.6;
      min-height: 100vh;
    }

    a {
      color: var(--cyan);
      text-decoration: none;
    }

    a:hover {
      text-decoration: underline;
    }

    :focus-visible {
      outline: 3px solid #4ccfff;
      outline-offset: 4px;
    }

    [hidden] {
      display: none !important;
    }

    .skip-link {
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
    }

    .skip-link:focus {
      top: 16px;
    }

    .shell {
      display: grid;
      grid-template-columns: 264px minmax(0, 1fr) 380px;
      min-height: 100vh;
    }

    .sidebar {
      background: var(--card);
      border-right: 1px solid var(--border);
      padding: 20px 16px;
      display: flex;
      flex-direction: column;
      gap: 16px;
    }

    .brand-row {
      display: flex;
      align-items: center;
      gap: 12px;
    }

    #streamline-presence-slot {
      width: 48px;
      height: 48px;
      display: inline-flex;
      align-items: center;
      justify-content: center;
      border-radius: 14px;
      background: var(--secondary);
      border: 1px solid var(--border);
      flex-shrink: 0;
    }

    #streamline-presence-slot[data-state="active"] {
      border-color: var(--cyan);
    }

    #streamline-presence-slot[data-state="conveyor"] {
      border-color: var(--primary);
    }

    #streamline-presence-slot[data-state="action-needed"] {
      border-color: var(--warn-border);
    }

    body.motion-enabled .presence-core {
      animation: pulseCore 2.6s ease-in-out infinite;
    }

    @keyframes pulseCore {
      0%, 100% { opacity: 0.78; }
      50% { opacity: 1; }
    }

    @media (prefers-reduced-motion: reduce) {
      body.motion-enabled .presence-core {
        animation: none;
      }
    }

    .brand-title {
      font-size: 20px;
      font-weight: 700;
      margin: 0;
      color: var(--text);
    }

    .meta-floor {
      font-size: 14px;
      color: var(--dim);
      margin: 0;
    }

    .footer-stamp {
      font-size: 16px;
      color: var(--dim);
      padding-top: 8px;
    }

    .status-box {
      background: var(--muted);
      border: 1px solid var(--border);
      border-radius: var(--radius);
      padding: 14px;
      display: flex;
      flex-direction: column;
      gap: 8px;
    }

    .badge {
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
    }

    .badge-green {
      border-color: var(--green);
      color: var(--green);
    }

    .badge-cyan {
      border-color: var(--cyan);
      color: var(--cyan);
    }

    .badge-warn {
      border-color: var(--warn-border);
      color: var(--warn-text);
    }

    .badge-danger {
      border-color: var(--danger-border);
      color: var(--danger-text);
    }

    .nav-list {
      display: flex;
      flex-direction: column;
      gap: 8px;
      margin: 0;
      padding: 0;
      list-style: none;
    }

    .nav-item {
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
      gap: 8px;
    }

    .nav-item:hover, .nav-item.active {
      background: var(--accent);
      border-color: var(--border);
    }

    .main-region {
      max-width: 1480px;
      width: 100%;
      margin: 0 auto;
      padding: 20px 24px 40px;
      display: flex;
      flex-direction: column;
      gap: 18px;
    }

    .top-bar {
      display: flex;
      flex-wrap: wrap;
      align-items: center;
      justify-content: space-between;
      gap: 14px;
      background: var(--card);
      border: 1px solid var(--border);
      border-radius: var(--radius);
      padding: 16px 20px;
    }

    .banner {
      border-radius: var(--radius);
      padding: 16px 20px;
      border: 1px solid var(--warn-border);
      background: #231d12;
      color: var(--warn-text);
      display: flex;
      flex-wrap: wrap;
      align-items: center;
      justify-content: space-between;
      gap: 12px;
    }

    .banner-danger {
      border-color: var(--danger-border);
      background: #2b131b;
      color: var(--danger-text);
    }

    .card {
      background: var(--card);
      border: 1px solid var(--border);
      border-radius: var(--radius);
      padding: 20px;
    }

    .subcard {
      background: var(--muted);
      border: 1px solid var(--border);
      border-radius: 14px;
      padding: 16px;
    }

    .grid-cards {
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
      gap: 16px;
    }

    .split-grid {
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
      gap: 16px;
    }

    .btn {
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
    }

    .btn-primary {
      background: var(--primary);
      border-color: var(--primary);
      color: var(--primary-foreground);
    }

    .btn.active-filter {
      background: var(--primary);
      border-color: var(--cyan);
      color: var(--primary-foreground);
    }

    .input-control, .select-control, .textarea-control {
      min-height: 44px;
      padding: 10px 14px;
      border-radius: 12px;
      border: 1px solid var(--border);
      background: var(--popover);
      color: var(--foreground);
      font-size: 16px;
      font-family: inherit;
    }

    .input-control {
      min-width: 180px;
    }

    .textarea-control {
      width: 100%;
      min-height: 110px;
      resize: vertical;
    }

    .masked-input {
      -webkit-text-security: disc;
      text-security: disc;
    }

    .form-grid {
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(190px, 1fr));
      gap: 12px;
      align-items: end;
    }

    .field-group {
      display: flex;
      flex-direction: column;
      gap: 6px;
    }

    .checkbox-label {
      display: inline-flex;
      align-items: center;
      gap: 8px;
      min-height: 44px;
      font-size: 16px;
      cursor: pointer;
    }

    .checkbox-label input[type="checkbox"] {
      width: 20px;
      height: 20px;
    }

    .action-row {
      display: flex;
      flex-wrap: wrap;
      gap: 10px;
      align-items: center;
    }

    .dital-row {
      display: inline-flex;
      gap: 6px;
      align-items: center;
      flex-wrap: wrap;
    }

    .stage-pill {
      display: inline-flex;
      align-items: center;
      justify-content: center;
      min-width: 32px;
      min-height: 32px;
      padding: 4px 10px;
      border-radius: 8px;
      font-size: 14px;
      font-weight: 700;
      border: 1px solid var(--border);
      background: var(--popover);
      color: var(--dim);
    }

    .stage-pill.stage-current {
      border-color: var(--green);
      background: #143634;
      color: var(--green);
    }

    .stage-pill.stage-unknown {
      border-color: var(--warn-border);
      color: var(--warn-text);
    }

    .data-table {
      width: 100%;
      border-collapse: collapse;
      font-size: 14px;
    }

    .data-table th, .data-table td {
      text-align: left;
      padding: 10px 12px;
      border-bottom: 1px solid var(--border);
      vertical-align: top;
    }

    .data-table th {
      color: var(--cyan);
      font-size: 14px;
      font-weight: 700;
    }

    .diff-grid {
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(260px, 1fr));
      gap: 12px;
      margin-top: 12px;
    }

    .diff-pane {
      background: var(--popover);
      border: 1px solid var(--border);
      border-radius: 12px;
      padding: 12px;
      font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
      font-size: 14px;
      white-space: pre-wrap;
      word-break: break-word;
    }

    .diff-line-added, ins.word-added {
      background: #133c2e;
      color: var(--green);
      text-decoration: none;
      padding: 2px 4px;
      border-radius: 4px;
      display: inline-block;
    }

    .diff-line-removed, del.word-removed {
      background: #401a24;
      color: var(--danger-text);
      padding: 2px 4px;
      border-radius: 4px;
      display: inline-block;
    }

    .diff-line-context {
      color: var(--dim);
    }

    .word-diff-strip {
      display: flex;
      flex-wrap: wrap;
      gap: 6px;
      align-items: center;
      background: var(--popover);
      border: 1px solid var(--border);
      border-radius: 12px;
      padding: 12px;
      font-size: 14px;
      margin-top: 10px;
    }

    .right-panel {
      background: var(--card);
      border-left: 1px solid var(--border);
      padding: 20px;
      display: flex;
      flex-direction: column;
      gap: 14px;
    }

    .panel-tabs {
      display: flex;
      flex-wrap: wrap;
      gap: 8px;
    }

    .panel-tab {
      min-height: 44px;
      padding: 8px 12px;
      border-radius: 10px;
      border: 1px solid var(--border);
      background: var(--secondary);
      color: var(--foreground);
      font-size: 14px;
      font-weight: 600;
      cursor: pointer;
      display: inline-flex;
      align-items: center;
    }

    .panel-tab.active {
      background: var(--primary);
      border-color: var(--primary);
      color: var(--primary-foreground);
    }

    .code-inline {
      font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
      font-size: 14px;
      background: var(--popover);
      border: 1px solid var(--border);
      border-radius: 8px;
      padding: 2px 8px;
      color: var(--cyan);
    }

    .workspace-output {
      background: var(--popover);
      border: 1px solid var(--border);
      border-radius: 14px;
      padding: 16px;
      font-size: 16px;
      overflow-x: auto;
      word-break: break-word;
    }

    .md-render h1, .md-render h2, .md-render h3 {
      margin: 14px 0 8px;
      color: var(--cyan);
    }

    .md-render p {
      margin: 8px 0;
    }

    .md-render pre {
      background: var(--background);
      border: 1px solid var(--border);
      border-radius: 10px;
      padding: 12px;
      font-size: 14px;
      overflow-x: auto;
    }

    .md-render blockquote {
      margin: 10px 0;
      padding: 8px 14px;
      border-left: 3px solid var(--cyan);
      background: var(--muted);
      color: var(--dim);
    }

    .list-stack {
      display: flex;
      flex-direction: column;
      gap: 12px;
    }

    .empty-state {
      background: var(--muted);
      border: 1px dashed var(--border);
      border-radius: 14px;
      padding: 20px;
      display: flex;
      flex-direction: column;
      gap: 12px;
      align-items: flex-start;
    }

    @media (max-width: 1100px) {
      .shell {
        grid-template-columns: 1fr;
      }
      .sidebar {
        border-right: none;
        border-bottom: 1px solid var(--border);
      }
      .right-panel {
        border-left: none;
        border-top: 1px solid var(--border);
      }
    }
"##;

const UI_JS: &str = r####"
    (function () {
      const sessionSlotName = "streamline_session_cred";
      const shaNode = document.querySelector("[data-rules-sha256]");
      let activeRulesSha = shaNode ? shaNode.getAttribute("data-rules-sha256") : "";
      let activeCredential = "";
      let activeView = "overview";
      let activePanel = (function () {
        const params = new URLSearchParams(window.location.search);
        const val = (params.get("panel") || "read").toLowerCase();
        return ["read", "history", "related", "passport"].includes(val) ? val : "read";
      })();

      let cachedWorkspace = null;
      let cachedBootstrap = null;
      let cachedProjects = [];
      let selectedProjectBundle = null;
      let selectedProjectRuns = [];
      let cachedResources = [];
      let selectedResourceData = null;
      let selectedResourceHistory = [];
      let selectedResourcePassport = null;
      let triageTierFilter = "";
      let triageNeedsOnly = false;
      let triageSearchTimer = null;

      const presenceSlot = document.getElementById("streamline-presence-slot");
      const statusPill = document.getElementById("workspace-status-pill");
      const ruleStatusEl = document.getElementById("rule-gate-status");
      const updatedEl = document.getElementById("last-updated-stamp");
      const motionBtn = document.getElementById("toggle-motion-btn");
      const credInput = document.getElementById("cred-input");
      const ruleBanner = document.getElementById("rule-gate-banner");
      const ruleBannerSha = document.getElementById("rule-gate-banner-sha");
      const alertBanner = document.getElementById("alert-banner");
      const alertBannerMsg = document.getElementById("alert-banner-msg");
      const rightPanelBody = document.getElementById("right-panel-body");

      function makeEl(tag, className, textVal) {
        const node = document.createElement(tag);
        if (className) {
          node.className = className;
        }
        if (textVal !== undefined && textVal !== null) {
          node.textContent = String(textVal);
        }
        return node;
      }

      function setPresenceState(nextState) {
        if (presenceSlot) {
          presenceSlot.setAttribute("data-state", nextState);
        }
      }

      function showAlert(msg, isDanger) {
        alertBannerMsg.textContent = msg;
        if (isDanger) {
          alertBanner.classList.add("banner-danger");
        } else {
          alertBanner.classList.remove("banner-danger");
        }
        alertBanner.hidden = false;
      }

      function clearAlerts() {
        alertBanner.hidden = true;
        alertBannerMsg.textContent = "";
      }

      function showRuleBanner(shaVal) {
        if (shaVal) {
          activeRulesSha = shaVal;
        }
        ruleBannerSha.textContent = activeRulesSha || "unknown";
        ruleBanner.hidden = false;
        setPresenceState("action-needed");
      }

      function hideRuleBanner() {
        ruleBanner.hidden = true;
      }

      function formatErrorMessage(payload, status) {
        const err = payload && payload.error ? payload.error : null;
        if (!err) {
          return "Request failed with status " + status;
        }
        if (err.code === "REDACTION_BLOCKED" && err.detail) {
          const fieldName = err.detail.field || "input";
          const items = Array.isArray(err.detail.violations) ? err.detail.violations : [];
          const lineInfo = items.map(function (v) {
            return "line " + v.line + " (" + v.reason + ")";
          }).join("; ");
          return "422 Redaction Blocked in field '" + fieldName + "'" + (lineInfo ? ": " + lineInfo : "");
        }
        return (err.code || "ERROR") + " (" + status + "): " + (err.message || "request failed");
      }

      async function callTool(toolName, args) {
        clearAlerts();
        const headers = { "Content-Type": "application/json" };
        if (activeCredential) {
          headers["X-Streamline-Credential"] = activeCredential;
        }
        statusPill.textContent = "Running " + toolName;
        setPresenceState(toolName === "dispatch_run" ? "conveyor" : "active");
        try {
          const resp = await fetch("/api/tools/" + encodeURIComponent(toolName), {
            method: "POST",
            headers: headers,
            body: JSON.stringify(args || {})
          });
          let payload = {};
          try {
            payload = await resp.json();
          } catch (_e) {
            payload = { ok: false, error: { code: "INVALID_RESPONSE", message: "non-JSON response" } };
          }
          updatedEl.textContent = new Date().toISOString();
          statusPill.textContent = resp.ok ? "Ready" : ("Error " + resp.status);
          if (!resp.ok) {
            const errCode = payload && payload.error ? payload.error.code : "";
            if (resp.status === 412 || errCode === "RULES_ACK_REQUIRED") {
              const shaFromErr = (payload.error && payload.error.current_rules_sha256)
                || (payload.error && payload.error.detail && payload.error.detail.current_rules_sha256)
                || activeRulesSha;
              showRuleBanner(shaFromErr);
            } else {
              showAlert(formatErrorMessage(payload, resp.status), true);
            }
            setPresenceState("action-needed");
          } else {
            setPresenceState("idle");
          }
          return { ok: resp.ok, status: resp.status, payload: payload };
        } catch (networkErr) {
          statusPill.textContent = "Network Error";
          setPresenceState("action-needed");
          showAlert("Network failure calling " + toolName + ": " + String(networkErr), true);
          return { ok: false, status: 0, payload: {} };
        }
      }

      function renderMarkdownSafe(container, markdownText) {
        container.replaceChildren();
        const wrap = makeEl("div", "md-render");
        const lines = String(markdownText || "").replace(/\r\n/g, "\n").split("\n");
        let inCode = false;
        let codeLines = [];
        let listEl = null;

        function flushList() {
          if (listEl) {
            wrap.appendChild(listEl);
            listEl = null;
          }
        }

        lines.forEach(function (line) {
          if (line.startsWith("```")) {
            flushList();
            if (!inCode) {
              inCode = true;
              codeLines = [];
            } else {
              inCode = false;
              const pre = makeEl("pre");
              pre.textContent = codeLines.join("\n");
              wrap.appendChild(pre);
            }
            return;
          }
          if (inCode) {
            codeLines.push(line);
            return;
          }
          const trimmed = line.trim();
          if (!trimmed) {
            flushList();
            return;
          }
          if (trimmed.startsWith("### ")) {
            flushList();
            wrap.appendChild(makeEl("h3", "", trimmed.slice(4)));
          } else if (trimmed.startsWith("## ")) {
            flushList();
            wrap.appendChild(makeEl("h2", "", trimmed.slice(3)));
          } else if (trimmed.startsWith("# ")) {
            flushList();
            wrap.appendChild(makeEl("h1", "", trimmed.slice(2)));
          } else if (trimmed.startsWith("> ")) {
            flushList();
            wrap.appendChild(makeEl("blockquote", "", trimmed.slice(2)));
          } else if (trimmed.startsWith("- ") || trimmed.startsWith("* ")) {
            if (!listEl) {
              listEl = makeEl("ul");
            }
            listEl.appendChild(makeEl("li", "", trimmed.slice(2)));
          } else {
            flushList();
            wrap.appendChild(makeEl("p", "", line));
          }
        });
        flushList();
        if (inCode && codeLines.length) {
          const pre = makeEl("pre");
          pre.textContent = codeLines.join("\n");
          wrap.appendChild(pre);
        }
        container.appendChild(wrap);
      }

      function freshnessBadgeClass(freshness) {
        const val = String(freshness || "unknown").toLowerCase();
        if (val === "verified") { return "badge badge-green"; }
        if (val === "stale" || val === "unknown") { return "badge badge-warn"; }
        if (val === "drift") { return "badge badge-danger"; }
        return "badge";
      }

      function switchView(nextView) {
        activeView = nextView;
        document.querySelectorAll(".nav-item").forEach(function (btn) {
          const isMatch = btn.getAttribute("data-view") === nextView;
          btn.classList.toggle("active", isMatch);
        });
        document.querySelectorAll(".view-panel").forEach(function (sec) {
          const isMatch = sec.getAttribute("data-view-panel") === nextView;
          sec.hidden = !isMatch;
        });
        if (activeCredential) {
          refreshActiveView();
        }
      }

      function switchPanel(nextPanel, updateUrl) {
        activePanel = nextPanel;
        document.querySelectorAll(".panel-tab").forEach(function (tab) {
          tab.classList.toggle("active", tab.getAttribute("data-panel") === nextPanel);
        });
        if (updateUrl) {
          const url = new URL(window.location.href);
          url.searchParams.set("panel", nextPanel);
          window.history.replaceState(null, "", url.pathname + url.search);
        }
        renderRightPanel();
      }

      function renderRightPanel() {
        rightPanelBody.replaceChildren();

        if (activePanel === "read") {
          if (!selectedResourceData) {
            rightPanelBody.appendChild(makeEl("p", "meta-floor", "Active panel: read. Select any project document, shelf item, or skill to read its rendered Markdown."));
            return;
          }
          const res = selectedResourceData.resource || {};
          const head = makeEl("div", "list-stack");
          head.appendChild(makeEl("h4", "", res.title || res.slug || "Resource"));
          const pills = makeEl("div", "action-row");
          if (res.shelf_slug && res.slug) {
            pills.appendChild(makeEl("span", "badge badge-cyan", res.shelf_slug + "/" + res.slug));
          }
          if (res.current_rev) {
            pills.appendChild(makeEl("span", "badge", "rev " + res.current_rev));
          }
          if (res.lifecycle_tier) {
            pills.appendChild(makeEl("span", "badge", res.lifecycle_tier));
          }
          head.appendChild(pills);
          rightPanelBody.appendChild(head);
          const mdHost = makeEl("div");
          renderMarkdownSafe(mdHost, selectedResourceData.body_md || "");
          rightPanelBody.appendChild(mdHost);
          return;
        }

        if (activePanel === "history") {
          if (!selectedResourceData || !selectedResourceData.resource || !selectedResourceData.resource.shelf_slug) {
            rightPanelBody.appendChild(makeEl("p", "meta-floor", "Active panel: history. Select a stored resource to inspect revisions and restore prior versions."));
            return;
          }
          const res = selectedResourceData.resource;
          rightPanelBody.appendChild(makeEl("h4", "", "Revision History: " + res.shelf_slug + "/" + res.slug));
          if (!selectedResourceHistory.length) {
            rightPanelBody.appendChild(makeEl("p", "meta-floor", "No revision entries returned."));
            return;
          }
          const list = makeEl("div", "list-stack");
          selectedResourceHistory.forEach(function (revItem) {
            const card = makeEl("div", "subcard");
            card.appendChild(makeEl("div", "badge badge-cyan", "rev " + revItem.rev + " by " + revItem.author_principal));
            card.appendChild(makeEl("p", "", revItem.summary || "(no summary)"));
            card.appendChild(makeEl("p", "meta-floor", revItem.created_at + " | sha256: " + String(revItem.sha256 || "").slice(0, 12)));
            const rowBtns = makeEl("div", "action-row");
            if (revItem.rev !== res.current_rev) {
              const cmpBtn = makeEl("button", "btn", "Compare to Current");
              cmpBtn.type = "button";
              cmpBtn.addEventListener("click", async function () {
                const cmpRes = await callTool("compare_resources", {
                  shelf_slug: res.shelf_slug,
                  slug: res.slug,
                  left_rev: revItem.rev,
                  right_rev: res.current_rev
                });
                if (cmpRes.ok && cmpRes.payload.data) {
                  renderDiffViewer(cmpRes.payload.data, "rev " + revItem.rev + " vs rev " + res.current_rev);
                  switchView("projects");
                }
              });
              const restoreBtn = makeEl("button", "btn btn-primary", "Restore Revision");
              restoreBtn.type = "button";
              restoreBtn.addEventListener("click", async function () {
                const rst = await callTool("restore_resource", {
                  shelf_slug: res.shelf_slug,
                  slug: res.slug,
                  target_rev: revItem.rev
                });
                if (rst.ok) {
                  await selectResource(res.shelf_slug, res.slug);
                }
              });
              rowBtns.appendChild(cmpBtn);
              rowBtns.appendChild(restoreBtn);
            } else {
              rowBtns.appendChild(makeEl("span", "badge badge-green", "Current Revision"));
            }
            card.appendChild(rowBtns);
            list.appendChild(card);
          });
          rightPanelBody.appendChild(list);
          return;
        }

        if (activePanel === "related") {
          const bundle = selectedProjectBundle;
          if (!bundle) {
            rightPanelBody.appendChild(makeEl("p", "meta-floor", "Active panel: related. Select a Trio+ project or project-linked resource to inspect OWN decisions, REQ rows, divergences, and sync reports."));
            return;
          }
          rightPanelBody.appendChild(makeEl("h4", "", "Related Trio+ Rows: " + bundle.project.project_id));
          rightPanelBody.appendChild(makeEl("p", "meta-floor", "OWN decisions: " + bundle.own_decisions.length + " | REQ rows: " + bundle.requirements.length + " | DIVs: " + bundle.divergences.length));

          if (bundle.requirements.length) {
            const reqList = makeEl("div", "list-stack");
            bundle.requirements.forEach(function (reqRow) {
              const item = makeEl("div", "subcard");
              item.appendChild(makeEl("span", "badge badge-cyan", reqRow.req_id + " [" + reqRow.stage + "]"));
              item.appendChild(makeEl("p", "", reqRow.statement));
              if (reqRow.evidence_ref) {
                item.appendChild(makeEl("p", "meta-floor", "Evidence: " + reqRow.evidence_ref));
              }
              reqList.appendChild(item);
            });
            rightPanelBody.appendChild(reqList);
          }

          if (bundle.divergences.length) {
            rightPanelBody.appendChild(makeEl("h4", "", "Divergences"));
            bundle.divergences.forEach(function (divRow) {
              const item = makeEl("div", "subcard");
              item.appendChild(makeEl("span", "badge badge-warn", divRow.div_id + " (" + divRow.status + ")"));
              item.appendChild(makeEl("p", "", "Observed: " + divRow.observed));
              item.appendChild(makeEl("p", "meta-floor", "Resolution: " + divRow.resolution));
              rightPanelBody.appendChild(item);
            });
          }

          if (bundle.sync_reports.length) {
            const latest = bundle.sync_reports[0];
            const syncBox = makeEl("div", "subcard");
            syncBox.appendChild(makeEl("span", "badge badge-green", "Latest Sync: " + latest.verdict + " (" + latest.acceptor_family + ")"));
            syncBox.appendChild(makeEl("p", "meta-floor", "Plan rev " + latest.plan_rev + " vs As-Built rev " + latest.asbuilt_rev));
            rightPanelBody.appendChild(syncBox);
          }
          return;
        }

        if (activePanel === "passport") {
          const res = selectedResourceData && selectedResourceData.resource ? selectedResourceData.resource : null;
          if (!res || !res.id) {
            rightPanelBody.appendChild(makeEl("p", "meta-floor", "Active panel: passport. Select a resource to inspect steward, freshness_state, last_verified_at, lineage_ref, and next_action."));
            return;
          }
          const passport = selectedResourcePassport || (selectedResourceData ? selectedResourceData.passport : null);
          rightPanelBody.appendChild(makeEl("h4", "", "Source Passport: " + res.title));
          const card = makeEl("div", "subcard");
          const freshness = passport ? passport.freshness_state : "unknown";
          card.appendChild(makeEl("span", freshnessBadgeClass(freshness), "Freshness: " + String(freshness).toUpperCase()));
          card.appendChild(makeEl("p", "", "Steward: " + (passport ? passport.steward : "Unassigned")));
          card.appendChild(makeEl("p", "meta-floor", "Last verified at: " + (passport ? passport.last_verified_at : "Never")));
          card.appendChild(makeEl("p", "meta-floor", "Lineage ref: " + (passport ? passport.lineage_ref : "None")));
          card.appendChild(makeEl("p", "meta-floor", "Access scope: " + (passport ? passport.access_scope : res.shelf_slug)));
          card.appendChild(makeEl("p", "", "Next action: " + (passport ? passport.next_action : "Review and verify source")));
          rightPanelBody.appendChild(card);

          const form = makeEl("form", "list-stack");
          const stewardIn = makeEl("input", "input-control");
          stewardIn.placeholder = "Steward handle";
          stewardIn.value = passport ? passport.steward : "owner";
          const freshSel = makeEl("select", "select-control");
          ["verified", "stale", "drift", "unknown"].forEach(function (optVal) {
            const opt = makeEl("option", "", optVal);
            opt.value = optVal;
            if (optVal === freshness) { opt.selected = true; }
            freshSel.appendChild(opt);
          });
          const lineageIn = makeEl("input", "input-control");
          lineageIn.placeholder = "Lineage reference";
          lineageIn.value = passport ? passport.lineage_ref : (res.shelf_slug + "/" + res.slug);
          const nextActIn = makeEl("input", "input-control");
          nextActIn.placeholder = "Next action";
          nextActIn.value = passport ? passport.next_action : "Verified in workspace";
          const savePassBtn = makeEl("button", "btn btn-primary", "Update Source Passport");
          savePassBtn.type = "submit";
          form.appendChild(stewardIn);
          form.appendChild(freshSel);
          form.appendChild(lineageIn);
          form.appendChild(nextActIn);
          form.appendChild(savePassBtn);
          form.addEventListener("submit", async function (ev) {
            ev.preventDefault();
            const upd = await callTool("update_source_passport", {
              resource_id: res.id,
              steward: stewardIn.value.trim() || "owner",
              freshness_state: freshSel.value,
              lineage_ref: lineageIn.value.trim() || (res.shelf_slug + "/" + res.slug),
              next_action: nextActIn.value.trim() || "Verified"
            });
            if (upd.ok) {
              selectedResourcePassport = upd.payload.data;
              renderRightPanel();
            }
          });
          rightPanelBody.appendChild(form);
        }
      }

      async function selectResource(shelfSlug, slug) {
        const resCall = await callTool("read_resource", { shelf_slug: shelfSlug, slug: slug });
        if (!resCall.ok || !resCall.payload.data) {
          return;
        }
        selectedResourceData = resCall.payload.data;
        selectedResourcePassport = selectedResourceData.passport || null;

        const histCall = await callTool("resource_history", { shelf_slug: shelfSlug, slug: slug });
        selectedResourceHistory = (histCall.ok && Array.isArray(histCall.payload.data)) ? histCall.payload.data : [];

        if (selectedResourceData.resource && selectedResourceData.resource.project_id) {
          const pid = selectedResourceData.resource.project_id;
          if (!selectedProjectBundle || selectedProjectBundle.project.project_id !== pid) {
            const bCall = await callTool("get_project_bundle", { project_id: pid });
            if (bCall.ok && bCall.payload.data) {
              selectedProjectBundle = bCall.payload.data;
            }
          }
        }
        renderRightPanel();
      }

      function renderDiffViewer(diffData, labelText) {
        const pane = document.getElementById("diff-viewer-pane");
        const summaryEl = document.getElementById("diff-summary-text");
        const sideEl = document.getElementById("diff-side-by-side");
        const wordsEl = document.getElementById("diff-changed-words");
        pane.hidden = false;
        summaryEl.textContent = labelText + " | " + (diffData.summary || ("unchanged lines: " + diffData.unchanged_lines));

        sideEl.replaceChildren();
        const leftBox = makeEl("div", "diff-pane");
        leftBox.appendChild(makeEl("div", "badge", "Left (rev " + diffData.left_rev + ")"));
        const rightBox = makeEl("div", "diff-pane");
        rightBox.appendChild(makeEl("div", "badge", "Right (rev " + diffData.right_rev + ")"));

        const hunks = Array.isArray(diffData.line_hunks) ? diffData.line_hunks : [];
        hunks.forEach(function (hunk) {
          const kind = hunk.kind || "context";
          const lineText = hunk.text || "";
          if (kind === "removed") {
            leftBox.appendChild(makeEl("div", "diff-line-removed", "- " + lineText));
          } else if (kind === "added") {
            rightBox.appendChild(makeEl("div", "diff-line-added", "+ " + lineText));
          } else {
            leftBox.appendChild(makeEl("div", "diff-line-context", "  " + lineText));
            rightBox.appendChild(makeEl("div", "diff-line-context", "  " + lineText));
          }
        });
        sideEl.appendChild(leftBox);
        sideEl.appendChild(rightBox);

        wordsEl.replaceChildren();
        wordsEl.appendChild(makeEl("strong", "", "Changed words: "));
        const words = Array.isArray(diffData.changed_words) ? diffData.changed_words : [];
        if (!words.length) {
          wordsEl.appendChild(makeEl("span", "meta-floor", "No word-level changes."));
        } else {
          words.forEach(function (w) {
            if (w.kind === "added") {
              wordsEl.appendChild(makeEl("ins", "word-added", "+" + w.word));
            } else if (w.kind === "removed") {
              wordsEl.appendChild(makeEl("del", "word-removed", "-" + w.word));
            } else {
              wordsEl.appendChild(makeEl("span", "meta-floor", w.word));
            }
          });
        }
      }

      async function loadBootstrapAndWorkspace() {
        const [bootRes, wsRes] = await Promise.all([
          callTool("bootstrap", {}),
          callTool("get_workspace", {})
        ]);
        if (bootRes.ok && bootRes.payload.data) {
          cachedBootstrap = bootRes.payload.data;
          activeRulesSha = cachedBootstrap.rules_sha256 || activeRulesSha;
          if (cachedBootstrap.rules_acked) {
            ruleStatusEl.textContent = "Rule Gate: Acknowledged";
            hideRuleBanner();
          } else {
            ruleStatusEl.textContent = "Rule Gate: Ack Required";
            showRuleBanner(activeRulesSha);
          }
        }
        if (wsRes.ok && wsRes.payload.data) {
          cachedWorkspace = wsRes.payload.data;
          cachedProjects = Array.isArray(cachedWorkspace.projects) ? cachedWorkspace.projects : [];
          const pendingCount = cachedProjects.filter(function (p) { return p.state === "DOC_PENDING_OWNER"; }).length;
          const badgeNode = document.getElementById("inbox-count-badge");
          if (badgeNode) {
            badgeNode.textContent = String(pendingCount);
          }
          if (pendingCount > 0 && cachedWorkspace.rules_acked) {
            setPresenceState("action-needed");
          }
        }
        renderOverview();
        refreshActiveView();
      }

      function renderOverview() {
        const metricsEl = document.getElementById("overview-metrics");
        const summaryEl = document.getElementById("overview-bootstrap-summary");
        metricsEl.replaceChildren();
        summaryEl.replaceChildren();

        if (!cachedWorkspace) {
          summaryEl.appendChild(makeEl("p", "meta-floor", "Connect with a valid grant credential in the top bar to load active projects, shelves, approval inbox items, and forward lessons."));
          return;
        }

        const shelves = Array.isArray(cachedWorkspace.shelves) ? cachedWorkspace.shelves : [];
        const projects = Array.isArray(cachedWorkspace.projects) ? cachedWorkspace.projects : [];
        const pendingCount = projects.filter(function (p) { return p.state === "DOC_PENDING_OWNER"; }).length;
        const totalItems = shelves.reduce(function (acc, s) { return acc + (s.total_count || 0); }, 0);

        [
          ["Principal", cachedWorkspace.principal_handle + " (" + cachedWorkspace.principal_role + ")"],
          ["Authorized Shelves", shelves.length + " shelves / " + totalItems + " resources"],
          ["Trio+ Projects", projects.length + " projects"],
          ["Owner Approval Inbox", pendingCount + " DOC_PENDING_OWNER"]
        ].forEach(function (pair) {
          const box = makeEl("div", "subcard");
          box.appendChild(makeEl("p", "meta-floor", pair[0]));
          box.appendChild(makeEl("strong", "", pair[1]));
          metricsEl.appendChild(box);
        });

        const tagsStr = Array.isArray(cachedWorkspace.granted_tags) ? cachedWorkspace.granted_tags.join(", ") : "";
        summaryEl.appendChild(makeEl("p", "", "Granted tags: " + (tagsStr || "none") + " | Write: " + cachedWorkspace.can_write + " | Dispatch: " + cachedWorkspace.can_dispatch));
        if (cachedBootstrap && Array.isArray(cachedBootstrap.required_onboarding_slugs)) {
          summaryEl.appendChild(makeEl("p", "meta-floor", "Onboarding slugs: " + cachedBootstrap.required_onboarding_slugs.join(", ")));
        }
      }

      async function refreshActiveView() {
        if (activeView === "overview") {
          renderOverview();
        } else if (activeView === "projects") {
          await loadProjectsView();
        } else if (activeView === "shelves") {
          await loadShelvesAndTriageView();
        } else if (activeView === "inbox") {
          await loadInboxView();
        } else if (activeView === "lessons") {
          await loadLessonsView();
        } else if (activeView === "apps") {
          await loadAppsView();
        }
      }

      async function loadProjectsView() {
        const res = await callTool("list_trio_projects", {});
        const listEl = document.getElementById("projects-list");
        listEl.replaceChildren();
        if (!res.ok) {
          return;
        }
        cachedProjects = Array.isArray(res.payload.data) ? res.payload.data : [];
        if (!cachedProjects.length) {
          const empty = makeEl("div", "empty-state");
          empty.appendChild(makeEl("p", "", "No Trio+ projects exist in your authorized scope yet."));
          const newBtn = makeEl("button", "btn btn-primary", "New Trio+ Project");
          newBtn.type = "button";
          newBtn.addEventListener("click", function () {
            document.getElementById("create-project-details").open = true;
            document.getElementById("proj-id-input").focus();
          });
          empty.appendChild(newBtn);
          listEl.appendChild(empty);
          return;
        }

        cachedProjects.forEach(function (proj) {
          const card = makeEl("div", "subcard");
          const top = makeEl("div", "action-row");
          const titleBtn = makeEl("button", "btn", proj.title + " (" + proj.project_id + ")");
          titleBtn.type = "button";
          titleBtn.addEventListener("click", function () { selectProject(proj.project_id); });
          top.appendChild(titleBtn);
          top.appendChild(makeEl("span", "badge badge-cyan", proj.profile + " D" + proj.difficulty + "/R" + proj.risk));
          top.appendChild(makeEl("span", proj.state === "DONE" ? "badge badge-green" : "badge badge-warn", proj.state));
          if (proj.plan_locked_rev !== null && proj.plan_locked_rev !== undefined) {
            top.appendChild(makeEl("span", "badge badge-green", "Plan Locked rev " + proj.plan_locked_rev));
          }
          card.appendChild(top);
          listEl.appendChild(card);
        });
      }

      async function selectProject(projectId) {
        const [bundleRes, runsRes] = await Promise.all([
          callTool("get_project_bundle", { project_id: projectId }),
          callTool("list_conveyor_runs", { project_id: projectId })
        ]);
        if (!bundleRes.ok || !bundleRes.payload.data) {
          return;
        }
        selectedProjectBundle = bundleRes.payload.data;
        selectedProjectRuns = (runsRes.ok && Array.isArray(runsRes.payload.data)) ? runsRes.payload.data : [];
        renderSelectedProject();
        if (selectedProjectBundle.resources && selectedProjectBundle.resources.length) {
          const preferred = selectedProjectBundle.resources.find(function (r) {
            return r.trio_role === "blueprint" || r.trio_role === "fix_record";
          }) || selectedProjectBundle.resources[0];
          await selectResource(preferred.shelf_slug, preferred.slug);
        } else {
          renderRightPanel();
        }
      }

      function renderSelectedProject() {
        const pane = document.getElementById("project-detail-pane");
        if (!selectedProjectBundle) {
          pane.hidden = true;
          return;
        }
        pane.hidden = false;
        const proj = selectedProjectBundle.project;
        const head = document.getElementById("project-detail-header");
        head.replaceChildren();
        head.appendChild(makeEl("h4", "", proj.title + " (" + proj.project_id + ")"));
        const badges = makeEl("div", "action-row");
        badges.appendChild(makeEl("span", "badge badge-cyan", proj.profile + " | D" + proj.difficulty + " R" + proj.risk));
        badges.appendChild(makeEl("span", proj.state === "DONE" ? "badge badge-green" : "badge badge-warn", proj.state));
        badges.appendChild(makeEl("span", proj.plan_locked_rev ? "badge badge-green" : "badge", proj.plan_locked_rev ? ("Locked rev " + proj.plan_locked_rev) : "Plan Unlocked"));
        badges.appendChild(makeEl("span", proj.cleanup_verified ? "badge badge-green" : "badge badge-warn", proj.cleanup_verified ? "Cleanup Verified" : "Cleanup Pending"));
        badges.appendChild(makeEl("span", proj.lessons_satisfied ? "badge badge-green" : "badge badge-warn", proj.lessons_satisfied ? "Lessons Recorded" : "Lessons Pending"));
        head.appendChild(badges);

        const ownBody = document.getElementById("own-table-body");
        ownBody.replaceChildren();
        selectedProjectBundle.own_decisions.forEach(function (own) {
          const tr = makeEl("tr");
          tr.appendChild(makeEl("td", "", own.own_id));
          tr.appendChild(makeEl("td", "", own.statement));
          tr.appendChild(makeEl("td", "", own.locked_at ? "Locked" : "Editable"));
          ownBody.appendChild(tr);
        });

        const divBody = document.getElementById("div-table-body");
        divBody.replaceChildren();
        selectedProjectBundle.divergences.forEach(function (divItem) {
          const tr = makeEl("tr");
          tr.appendChild(makeEl("td", "", divItem.div_id + " (" + divItem.status + ")"));
          tr.appendChild(makeEl("td", "", divItem.designed + " -> " + divItem.observed));
          tr.appendChild(makeEl("td", "", divItem.resolution));
          divBody.appendChild(tr);
        });

        const reqBody = document.getElementById("req-table-body");
        const reqSel = document.getElementById("req-id-select");
        reqBody.replaceChildren();
        reqSel.replaceChildren();
        selectedProjectBundle.requirements.forEach(function (reqRow) {
          const opt = makeEl("option", "", reqRow.req_id);
          opt.value = reqRow.req_id;
          reqSel.appendChild(opt);

          const tr = makeEl("tr");
          tr.appendChild(makeEl("td", "", reqRow.req_id + " (" + reqRow.source_id + ")"));
          tr.appendChild(makeEl("td", "", reqRow.statement));
          const stageTd = makeEl("td");
          const dital = makeEl("div", "dital-row");
          ["D", "I", "T", "A", "L"].forEach(function (letter) {
            const cls = reqRow.stage === letter ? "stage-pill stage-current" : "stage-pill";
            dital.appendChild(makeEl("span", cls, letter));
          });
          if (reqRow.stage === "UNKNOWN") {
            dital.appendChild(makeEl("span", "stage-pill stage-unknown", "UNKNOWN"));
          }
          stageTd.appendChild(dital);
          tr.appendChild(stageTd);
          tr.appendChild(makeEl("td", "", reqRow.evidence_ref || "-"));
          reqBody.appendChild(tr);
        });

        const docsList = document.getElementById("project-docs-list");
        docsList.replaceChildren();
        selectedProjectBundle.resources.forEach(function (r) {
          const docBtn = makeEl("button", "btn", (r.trio_role || r.slug) + " (rev " + r.current_rev + ")");
          docBtn.type = "button";
          docBtn.addEventListener("click", function () {
            selectResource(r.shelf_slug, r.slug);
          });
          docsList.appendChild(docBtn);
        });

        const runsBody = document.getElementById("conveyor-runs-body");
        runsBody.replaceChildren();
        selectedProjectRuns.forEach(function (runItem) {
          const tr = makeEl("tr");
          tr.appendChild(makeEl("td", "", runItem.run_id));
          tr.appendChild(makeEl("td", "", runItem.phase + " / " + runItem.family));
          tr.appendChild(makeEl("td", "", runItem.state));
          tr.appendChild(makeEl("td", "", "cleanup=" + runItem.cleanup_clean + ", scan=" + runItem.scan_clean));
          const inspectTd = makeEl("td");
          const inspectBtn = makeEl("button", "btn", "Status");
          inspectBtn.type = "button";
          inspectBtn.addEventListener("click", async function () {
            const st = await callTool("get_run_status", { run_id: runItem.run_id });
            if (st.ok && st.payload.data) {
              document.getElementById("conveyor-status-out").textContent = JSON.stringify(st.payload.data, null, 2);
            }
          });
          inspectTd.appendChild(inspectBtn);
          tr.appendChild(inspectTd);
          runsBody.appendChild(tr);
        });
      }

      async function loadShelvesAndTriageView() {
        const [shelfRes, listRes] = await Promise.all([
          callTool("list_shelves", {}),
          queryTriageResources()
        ]);
        const shelfGrid = document.getElementById("shelves-summary-grid");
        const shelfSelect = document.getElementById("triage-shelf-filter");
        shelfGrid.replaceChildren();

        if (shelfRes.ok && Array.isArray(shelfRes.payload.data)) {
          const prevVal = shelfSelect.value;
          shelfSelect.replaceChildren();
          const allOpt = makeEl("option", "", "All shelves");
          allOpt.value = "";
          shelfSelect.appendChild(allOpt);

          shelfRes.payload.data.forEach(function (s) {
            const opt = makeEl("option", "", s.title + " (" + s.slug + ")");
            opt.value = s.slug;
            if (s.slug === prevVal) { opt.selected = true; }
            shelfSelect.appendChild(opt);

            const card = makeEl("div", "subcard");
            const headBtn = makeEl("button", "btn", s.title + " (" + s.total_count + ")");
            headBtn.type = "button";
            headBtn.addEventListener("click", function () {
              shelfSelect.value = s.slug;
              queryTriageResources();
            });
            card.appendChild(headBtn);
            const counts = makeEl("div", "action-row");
            counts.style.marginTop = "8px";
            counts.appendChild(makeEl("span", "badge badge-green", "Verified: " + s.verified_count));
            counts.appendChild(makeEl("span", "badge badge-warn", "Stale: " + s.stale_count));
            counts.appendChild(makeEl("span", "badge badge-danger", "Drift: " + s.drift_count));
            counts.appendChild(makeEl("span", "badge badge-warn", "Unknown: " + s.unknown_count));
            card.appendChild(counts);
            shelfGrid.appendChild(card);
          });
        }
        if (listRes) {
          renderTriageList(listRes);
        }
      }

      async function queryTriageResources() {
        const shelfVal = document.getElementById("triage-shelf-filter").value;
        const searchVal = document.getElementById("triage-search-input").value.trim();
        const filterObj = {};
        if (shelfVal) { filterObj.shelf_slug = shelfVal; }
        if (triageTierFilter) { filterObj.lifecycle_tier = triageTierFilter; }
        if (triageNeedsOnly) { filterObj.triage_state = "needs_triage"; }

        if (searchVal) {
          filterObj.query = searchVal;
          const res = await callTool("search_resources", filterObj);
          if (res.ok && Array.isArray(res.payload.data)) {
            cachedResources = res.payload.data.map(function (hit) { return hit.resource; });
            renderTriageList(res.payload.data);
            return res.payload.data;
          }
          return [];
        } else {
          const res = await callTool("list_resources", filterObj);
          if (res.ok && Array.isArray(res.payload.data)) {
            cachedResources = res.payload.data;
            renderTriageList(cachedResources);
            return cachedResources;
          }
          return [];
        }
      }

      function renderTriageList(items) {
        const host = document.getElementById("triage-resource-list");
        host.replaceChildren();
        if (!Array.isArray(items) || !items.length) {
          const empty = makeEl("div", "empty-state");
          empty.appendChild(makeEl("p", "", "No resources matched the current shelf, lifecycle tier, or search filter."));
          const resetBtn = makeEl("button", "btn", "Reset Triage Filters");
          resetBtn.type = "button";
          resetBtn.addEventListener("click", function () {
            document.getElementById("triage-shelf-filter").value = "";
            document.getElementById("triage-search-input").value = "";
            triageTierFilter = "";
            triageNeedsOnly = false;
            updateTierFilterButtons();
            queryTriageResources();
          });
          empty.appendChild(resetBtn);
          host.appendChild(empty);
          return;
        }

        items.forEach(function (entry) {
          const res = entry.resource ? entry.resource : entry;
          const card = makeEl("div", "subcard");
          const row = makeEl("div", "action-row");
          const openBtn = makeEl("button", "btn", res.title + " (" + res.shelf_slug + "/" + res.slug + ")");
          openBtn.type = "button";
          openBtn.addEventListener("click", function () { selectResource(res.shelf_slug, res.slug); });
          row.appendChild(openBtn);
          row.appendChild(makeEl("span", "badge badge-cyan", res.lifecycle_tier));
          row.appendChild(makeEl("span", res.triage_state === "needs_triage" ? "badge badge-warn" : "badge badge-green", res.triage_state));
          card.appendChild(row);
          if (entry.snippet) {
            card.appendChild(makeEl("p", "meta-floor", entry.snippet));
          }

          const triageBtns = makeEl("div", "action-row");
          triageBtns.style.marginTop = "10px";
          [
            ["Keep Dormant", "dormant"],
            ["Promote to Active", "active"],
            ["Archive Retired", "retired"]
          ].forEach(function (spec) {
            const actBtn = makeEl("button", "btn", spec[0]);
            actBtn.type = "button";
            actBtn.addEventListener("click", async function () {
              const upd = await callTool("set_lifecycle_tier", {
                shelf_slug: res.shelf_slug,
                slug: res.slug,
                lifecycle_tier: spec[1],
                triage_state: "reviewed"
              });
              if (upd.ok) {
                await queryTriageResources();
              }
            });
            triageBtns.appendChild(actBtn);
          });
          card.appendChild(triageBtns);
          host.appendChild(card);
        });
      }

      function updateTierFilterButtons() {
        document.querySelectorAll(".tier-filter-btn").forEach(function (btn) {
          btn.classList.toggle("active-filter", btn.getAttribute("data-tier") === triageTierFilter);
        });
        const needsBtn = document.getElementById("triage-needs-btn");
        needsBtn.setAttribute("aria-pressed", triageNeedsOnly ? "true" : "false");
        needsBtn.classList.toggle("active-filter", triageNeedsOnly);
      }

      async function loadInboxView() {
        const listRes = await callTool("list_trio_projects", {});
        const host = document.getElementById("inbox-queue-list");
        host.replaceChildren();
        if (!listRes.ok) {
          return;
        }
        const allProjs = Array.isArray(listRes.payload.data) ? listRes.payload.data : [];
        const pending = allProjs.filter(function (p) { return p.state === "DOC_PENDING_OWNER"; });
        const badgeNode = document.getElementById("inbox-count-badge");
        if (badgeNode) { badgeNode.textContent = String(pending.length); }

        if (!pending.length) {
          const empty = makeEl("div", "empty-state");
          empty.appendChild(makeEl("p", "", "No projects are currently waiting in DOC_PENDING_OWNER."));
          const goBtn = makeEl("button", "btn btn-primary", "Open Trio+ Projects");
          goBtn.type = "button";
          goBtn.addEventListener("click", function () { switchView("projects"); });
          empty.appendChild(goBtn);
          host.appendChild(empty);
          return;
        }

        for (const proj of pending) {
          const card = makeEl("div", "subcard");
          card.appendChild(makeEl("h4", "", proj.title + " (" + proj.project_id + ")"));
          const badges = makeEl("div", "action-row");
          badges.appendChild(makeEl("span", "badge badge-warn", "DOC_PENDING_OWNER"));
          badges.appendChild(makeEl("span", proj.cleanup_verified ? "badge badge-green" : "badge badge-danger", proj.cleanup_verified ? "Cleanup Verified" : "Cleanup Unverified"));
          badges.appendChild(makeEl("span", proj.lessons_satisfied ? "badge badge-green" : "badge badge-danger", proj.lessons_satisfied ? "Central Lessons Satisfied" : "Lessons Missing"));
          card.appendChild(badges);

          const bundleRes = await callTool("get_project_bundle", { project_id: proj.project_id });
          if (bundleRes.ok && bundleRes.payload.data) {
            const bundle = bundleRes.payload.data;
            if (bundle.sync_reports && bundle.sync_reports.length) {
              const latest = bundle.sync_reports[0];
              badges.appendChild(makeEl("span", "badge badge-cyan", "Sync: " + latest.verdict + " (" + latest.acceptor_family + ")"));
            } else {
              badges.appendChild(makeEl("span", "badge badge-warn", "Sync: UNKNOWN"));
            }
            if (proj.how_it_works_resource_id) {
              const docRes = await callTool("read_resource", { resource_id: proj.how_it_works_resource_id });
              if (docRes.ok && docRes.payload.data) {
                const docContainer = makeEl("div", "workspace-output");
                docContainer.style.marginTop = "12px";
                renderMarkdownSafe(docContainer, docRes.payload.data.body_md || "");
                card.appendChild(docContainer);
              }
            }
          }

          const actions = makeEl("div", "action-row");
          actions.style.marginTop = "12px";
          const approveBtn = makeEl("button", "btn btn-primary", "Approve Release");
          approveBtn.type = "button";
          approveBtn.addEventListener("click", async function () {
            const appr = await callTool("approve_release", { project_id: proj.project_id });
            if (appr.ok) {
              await loadInboxView();
            }
          });
          const inspectBtn = makeEl("button", "btn", "Inspect Bundle");
          inspectBtn.type = "button";
          inspectBtn.addEventListener("click", function () {
            switchView("projects");
            selectProject(proj.project_id);
          });
          actions.appendChild(approveBtn);
          actions.appendChild(inspectBtn);
          card.appendChild(actions);
          host.appendChild(card);
        }
      }

      async function loadLessonsView() {
        const tagVal = document.getElementById("lessons-tag-filter").value.trim();
        const projVal = document.getElementById("lessons-project-filter").value.trim();
        const textVal = document.getElementById("lessons-text-filter").value.trim().toLowerCase();
        const args = { limit: 100 };
        if (tagVal) { args.tag_filter = tagVal; }
        if (projVal) { args.project_filter = projVal; }

        const res = await callTool("query_lessons", args);
        const host = document.getElementById("lessons-list");
        host.replaceChildren();
        if (!res.ok) { return; }

        let items = Array.isArray(res.payload.data) ? res.payload.data : [];
        if (textVal) {
          items = items.filter(function (l) {
            return String(l.forward_rule || "").toLowerCase().includes(textVal)
              || String(l.observed_fact || "").toLowerCase().includes(textVal)
              || String(l.cause_note || "").toLowerCase().includes(textVal);
          });
        }
        if (!items.length) {
          host.appendChild(makeEl("p", "meta-floor", "No central forward lessons matched the filter."));
          return;
        }
        items.forEach(function (l) {
          const card = makeEl("div", "subcard");
          const row = makeEl("div", "action-row");
          row.appendChild(makeEl("span", "badge badge-cyan", l.cause_kind + " / " + l.validation_state));
          if (l.project_id) {
            row.appendChild(makeEl("span", "badge", l.project_id));
          }
          if (Array.isArray(l.tags) && l.tags.length) {
            row.appendChild(makeEl("span", "badge badge-green", l.tags.join(", ")));
          }
          card.appendChild(row);
          card.appendChild(makeEl("p", "", "Forward Rule: " + l.forward_rule));
          card.appendChild(makeEl("p", "meta-floor", "Observed: " + l.observed_fact));
          if (l.cause_note) {
            card.appendChild(makeEl("p", "meta-floor", "Cause: " + l.cause_note));
          }
          host.appendChild(card);
        });
      }

      async function loadAppsView() {
        const res = await callTool("list_skills", {});
        const host = document.getElementById("skills-list");
        host.replaceChildren();
        if (!res.ok || !res.payload.data) {
          return;
        }
        const bundled = Array.isArray(res.payload.data.bundled_skills) ? res.payload.data.bundled_skills : [];
        bundled.forEach(function (sk) {
          const card = makeEl("div", "subcard");
          card.appendChild(makeEl("h4", "", sk.name + " (role: " + sk.role + ")"));
          card.appendChild(makeEl("p", "", sk.description || "Unmerged role skill"));
          card.appendChild(makeEl("p", "meta-floor", "sha256: " + String(sk.sha256 || "").slice(0, 12)));
          const readBtn = makeEl("button", "btn", "Read Skill");
          readBtn.type = "button";
          readBtn.addEventListener("click", function () {
            selectedResourceData = {
              resource: { title: sk.name, shelf_slug: "skills", slug: sk.role, lifecycle_tier: "active" },
              body_md: sk.body_md || ""
            };
            selectedResourceHistory = [];
            selectedResourcePassport = null;
            switchPanel("read", true);
          });
          card.appendChild(readBtn);
          host.appendChild(card);
        });
      }

      function gatherAssessmentPayload() {
        const optModules = [];
        document.querySelectorAll('input[name="opt_module"]:checked').forEach(function (box) {
          optModules.push(box.value);
        });
        return {
          assessment: {
            d1_scope: Number(document.getElementById("proj-d1").value || 0),
            d2_interfaces: Number(document.getElementById("proj-d2").value || 0),
            d3_uncertainty: Number(document.getElementById("proj-d3").value || 0),
            d4_ui: Number(document.getElementById("proj-d4").value || 0),
            d5_novelty: Number(document.getElementById("proj-d5").value || 0),
            r1_exposure: Number(document.getElementById("proj-r1").value || 0),
            r2_irreversibility: Number(document.getElementById("proj-r2").value || 0),
            r3_blast_radius: Number(document.getElementById("proj-r3").value || 0),
            r4_evidence: Number(document.getElementById("proj-r4").value || 0),
            r5_custody: Number(document.getElementById("proj-r5").value || 0),
            work_kind: document.getElementById("proj-kind-select").value,
            has_ui: document.getElementById("proj-has-ui").checked,
            is_game: document.getElementById("proj-is-game").checked,
            under_accepted_design: document.getElementById("proj-under-design").checked
          },
          opt_in_modules: optModules
        };
      }

      motionBtn.addEventListener("click", function () {
        const paused = document.body.classList.toggle("motion-enabled") === false;
        motionBtn.setAttribute("aria-pressed", paused ? "true" : "false");
        motionBtn.textContent = paused ? "Resume animation" : "Pause animation";
      });

      document.getElementById("dismiss-alert-btn").addEventListener("click", clearAlerts);

      document.getElementById("session-connect-form").addEventListener("submit", function (ev) {
        ev.preventDefault();
        const entered = credInput.value.trim();
        if (!entered) {
          showAlert("Enter a grant credential to connect.", false);
          return;
        }
        activeCredential = entered;
        window.sessionStorage.setItem(sessionSlotName, activeCredential);
        loadBootstrapAndWorkspace();
      });

      document.getElementById("disconnect-btn").addEventListener("click", function () {
        activeCredential = "";
        credInput.value = "";
        window.sessionStorage.removeItem(sessionSlotName);
        cachedWorkspace = null;
        cachedBootstrap = null;
        cachedProjects = [];
        selectedProjectBundle = null;
        selectedResourceData = null;
        ruleStatusEl.textContent = "Rule Gate: connect credential to inspect";
        hideRuleBanner();
        clearAlerts();
        setPresenceState("idle");
        renderOverview();
        renderRightPanel();
      });

      async function handleAckRulesClick() {
        const res = await callTool("ack_rules", { rules_sha256: activeRulesSha });
        if (res.ok) {
          ruleStatusEl.textContent = "Rule Gate: Acknowledged";
          hideRuleBanner();
          await loadBootstrapAndWorkspace();
        }
      }

      document.getElementById("ack-rules-btn").addEventListener("click", handleAckRulesClick);
      document.getElementById("ack-rules-banner-btn").addEventListener("click", handleAckRulesClick);

      document.getElementById("overview-quick-new-project").addEventListener("click", function () {
        switchView("projects");
        document.getElementById("create-project-details").open = true;
      });
      document.getElementById("overview-quick-inbox").addEventListener("click", function () { switchView("inbox"); });
      document.getElementById("overview-quick-shelves").addEventListener("click", function () { switchView("shelves"); });
      document.getElementById("overview-quick-lessons").addEventListener("click", function () { switchView("lessons"); });

      document.querySelectorAll(".nav-item").forEach(function (btn) {
        btn.addEventListener("click", function () {
          switchView(btn.getAttribute("data-view"));
        });
      });

      document.querySelectorAll(".panel-tab").forEach(function (tab) {
        tab.addEventListener("click", function (ev) {
          ev.preventDefault();
          switchPanel(tab.getAttribute("data-panel"), true);
        });
      });

      document.getElementById("assess-project-btn").addEventListener("click", async function () {
        const payload = gatherAssessmentPayload();
        const res = await callTool("assess_trio_work", payload);
        if (res.ok && res.payload.data) {
          const out = res.payload.data;
          document.getElementById("assess-preview").textContent =
            "Recommended Profile: " + out.profile + " (D" + out.difficulty + "/R" + out.risk + ") | Roles: " + (out.expected_roles || []).join(", ");
        }
      });

      document.getElementById("create-project-form").addEventListener("submit", async function (ev) {
        ev.preventDefault();
        const pid = document.getElementById("proj-id-input").value.trim();
        const title = document.getElementById("proj-title-input").value.trim();
        const base = gatherAssessmentPayload();
        const ownStmt = document.getElementById("proj-own1-input").value.trim();
        const reqStmt = document.getElementById("proj-req1-input").value.trim();
        const args = {
          project_id: pid,
          request_id: "ui-" + pid + "-" + Date.now(),
          title: title,
          assessment: base.assessment,
          opt_in_modules: base.opt_in_modules,
          initial_own_decisions: ownStmt ? [{ own_id: "OWN-001", statement: ownStmt }] : [],
          initial_requirements: reqStmt ? [{ req_id: "REQ-01", source_id: ownStmt ? "OWN-001" : "PLAN", statement: reqStmt }] : []
        };
        const res = await callTool("create_trio_project", args);
        if (res.ok) {
          await loadProjectsView();
          await selectProject(pid);
        }
      });

      document.getElementById("lock-plan-btn").addEventListener("click", async function () {
        if (!selectedProjectBundle) { return; }
        const res = await callTool("lock_project_plan", { project_id: selectedProjectBundle.project.project_id });
        if (res.ok) {
          await selectProject(selectedProjectBundle.project.project_id);
        }
      });

      document.getElementById("compare-plan-btn").addEventListener("click", async function () {
        if (!selectedProjectBundle) { return; }
        const docs = selectedProjectBundle.resources || [];
        const leftDoc = docs.find(function (d) { return d.trio_role === "blueprint" || d.trio_role === "fix_record"; });
        const rightDoc = docs.find(function (d) { return d.trio_role === "as_built"; });
        let cmpArgs = null;
        if (leftDoc && rightDoc) {
          cmpArgs = { left_resource_id: leftDoc.id, right_resource_id: rightDoc.id };
        } else if (docs.length >= 2) {
          cmpArgs = { left_resource_id: docs[0].id, right_resource_id: docs[1].id };
        } else if (docs.length === 1 && docs[0].current_rev >= 2) {
          cmpArgs = { shelf_slug: docs[0].shelf_slug, slug: docs[0].slug, left_rev: 1, right_rev: docs[0].current_rev };
        } else {
          showAlert("At least two documents or two revisions are required to compare.", false);
          return;
        }
        const cmpRes = await callTool("compare_resources", cmpArgs);
        if (cmpRes.ok && cmpRes.payload.data) {
          renderDiffViewer(cmpRes.payload.data, "Project Plan vs As-Built Diff");
        }
      });

      document.getElementById("check-sync-btn").addEventListener("click", async function () {
        if (!selectedProjectBundle) { return; }
        const res = await callTool("check_sync", { project_id: selectedProjectBundle.project.project_id });
        if (res.ok && res.payload.data) {
          document.getElementById("conveyor-status-out").textContent = JSON.stringify(res.payload.data, null, 2);
        }
      });

      document.getElementById("own-add-form").addEventListener("submit", async function (ev) {
        ev.preventDefault();
        if (!selectedProjectBundle) { return; }
        const res = await callTool("upsert_own_decision", {
          project_id: selectedProjectBundle.project.project_id,
          own_id: document.getElementById("own-id-input").value.trim(),
          statement: document.getElementById("own-stmt-input").value.trim()
        });
        if (res.ok) {
          await selectProject(selectedProjectBundle.project.project_id);
        }
      });

      document.getElementById("div-add-form").addEventListener("submit", async function (ev) {
        ev.preventDefault();
        if (!selectedProjectBundle) { return; }
        const res = await callTool("record_divergence", {
          project_id: selectedProjectBundle.project.project_id,
          designed: document.getElementById("div-designed-input").value.trim(),
          observed: document.getElementById("div-observed-input").value.trim(),
          resolution: document.getElementById("div-resolution-input").value.trim(),
          status: document.getElementById("div-status-select").value
        });
        if (res.ok) {
          await selectProject(selectedProjectBundle.project.project_id);
        }
      });

      document.getElementById("req-advance-form").addEventListener("submit", async function (ev) {
        ev.preventDefault();
        if (!selectedProjectBundle) { return; }
        const familyVal = document.getElementById("req-family-input").value.trim();
        const args = {
          project_id: selectedProjectBundle.project.project_id,
          req_id: document.getElementById("req-id-select").value,
          target_stage: document.getElementById("req-stage-select").value,
          evidence_ref: document.getElementById("req-evidence-input").value.trim()
        };
        if (familyVal) { args.actor_family = familyVal; }
        const res = await callTool("update_requirement_stage", args);
        if (res.ok) {
          await selectProject(selectedProjectBundle.project.project_id);
        }
      });

      document.getElementById("conveyor-dispatch-form").addEventListener("submit", async function (ev) {
        ev.preventDefault();
        if (!selectedProjectBundle) { return; }
        const rawPaths = document.getElementById("conveyor-inputs").value
          .split(",")
          .map(function (s) { return s.trim(); })
          .filter(Boolean);
        const builder = document.getElementById("conveyor-builder-harness").value.trim();
        const acceptor = document.getElementById("conveyor-acceptor-harness").value.trim();
        const args = {
          project_id: selectedProjectBundle.project.project_id,
          input_paths: rawPaths
        };
        if (builder) { args.preferred_builder_harness = builder; }
        if (acceptor) { args.preferred_acceptor_harness = acceptor; }
        const res = await callTool("dispatch_run", args);
        if (res.ok) {
          document.getElementById("conveyor-status-out").textContent = JSON.stringify(res.payload.data, null, 2);
          await selectProject(selectedProjectBundle.project.project_id);
        }
      });

      document.getElementById("closeout-cleanup-btn").addEventListener("click", async function () {
        if (!selectedProjectBundle) { return; }
        const res = await callTool("mark_project_cleanup_verified", {
          project_id: selectedProjectBundle.project.project_id,
          detail: "Verified clean via Streamline Web UI"
        });
        if (res.ok) {
          await selectProject(selectedProjectBundle.project.project_id);
        }
      });

      document.getElementById("closeout-how-form").addEventListener("submit", async function (ev) {
        ev.preventDefault();
        if (!selectedProjectBundle) { return; }
        const titleVal = document.getElementById("how-title-input").value.trim();
        const args = {
          project_id: selectedProjectBundle.project.project_id,
          markdown: document.getElementById("how-md-input").value
        };
        if (titleVal) { args.title = titleVal; }
        const res = await callTool("submit_how_it_works", args);
        if (res.ok) {
          await selectProject(selectedProjectBundle.project.project_id);
        }
      });

      document.querySelectorAll(".tier-filter-btn").forEach(function (btn) {
        btn.addEventListener("click", function () {
          triageTierFilter = btn.getAttribute("data-tier");
          updateTierFilterButtons();
          if (activeCredential) { queryTriageResources(); }
        });
      });

      document.getElementById("triage-needs-btn").addEventListener("click", function () {
        triageNeedsOnly = !triageNeedsOnly;
        updateTierFilterButtons();
        if (activeCredential) { queryTriageResources(); }
      });

      document.getElementById("triage-shelf-filter").addEventListener("change", function () {
        if (activeCredential) { queryTriageResources(); }
      });

      document.getElementById("triage-search-form").addEventListener("submit", function (ev) {
        ev.preventDefault();
        if (activeCredential) { queryTriageResources(); }
      });

      document.getElementById("triage-search-input").addEventListener("input", function () {
        if (!activeCredential) { return; }
        if (triageSearchTimer) { clearTimeout(triageSearchTimer); }
        triageSearchTimer = setTimeout(function () { queryTriageResources(); }, 220);
      });

      document.getElementById("inbox-refresh-btn").addEventListener("click", function () {
        if (activeCredential) { loadInboxView(); }
      });

      document.getElementById("lessons-filter-form").addEventListener("submit", function (ev) {
        ev.preventDefault();
        if (activeCredential) { loadLessonsView(); }
      });

      document.getElementById("record-lesson-form").addEventListener("submit", async function (ev) {
        ev.preventDefault();
        const pid = document.getElementById("lesson-project-input").value.trim();
        const tags = document.getElementById("lesson-tags-input").value
          .split(",")
          .map(function (t) { return t.trim(); })
          .filter(Boolean);
        const args = {
          tags: tags,
          observed_fact: document.getElementById("lesson-fact-input").value.trim(),
          cause_kind: document.getElementById("lesson-cause-kind").value,
          cause_note: document.getElementById("lesson-cause-note").value.trim(),
          forward_rule: document.getElementById("lesson-rule-input").value.trim(),
          validation_state: document.getElementById("lesson-validation-state").value
        };
        if (pid) { args.project_id = pid; }
        const res = await callTool("record_lesson", args);
        if (res.ok) {
          document.getElementById("record-lesson-form").reset();
          await loadLessonsView();
        }
      });

      document.getElementById("tech-ready-btn").addEventListener("click", async function () {
        const resp = await fetch("/ready");
        const data = await resp.json();
        document.getElementById("tech-diagnostics-out").textContent = JSON.stringify(data, null, 2);
      });

      document.getElementById("tech-sync-vault-btn").addEventListener("click", async function () {
        const res = await callTool("sync_vault", {});
        document.getElementById("tech-diagnostics-out").textContent = JSON.stringify(res.payload, null, 2);
      });

      document.getElementById("tech-export-workspace-btn").addEventListener("click", async function () {
        const res = await callTool("export_workspace", { include_bodies: true });
        document.getElementById("tech-diagnostics-out").textContent = JSON.stringify(res.payload, null, 2);
      });

      document.getElementById("tech-export-trajectory-btn").addEventListener("click", async function () {
        if (!selectedProjectBundle) {
          showAlert("Select a project first in Projects (Trio+) before exporting its trajectory.", false);
          return;
        }
        const res = await callTool("export_trajectory", { project_id: selectedProjectBundle.project.project_id });
        document.getElementById("tech-diagnostics-out").textContent = JSON.stringify(res.payload, null, 2);
      });

      const savedCred = window.sessionStorage.getItem(sessionSlotName);
      if (savedCred) {
        activeCredential = savedCred;
        credInput.value = savedCred;
        loadBootstrapAndWorkspace();
      }
      renderRightPanel();
    })();
"####;

/// Serves the Streamline dark-palette 3-pane interactive workspace shell.
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
  <style>{ui_css}</style>
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
          <li><button type="button" class="nav-item active" data-view="overview">Overview</button></li>
          <li><button type="button" class="nav-item" data-view="projects">Projects (Trio+)</button></li>
          <li><button type="button" class="nav-item" data-view="shelves">Shelves &amp; Legacy Triage</button></li>
          <li><button type="button" class="nav-item" data-view="inbox"><span>Approval Inbox (DOC-PENDING-OWNER)</span><span class="badge badge-warn" id="inbox-count-badge">0</span></button></li>
          <li><button type="button" class="nav-item" data-view="lessons">Central Lessons</button></li>
          <li><button type="button" class="nav-item" data-view="apps">Skills &amp; Connected Apps</button></li>
        </ul>
      </nav>
    </aside>

    <main id="main-content" class="main-region" tabindex="-1">
      <header class="top-bar">
        <div>
          <h2 style="margin:0;font-size:22px;">Governance, Central Lessons, and Trio+ Server</h2>
          <p class="meta-floor">Endpoints: <span class="code-inline">/ready</span>, <span class="code-inline">/mcp</span>, <span class="code-inline">/api/tools/:tool_name</span>, <span class="code-inline">/downloads/streamline-skills.zip</span></p>
        </div>
        <form id="session-connect-form" class="action-row">
          <label for="cred-input" class="meta-floor">Session credential</label>
          <input id="cred-input" class="input-control masked-input" type="text" autocomplete="off" spellcheck="false" placeholder="Enter grant credential">
          <button type="submit" class="btn btn-primary" id="connect-btn">Connect &amp; Check Rules</button>
          <button type="button" class="btn" id="disconnect-btn">Clear Session</button>
          <a class="btn" href="/downloads/streamline-skills.zip">Download Skills ZIP</a>
        </form>
      </header>

      <div id="rule-gate-banner" class="banner" role="alert" hidden>
        <div>
          <strong>412 RULES_ACK_REQUIRED:</strong> Acknowledge the active governance digest (<span class="code-inline" id="rule-gate-banner-sha">{full_sha}</span>) before writing or dispatching runs.
        </div>
        <button type="button" class="btn btn-primary" id="ack-rules-banner-btn">Acknowledge Active Rules</button>
      </div>

      <div id="alert-banner" class="banner banner-danger" role="alert" hidden>
        <span id="alert-banner-msg"></span>
        <button type="button" class="btn" id="dismiss-alert-btn">Dismiss</button>
      </div>

      <div class="action-row" style="justify-content:space-between;">
        <span class="meta-floor">Workspace status</span>
        <span class="badge badge-cyan" id="workspace-status-pill">Idle</span>
      </div>

      <section class="view-panel list-stack" data-view-panel="overview" id="view-overview">
        <div class="grid-cards" aria-label="Server overview">
          <article class="card">
            <h3 style="margin-top:0;">Rule Gate Status</h3>
            <p>Callers acknowledge the active <span class="code-inline">standards/rules</span> SHA-256 digest before any write or conveyor dispatch.</p>
            <button type="button" class="btn btn-primary" id="ack-rules-btn">Acknowledge Active Rules</button>
          </article>
          <article class="card">
            <h3 style="margin-top:0;">Quick Workspace Actions</h3>
            <div class="action-row">
              <button type="button" class="btn" id="overview-quick-new-project">New Trio+ Project</button>
              <button type="button" class="btn" id="overview-quick-shelves">Triage Legacy</button>
              <button type="button" class="btn" id="overview-quick-inbox">Owner Inbox</button>
              <button type="button" class="btn" id="overview-quick-lessons">Central Lessons</button>
            </div>
          </article>
          <article class="card">
            <h3 style="margin-top:0;">Redaction &amp; Tag Guard</h3>
            <p>All writes pass through <span class="code-inline">streamline-safe</span> and respect tag-scoped grants.</p>
            <a class="btn" href="/ready" target="_blank" rel="noopener">Inspect /ready JSON</a>
          </article>
        </div>

        <article class="card">
          <h3 style="margin-top:0;">Live Workspace Snapshot</h3>
          <div id="overview-metrics" class="grid-cards"></div>
          <div id="overview-bootstrap-summary" style="margin-top:14px;"></div>
        </article>
      </section>

      <section class="view-panel list-stack" data-view-panel="projects" id="view-projects" hidden>
        <article class="card">
          <div class="action-row" style="justify-content:space-between;">
            <h3 style="margin:0;">Trio+ Projects (P0 to P4)</h3>
            <span class="meta-floor">D -&gt; I -&gt; T -&gt; A -&gt; L lifecycle</span>
          </div>

          <details id="create-project-details" style="margin-top:14px;">
            <summary class="btn btn-primary" style="width:fit-content;">New Trio+ Project (P0 to P4)</summary>
            <form id="create-project-form" class="list-stack" style="margin-top:16px;">
              <div class="form-grid">
                <div class="field-group">
                  <label for="proj-id-input" class="meta-floor">Project ID (slug)</label>
                  <input id="proj-id-input" class="input-control" type="text" required placeholder="project-alpha">
                </div>
                <div class="field-group">
                  <label for="proj-title-input" class="meta-floor">Project Title</label>
                  <input id="proj-title-input" class="input-control" type="text" required placeholder="Project Alpha">
                </div>
                <div class="field-group">
                  <label for="proj-kind-select" class="meta-floor">Work Kind</label>
                  <select id="proj-kind-select" class="select-control">
                    <option value="software">software</option>
                    <option value="council">council</option>
                    <option value="procedure">procedure</option>
                    <option value="general">general</option>
                  </select>
                </div>
              </div>

              <div class="form-grid">
                <div class="field-group"><label for="proj-d1" class="meta-floor">D1 Scope (0..4)</label><input id="proj-d1" class="input-control" type="number" min="0" max="4" value="1"></div>
                <div class="field-group"><label for="proj-d2" class="meta-floor">D2 Interfaces</label><input id="proj-d2" class="input-control" type="number" min="0" max="4" value="1"></div>
                <div class="field-group"><label for="proj-d3" class="meta-floor">D3 Uncertainty</label><input id="proj-d3" class="input-control" type="number" min="0" max="4" value="0"></div>
                <div class="field-group"><label for="proj-d4" class="meta-floor">D4 UI Surface</label><input id="proj-d4" class="input-control" type="number" min="0" max="4" value="0"></div>
                <div class="field-group"><label for="proj-d5" class="meta-floor">D5 Novelty</label><input id="proj-d5" class="input-control" type="number" min="0" max="4" value="1"></div>
              </div>

              <div class="form-grid">
                <div class="field-group"><label for="proj-r1" class="meta-floor">R1 Exposure (0..4)</label><input id="proj-r1" class="input-control" type="number" min="0" max="4" value="1"></div>
                <div class="field-group"><label for="proj-r2" class="meta-floor">R2 Irreversibility</label><input id="proj-r2" class="input-control" type="number" min="0" max="4" value="1"></div>
                <div class="field-group"><label for="proj-r3" class="meta-floor">R3 Blast Radius</label><input id="proj-r3" class="input-control" type="number" min="0" max="4" value="0"></div>
                <div class="field-group"><label for="proj-r4" class="meta-floor">R4 Evidence</label><input id="proj-r4" class="input-control" type="number" min="0" max="4" value="1"></div>
                <div class="field-group"><label for="proj-r5" class="meta-floor">R5 Custody</label><input id="proj-r5" class="input-control" type="number" min="0" max="4" value="1"></div>
              </div>

              <div class="action-row">
                <label class="checkbox-label"><input id="proj-has-ui" type="checkbox"> Includes UI / UX (P2)</label>
                <label class="checkbox-label"><input id="proj-is-game" type="checkbox"> Game / Simulation (P3)</label>
                <label class="checkbox-label"><input id="proj-under-design" type="checkbox"> Fix under accepted design (P0)</label>
              </div>

              <details id="optional-modules-details">
                <summary class="btn" style="width:fit-content;">Optional Tracking Modules (collapsed by default)</summary>
                <div class="action-row" style="margin-top:10px;">
                  <label class="checkbox-label"><input type="checkbox" name="opt_module" value="schedule"> schedule</label>
                  <label class="checkbox-label"><input type="checkbox" name="opt_module" value="ux"> ux</label>
                  <label class="checkbox-label"><input type="checkbox" name="opt_module" value="game"> game</label>
                  <label class="checkbox-label"><input type="checkbox" name="opt_module" value="budget"> budget</label>
                </div>
              </details>

              <div class="form-grid">
                <div class="field-group">
                  <label for="proj-own1-input" class="meta-floor">Initial OWN-001 Decision</label>
                  <input id="proj-own1-input" class="input-control" type="text" placeholder="Keep service bound to 127.0.0.1">
                </div>
                <div class="field-group">
                  <label for="proj-req1-input" class="meta-floor">Initial REQ-01 Statement</label>
                  <input id="proj-req1-input" class="input-control" type="text" placeholder="Verify health endpoint and redaction">
                </div>
              </div>

              <p class="meta-floor" id="assess-preview">Preview difficulty, risk, and P0..P4 profile before creating.</p>
              <div class="action-row">
                <button type="button" class="btn" id="assess-project-btn">Assess Profile</button>
                <button type="submit" class="btn btn-primary" id="create-project-submit-btn">Create Trio+ Project</button>
              </div>
            </form>
          </details>

          <div id="projects-list" class="list-stack" style="margin-top:16px;"></div>
        </article>

        <article class="card" id="project-detail-pane">
          <div id="project-detail-header">
            <p class="meta-floor">Select a Trio+ project above to lock the plan, compare diffs, inspect OWN vs DIV rows, advance D -&gt; I -&gt; T -&gt; A -&gt; L, or dispatch the hands-off conveyor.</p>
          </div>

          <div class="action-row" style="margin-top:14px;">
            <button type="button" class="btn btn-primary" id="lock-plan-btn">Lock Plan Version</button>
            <button type="button" class="btn" id="compare-plan-btn">Compare</button>
            <button type="button" class="btn" id="check-sync-btn">Check Sync</button>
          </div>

          <div id="diff-viewer-pane" class="subcard" style="margin-top:14px;" hidden>
            <p id="diff-summary-text" class="meta-floor"></p>
            <div id="diff-side-by-side" class="diff-grid"></div>
            <div id="diff-changed-words" class="word-diff-strip"></div>
          </div>

          <div class="split-grid" style="margin-top:16px;">
            <div class="subcard">
              <h4 style="margin-top:0;">OWN Decisions (Locked after Plan Lock)</h4>
              <table class="data-table">
                <thead><tr><th>ID</th><th>Statement</th><th>Lock</th></tr></thead>
                <tbody id="own-table-body"></tbody>
              </table>
              <form id="own-add-form" class="action-row" style="margin-top:10px;">
                <input id="own-id-input" class="input-control" type="text" placeholder="OWN-002" required>
                <input id="own-stmt-input" class="input-control" type="text" placeholder="Decision statement" required>
                <button type="submit" class="btn">Save OWN</button>
              </form>
            </div>

            <div class="subcard">
              <h4 style="margin-top:0;">DIV Divergences (Append-Only)</h4>
              <table class="data-table">
                <thead><tr><th>ID</th><th>Designed -&gt; Observed</th><th>Resolution</th></tr></thead>
                <tbody id="div-table-body"></tbody>
              </table>
              <form id="div-add-form" class="list-stack" style="margin-top:10px;">
                <input id="div-designed-input" class="input-control" type="text" placeholder="Designed expectation" required>
                <input id="div-observed-input" class="input-control" type="text" placeholder="Observed reality" required>
                <input id="div-resolution-input" class="input-control" type="text" placeholder="Resolution" required>
                <div class="action-row">
                  <select id="div-status-select" class="select-control">
                    <option value="open">open</option>
                    <option value="accepted">accepted</option>
                    <option value="fixed">fixed</option>
                  </select>
                  <button type="submit" class="btn">Record Divergence</button>
                </div>
              </form>
            </div>
          </div>

          <div class="subcard" style="margin-top:16px;">
            <h4 style="margin-top:0;">Requirements Trace (D -&gt; I -&gt; T -&gt; A -&gt; L)</h4>
            <table class="data-table">
              <thead><tr><th>ID</th><th>Statement</th><th>Stage</th><th>Evidence</th></tr></thead>
              <tbody id="req-table-body"></tbody>
            </table>
            <form id="req-advance-form" class="action-row" style="margin-top:10px;">
              <select id="req-id-select" class="select-control"></select>
              <select id="req-stage-select" class="select-control">
                <option value="I">I (Implemented)</option>
                <option value="T">T (Tested)</option>
                <option value="A">A (Accepted)</option>
                <option value="L">L (Live)</option>
                <option value="UNKNOWN">UNKNOWN</option>
              </select>
              <input id="req-evidence-input" class="input-control" type="text" placeholder="Evidence ref" required>
              <input id="req-family-input" class="input-control" type="text" placeholder="Actor family (e.g. google-gemini)">
              <button type="submit" class="btn">Advance Stage</button>
            </form>
          </div>

          <div class="subcard" style="margin-top:16px;">
            <h4 style="margin-top:0;">Project Documents &amp; Hands-Off Conveyor</h4>
            <div id="project-docs-list" class="action-row"></div>
            <form id="conveyor-dispatch-form" class="action-row" style="margin-top:12px;">
              <input id="conveyor-inputs" class="input-control" type="text" placeholder="Input file paths (comma-separated)">
              <input id="conveyor-builder-harness" class="input-control" type="text" placeholder="Builder harness (optional)">
              <input id="conveyor-acceptor-harness" class="input-control" type="text" placeholder="Acceptor harness (optional)">
              <button type="submit" class="btn btn-primary">Dispatch Conveyor Run</button>
            </form>
            <table class="data-table" style="margin-top:10px;">
              <thead><tr><th>Run ID</th><th>Phase / Family</th><th>State</th><th>Gates</th><th>Inspect</th></tr></thead>
              <tbody id="conveyor-runs-body"></tbody>
            </table>
            <pre id="conveyor-status-out" class="workspace-output" style="margin-top:10px;">Select a run or Check Sync to inspect structured output.</pre>
          </div>

          <div class="subcard" style="margin-top:16px;">
            <h4 style="margin-top:0;">3-Part Closeout Gate</h4>
            <div class="action-row">
              <button type="button" class="btn" id="closeout-cleanup-btn">1. Mark Cleanup Verified</button>
              <span class="meta-floor">2. Record central lesson in Central Lessons tab -&gt; 3. Submit HOW-IT-WORKS below</span>
            </div>
            <form id="closeout-how-form" class="list-stack" style="margin-top:10px;">
              <input id="how-title-input" class="input-control" type="text" placeholder="Optional HOW-IT-WORKS title">
              <textarea id="how-md-input" class="textarea-control" placeholder="Include ## Overview and Purpose, ## Runtime Location, ## Step-by-Step Flow, ## Health and Logs, ## Failure Modes and Recovery"></textarea>
              <button type="submit" class="btn btn-primary" style="width:fit-content;">Submit HOW-IT-WORKS (DOC_PENDING_OWNER)</button>
            </form>
          </div>
        </article>
      </section>

      <section class="view-panel list-stack" data-view-panel="shelves" id="view-shelves" hidden>
        <article class="card">
          <h3 style="margin-top:0;">Shelves &amp; Legacy Triage</h3>
          <div id="shelves-summary-grid" class="grid-cards"></div>
        </article>

        <article class="card">
          <div class="action-row" id="triage-controls" style="justify-content:space-between;">
            <div class="action-row" role="group" aria-label="Lifecycle tier filter">
              <button type="button" class="btn tier-filter-btn active-filter" data-tier="">All</button>
              <button type="button" class="btn tier-filter-btn" data-tier="active">Active</button>
              <button type="button" class="btn tier-filter-btn" data-tier="dormant">Dormant</button>
              <button type="button" class="btn tier-filter-btn" data-tier="legacy">Legacy</button>
              <button type="button" class="btn tier-filter-btn" data-tier="retired">Retired</button>
              <button type="button" class="btn" id="triage-needs-btn" aria-pressed="false">Needs Triage</button>
            </div>
            <form id="triage-search-form" class="action-row">
              <select id="triage-shelf-filter" class="select-control" aria-label="Filter by shelf">
                <option value="">All shelves</option>
              </select>
              <input id="triage-search-input" class="input-control" type="search" placeholder="Search shelves or legacy items (e.g. Furnace)">
              <button type="submit" class="btn btn-primary" id="triage-search-btn">Search</button>
            </form>
          </div>
          <p class="meta-floor" style="margin-top:10px;">Triage actions: Keep Dormant, Promote to Active, or Archive Retired via <span class="code-inline">/api/tools/set_lifecycle_tier</span>.</p>
          <div id="triage-resource-list" class="list-stack" style="margin-top:14px;"></div>
        </article>
      </section>

      <section class="view-panel list-stack" data-view-panel="inbox" id="view-inbox" hidden>
        <article class="card">
          <div class="action-row" style="justify-content:space-between;">
            <div>
              <h3 style="margin:0;">Owner Approval Inbox (DOC-PENDING-OWNER)</h3>
              <p class="meta-floor">Review plain-English HOW-IT-WORKS guides alongside cleanup and central-lesson badges before approving release.</p>
            </div>
            <button type="button" class="btn" id="inbox-refresh-btn">Refresh Inbox</button>
          </div>
          <div id="inbox-queue-list" class="list-stack" style="margin-top:16px;"></div>
        </article>
      </section>

      <section class="view-panel list-stack" data-view-panel="lessons" id="view-lessons" hidden>
        <article class="card">
          <h3 style="margin-top:0;">Central Lessons Learned</h3>
          <form id="lessons-filter-form" class="action-row">
            <input id="lessons-tag-filter" class="input-control" type="text" placeholder="Filter by tag">
            <input id="lessons-project-filter" class="input-control" type="text" placeholder="Filter by project_id">
            <input id="lessons-text-filter" class="input-control" type="search" placeholder="Instant text filter">
            <button type="submit" class="btn btn-primary">Query Central Lessons</button>
          </form>
          <div id="lessons-list" class="list-stack" style="margin-top:14px;"></div>
        </article>

        <article class="card">
          <h3 style="margin-top:0;">Record Central Forward Lesson</h3>
          <form id="record-lesson-form" class="list-stack">
            <div class="form-grid">
              <input id="lesson-project-input" class="input-control" type="text" placeholder="Project ID (optional)">
              <input id="lesson-tags-input" class="input-control" type="text" placeholder="Tags (comma-separated)">
              <select id="lesson-cause-kind" class="select-control">
                <option value="demonstrated">demonstrated</option>
                <option value="hypothesis">hypothesis</option>
              </select>
              <select id="lesson-validation-state" class="select-control">
                <option value="worked_once">worked_once</option>
                <option value="verified_reusable">verified_reusable</option>
              </select>
            </div>
            <input id="lesson-fact-input" class="input-control" type="text" placeholder="Observed fact" required>
            <input id="lesson-cause-note" class="input-control" type="text" placeholder="Root cause note">
            <input id="lesson-rule-input" class="input-control" type="text" placeholder="Forward rule for future runs" required>
            <button type="submit" class="btn btn-primary" style="width:fit-content;">Record Lesson</button>
          </form>
        </article>
      </section>

      <section class="view-panel list-stack" data-view-panel="apps" id="view-apps" hidden>
        <article class="card">
          <div class="action-row" style="justify-content:space-between;">
            <div>
              <h3 style="margin:0;">Portable Skills &amp; Unmerged Role Voices</h3>
              <p class="meta-floor">Download all 4 role skills (<span class="code-inline">director</span>, <span class="code-inline">worker</span>, <span class="code-inline">acceptor</span>, <span class="code-inline">vault</span>) as a portable ZIP.</p>
            </div>
            <a class="btn btn-primary" href="/downloads/streamline-skills.zip">Download streamline-skills.zip</a>
          </div>
          <div id="skills-list" class="grid-cards" style="margin-top:16px;"></div>
        </article>

        <article class="card">
          <details id="technical-details">
            <summary class="btn" style="width:fit-content;">Technical details</summary>
            <div class="list-stack" style="margin-top:14px;">
              <p class="meta-floor">MCP HTTP: <span class="code-inline">POST /mcp</span> | REST Mirror: <span class="code-inline">POST /api/tools/:tool_name</span> | Stdio: <span class="code-inline">streamline mcp-stdio</span></p>
              <div class="action-row">
                <button type="button" class="btn" id="tech-ready-btn">Inspect /ready JSON</button>
                <button type="button" class="btn" id="tech-sync-vault-btn">Sync Obsidian Vault</button>
                <button type="button" class="btn" id="tech-export-workspace-btn">Export Workspace Archive</button>
                <button type="button" class="btn" id="tech-export-trajectory-btn">Export Project Trajectory</button>
              </div>
              <pre id="tech-diagnostics-out" class="workspace-output">Technical diagnostics output appears here.</pre>
            </div>
          </details>
        </article>
      </section>

      <footer class="footer-stamp">
        Last updated: <span id="last-updated-stamp">Ready at startup</span> | Apache-2.0 Streamline Server
      </footer>
    </main>

    <aside class="right-panel" aria-label="Docked inspection panel">
      <div class="panel-tabs" role="tablist" aria-label="Right panel tabs">
        <a href="?panel=read" class="panel-tab {read_active}" data-panel="read">Read</a>
        <a href="?panel=history" class="panel-tab {history_active}" data-panel="history">History &amp; Restore</a>
        <a href="?panel=related" class="panel-tab {related_active}" data-panel="related">Related</a>
        <a href="?panel=passport" class="panel-tab {passport_active}" data-panel="passport">Source Passport</a>
      </div>
      <div id="right-panel-body" class="workspace-output">Active panel: {active_panel}. Select a project or resource after connecting to inspect markdown content, revision history, Trio+ trace rows, or Source Passport metadata.</div>
    </aside>
  </div>

  <script>{ui_js}</script>
  {presence_script_tag}
</body>
</html>
"##,
        ui_css = UI_CSS,
        ui_js = UI_JS,
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
