use axum::extract::{Query, State};
use axum::response::Html;
use serde::Deserialize;

use crate::state::AppState;

#[derive(Debug, Deserialize, Default)]
pub struct UiQueryParams {
    pub panel: Option<String>,
}

const UI_CSS: &str = r###"@font-face {
  font-family: 'Bricolage Grotesque';
  font-style: normal;
  font-weight: 600 700;
  font-stretch: 100%;
  font-display: swap;
  src: url('/_streamline/assets/fonts/bricolage-grotesque-latin-ext.woff2') format('woff2');
  unicode-range: U+0100-02BA, U+02BD-02C5, U+02C7-02CC, U+02CE-02D7, U+02DD-02FF, U+0304, U+0308, U+0329, U+1D00-1DBF, U+1E00-1E9F, U+1EF2-1EFF, U+2020, U+20A0-20AB, U+20AD-20C4, U+2113, U+2C60-2C7F, U+A720-A7FF;
}
@font-face {
  font-family: 'Bricolage Grotesque';
  font-style: normal;
  font-weight: 600 700;
  font-stretch: 100%;
  font-display: swap;
  src: url('/_streamline/assets/fonts/bricolage-grotesque-latin.woff2') format('woff2');
  unicode-range: U+0000-00FF, U+0131, U+0152-0153, U+02BB-02BC, U+02C6, U+02DA, U+02DC, U+0304, U+0308, U+0329, U+2000-206F, U+20AC, U+2122, U+2191, U+2193, U+2212, U+2215, U+FEFF, U+FFFD;
}
@font-face {
  font-family: 'Inter';
  font-style: normal;
  font-weight: 400 600;
  font-display: swap;
  src: url('/_streamline/assets/fonts/inter-latin-ext.woff2') format('woff2');
  unicode-range: U+0100-02BA, U+02BD-02C5, U+02C7-02CC, U+02CE-02D7, U+02DD-02FF, U+0304, U+0308, U+0329, U+1D00-1DBF, U+1E00-1E9F, U+1EF2-1EFF, U+2020, U+20A0-20AB, U+20AD-20C4, U+2113, U+2C60-2C7F, U+A720-A7FF;
}
@font-face {
  font-family: 'Inter';
  font-style: normal;
  font-weight: 400 600;
  font-display: swap;
  src: url('/_streamline/assets/fonts/inter-latin.woff2') format('woff2');
  unicode-range: U+0000-00FF, U+0131, U+0152-0153, U+02BB-02BC, U+02C6, U+02DA, U+02DC, U+0304, U+0308, U+0329, U+2000-206F, U+20AC, U+2122, U+2191, U+2193, U+2212, U+2215, U+FEFF, U+FFFD;
}

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
  --input: #3f577c;
  --ring: #74d8ff;
  --cyan: #74d8ff;
  --green: #94ead1;
  --text: #edf5ff;
  --dim: #b4c5e0;
  --radius: 18px;
  --frame-edge: #6481b94d;
  --surface: linear-gradient(135deg, #213b6142 0%, #111c35dd 37%, #0b1429ed 100%);
  --frame-shadow: inset 0 1px 0 #dceeff0b, 0 12px 36px #01071532;

  --sl-font-heading: "Bricolage Grotesque", "Segoe UI", system-ui, sans-serif;
  --sl-font-body: Inter, system-ui, -apple-system, "Segoe UI", Roboto, sans-serif;
  --sl-font-mono: ui-monospace, "SF Mono", Menlo, Consolas, monospace;
  --sl-text-title: 32px;
  --sl-text-h2: 24px;
  --sl-text-h3: 20px;
  --sl-text-body: 16px;
  --sl-text-label: 14px;
  --sl-text-min: 14px;
  --sl-leading: 1.5;
  --sl-radius: 14px;
  --sl-radius-lg: 22px;
  --sl-radius-card: 26px;
  --sl-radius-pill: 999px;
  --sl-card-pad: 1.25rem 1.35rem 1.4rem;
  --sl-ease: cubic-bezier(.22, 1, .36, 1);
  --sl-dur-hover: 160ms;
  --sl-dur-card: 280ms;
  --ss-ultramarine: #3268f5;
  --ss-space: #050817;
  --ss-panel: #0c1428;
  --ss-text: #edf5ff;
  --ss-text-2: #b4c5e0;
  --sl-chrome: linear-gradient(115deg, #e6f5ff8a, #6481b91a 34%, #3268f530 67%, #e6f5ff50);
}

* { box-sizing: border-box; }

[hidden] { display: none !important; }

body {
  margin: 0;
  background: var(--ss-space);
  color: var(--ss-text);
  font: 400 var(--sl-text-body)/var(--sl-leading) var(--sl-font-body);
  -webkit-font-smoothing: antialiased;
}

button, input, textarea, select { font: inherit; }
button, a, input, textarea, select {
  -webkit-tap-highlight-color: transparent;
  transition: background var(--sl-dur-hover) var(--sl-ease),
              border-color var(--sl-dur-hover) var(--sl-ease),
              box-shadow var(--sl-dur-hover) var(--sl-ease),
              color var(--sl-dur-hover) var(--sl-ease),
              transform var(--sl-dur-hover) var(--sl-ease);
}
button { cursor: pointer; color: inherit; background: transparent; border: 0; }
button:disabled { cursor: not-allowed; opacity: .55; }
a { color: inherit; text-decoration: none; }
svg { flex-shrink: 0; }

button:focus-visible, a:focus-visible, input:focus-visible, textarea:focus-visible, select:focus-visible, summary:focus-visible {
  outline: 3px solid #4ccfff;
  outline-offset: 4px;
}

h1, h2, h3, h4 {
  margin: 0;
  color: var(--text);
  font-family: var(--sl-font-heading);
  letter-spacing: -.01em;
}
h1 { font-size: var(--sl-text-title); line-height: 1.25; font-weight: 700; text-wrap: balance; }
h2 { font-size: var(--sl-text-h2); line-height: 1.3; font-weight: 600; }
h3 { font-size: var(--sl-text-h3); line-height: 1.4; font-weight: 600; }
h4 { font-size: 18px; line-height: 1.4; font-weight: 600; }
p { margin: 0; color: var(--dim); line-height: var(--sl-leading); }
textarea, pre, code { font-family: var(--sl-font-mono); }

.skip-link {
  position: fixed;
  z-index: 100;
  top: -80px;
  left: 20px;
  background: #2455d5;
  color: #fff;
  padding: 12px 20px;
  border-radius: 10px;
}
.skip-link:focus { top: 12px; }

.streamline-app {
  position: relative;
  isolation: isolate;
  display: block;
  min-height: 100vh;
  background: var(--ss-space);
}
.streamline-app::before {
  content: "";
  position: fixed;
  inset: 0;
  z-index: -1;
  pointer-events: none;
  opacity: .85;
  background: url("/_streamline/assets/artwork/streamline-nebula.svg") center top / cover no-repeat;
}
.streamline-app::after {
  content: "";
  position: fixed;
  inset: 0;
  z-index: -1;
  pointer-events: none;
  opacity: .18;
  background-image:
    url("/_streamline/assets/artwork/streamline-grain.svg"),
    radial-gradient(1px 1px at 12% 18%, #e6f5ff8c, transparent 65%),
    radial-gradient(1px 1px at 67% 8%, #e6f5ff75, transparent 65%),
    radial-gradient(1px 1px at 83% 31%, #e6f5ff65, transparent 65%),
    radial-gradient(1px 1px at 43% 73%, #e6f5ff60, transparent 65%);
  background-size: 220px 220px, 490px 580px, 670px 710px, 810px 760px, 560px 740px;
}
.streamline-app[data-calm="true"]::before,
.streamline-app[data-calm="true"]::after,
.streamline-app[data-calm="true"] .page-heading::before,
.streamline-app[data-calm="true"] .sl-hero-streaks {
  display: none;
}
.streamline-app[data-calm="true"] {
  background: var(--ss-space);
}

.sl-header {
  height: auto;
  min-height: 78px;
  position: sticky;
  top: 0;
  z-index: 35;
  display: flex;
  align-items: center;
  gap: 20px;
  padding: 14px max(24px, calc((100vw - 1320px) / 2));
  border-bottom: 1px solid #d1e6ff30;
  background: #080d20f5;
}
.sl-wordmark {
  font-family: var(--sl-font-heading), sans-serif;
  font-size: 28px;
  font-weight: 700;
  font-style: italic;
  line-height: 1.1;
  letter-spacing: -.02em;
  background: linear-gradient(105deg, #ffffff 7%, #d5e9ff 34%, #7edaff 69%, #b5afff 100%);
  background-clip: text;
  -webkit-background-clip: text;
  -webkit-text-fill-color: transparent;
  color: transparent;
  padding: 6px 2px;
  min-height: 44px;
  display: inline-flex;
  align-items: center;
  flex-shrink: 0;
}
.sl-menu-toggle { display: none; }
.icon-button {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 44px;
  min-width: 44px;
  height: 44px;
  color: #b2c8e7;
  border-radius: 10px;
}
.icon-button:hover { background: #203454; color: #fff; }

.sl-nav, .sl-nav-open {
  display: flex;
  align-items: center;
  gap: 6px;
  flex: 1;
  flex-wrap: wrap;
}
.sl-nav-btn {
  display: inline-flex;
  gap: 8px;
  align-items: center;
  border-radius: var(--sl-radius-pill);
  min-height: 44px;
  padding: 9px 14px;
  font-size: 14px;
  font-weight: 600;
  color: #b5c5df;
  border: 1px solid transparent;
  white-space: nowrap;
}
.sl-nav-btn:hover {
  background: #718ce818;
  color: #f6fbff;
}
.sl-nav-btn.selected {
  background: linear-gradient(105deg, #3268f52e, #3268f512);
  border-color: #7192ff66;
  color: #f3f8ff;
}

.sl-private {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  color: var(--dim);
  font-size: 14px;
  white-space: nowrap;
  flex-shrink: 0;
}
.sl-conn-pill {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 4px 10px;
  border-radius: var(--sl-radius-pill);
  border: 1px solid #497e73;
  background: #407d691b;
  color: var(--green);
  font-size: 13px;
  font-weight: 600;
}
.status-dot {
  display: inline-block;
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--cyan);
  flex-shrink: 0;
}
.status-dot.green { background: var(--green); }
.status-dot.amber { background: #ffd18a; }

.sl-main {
  max-width: 1320px;
  width: min(1320px, calc(100% - 48px));
  margin: 0 auto;
  padding: 32px 0 100px;
  min-width: 0;
}

.page-heading {
  position: relative;
  isolation: isolate;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 28px;
  min-height: 154px;
  padding: 24px 0 26px;
  margin-bottom: 28px;
  border-bottom: 1px solid #a9dbff30;
}
.page-heading::before {
  content: "";
  position: absolute;
  inset: -24px -32px auto;
  z-index: -1;
  height: 210px;
  background: url("/_streamline/assets/artwork/streamline-streaks.svg") center / 100% 100% no-repeat;
  opacity: .6;
  pointer-events: none;
}
.sl-hero-streaks {
  position: absolute;
  inset: 0;
  z-index: -1;
  display: block;
  background: url("/_streamline/assets/artwork/streamline-streaks.svg") center / cover no-repeat;
  pointer-events: none;
}
.page-heading > div:first-of-type {
  position: relative;
  z-index: 1;
  max-width: 760px;
}
.page-heading h1 {
  font-size: var(--sl-text-title);
  line-height: 1.25;
  font-weight: 700;
}
.page-heading p {
  max-width: 62ch;
  margin-top: 10px;
  font-size: 16px;
  color: var(--dim);
}
.page-heading-right {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 20px;
  flex-shrink: 0;
}

.sl-presence-original-font {
  font: 400 16px/1.6 "Segoe UI", Arial, Helvetica, sans-serif;
}
.sl-presence {
  display: flex;
  align-items: center;
  gap: .5rem;
  flex-shrink: 0;
  max-width: 100%;
}
.sl-stage {
  position: relative;
  width: 112px;
  height: 112px;
  flex: 0 0 112px;
  isolation: isolate;
  padding: 0;
  border: 0;
  border-radius: 50%;
  background: transparent;
  color: inherit;
  cursor: pointer;
  touch-action: manipulation;
  display: grid;
  place-items: center;
}
.sl-stage:focus-visible { outline: 2px solid #a9dbff; outline-offset: 2px; }
.sl-stage canvas {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  pointer-events: none;
}
.sl-field { z-index: 0; }
.sl-body { z-index: 1; }
.sl-fallback-mark {
  width: 88px;
  height: 88px;
  display: block;
  filter: drop-shadow(0 0 18px #3969f848);
  pointer-events: none;
}
.sl-caption {
  display: flex;
  flex-direction: column;
  gap: .15rem;
  font-size: 13px;
  color: #c9d7ea;
}
.sl-caption .brand-wordmark {
  font-family: var(--sl-font-heading);
  font-size: 20px;
  font-weight: 700;
  line-height: 1.1;
  letter-spacing: -.02em;
  background: linear-gradient(180deg, #fff 0%, #e6f5ff 38%, #6481b9 55%, #e6f5ff 100%);
  background-clip: text;
  -webkit-background-clip: text;
  -webkit-text-fill-color: transparent;
}
.sl-pause {
  min-height: 44px;
  padding: .3rem .5rem;
  margin-left: -.5rem;
  border: 0;
  background: transparent;
  color: #a9dbff;
  text-align: left;
  cursor: pointer;
  font: inherit;
  font-size: 14px;
  text-decoration: underline;
  text-underline-offset: 3px;
}
.sl-motion-note {
  font-size: 13px;
  color: #c9d7ea;
}

.panel {
  position: relative;
  padding: var(--sl-card-pad);
  border: 1px solid transparent;
  border-radius: var(--sl-radius-card);
  background: linear-gradient(150deg, #101a2ff8, #0c1428fa) padding-box, var(--sl-chrome) border-box;
  box-shadow: inset 0 1px 0 #e6f5ff0d, inset 0 0 40px #3268f50b, 0 24px 60px -30px #010715cc;
  min-width: 0;
}
.eyebrow {
  display: block;
  font-size: 14px;
  font-weight: 600;
  color: #a9dbff;
  margin-bottom: 10px;
}
.button {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  border: 1px solid transparent;
  border-radius: var(--sl-radius-pill);
  min-height: 44px;
  padding: .7rem 1.15rem;
  font: 600 14px/1.45 var(--sl-font-body);
  text-align: center;
}
.button.primary {
  background: linear-gradient(115deg, #3268f5, #4264eb);
  border-color: #a9c3ff6b;
  color: #fff;
  box-shadow: inset 0 1px 0 #e6f5ff22, 0 5px 22px #3268f535;
}
.button.primary:hover {
  background: #3268f5;
  border-color: #c0d8ffba;
  box-shadow: inset 0 1px 0 #e6f5ff22, 0 7px 26px #3268f54d;
  transform: translateY(-1px);
}
.button.secondary {
  background: #a9dbff0b;
  border-color: #a9dbff3b;
  color: #eaf7ff;
  box-shadow: inset 0 1px 0 #e6f5ff08;
}
.button.secondary:hover {
  background: #a9dbff16;
  border-color: #a9dbff70;
  transform: translateY(-1px);
}
.button.danger {
  color: #ffb8bf;
  border: 1px solid rgba(255, 133, 151, .4);
  background: rgba(139, 39, 64, .22);
}
.button.danger:hover {
  background: rgba(170, 44, 69, .35);
}
.text-button {
  display: inline-flex;
  align-items: center;
  gap: 7px;
  min-height: 44px;
  padding: 8px 4px;
  color: #a9dbff;
  font-size: 14px;
  text-align: left;
}
.text-button:hover { color: #fff; }

.tag {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  max-width: 100%;
  font-size: 14px;
  font-weight: 600;
  line-height: 1.45;
  padding: .28rem .65rem;
  border: 1px solid #476083;
  border-radius: var(--sl-radius-pill);
  color: #bed0e7;
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
}
.tag.blue { color: #c2e4ff; border-color: #5277ac; background: #2b57982a; }
.tag.green { color: #a1e8d5; border-color: #497e73; background: #407d691b; }
.tag.amber { color: #ffd18a; border-color: #96744e; background: #946a2d1a; }
.tag.danger { color: #ffb8bf; border-color: #ad5f6e; background: #6b203333; }

.banner {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 12px 16px;
  border: 1px solid;
  border-radius: 14px;
  margin-bottom: 24px;
  font-size: 16px;
  flex-wrap: wrap;
}
.banner > span { flex: 1; min-width: 180px; }
.banner.error { background: #4d252633; border-color: #ad7266; color: #ffcab4; }
.banner.success { background: #183d3433; border-color: #487f70; color: #adebd8; }

.row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 14px;
  flex-wrap: wrap;
  margin-bottom: 18px;
}
.tabs {
  display: flex;
  flex-wrap: wrap;
  gap: 10px;
  margin-bottom: 24px;
}
.section-stack { display: grid; gap: 22px; }

.sl-unlock-panel {
  max-width: 680px;
  margin: 0 auto 28px;
  padding: 32px;
  background: radial-gradient(ellipse at 100% 0, #6c5bce32, transparent 65%), linear-gradient(145deg, #162a4b, #0c1428);
}
.sl-unlock-panel h2 { font-size: 26px; margin-bottom: 10px; }
.sl-unlock-panel > p { margin-bottom: 22px; max-width: 58ch; }
.sl-unlock-form { display: grid; gap: 16px; }
.sl-unlock-actions { display: flex; gap: 12px; flex-wrap: wrap; align-items: center; }

.sh-home { display: grid; gap: 24px; }
.sh-welcome {
  padding: 30px;
  background: radial-gradient(ellipse at 100% 0, #6c5bce32, transparent 65%), linear-gradient(120deg, #1a38585c, #121d38d9);
}
.sh-welcome h2 { font-size: 28px; margin: 10px 0 10px; }
.sh-welcome > p { max-width: 65ch; font-size: 16px; }
.sh-connection {
  display: flex;
  gap: 14px;
  align-items: center;
  margin-top: 28px;
  padding-top: 24px;
  border-top: 1px solid #9ab5e02b;
  flex-wrap: wrap;
}
.sh-connection > div { flex: 1; min-width: 220px; }
.sh-connection strong { font-size: 16px; display: block; color: var(--text); }
.sh-connection p { font-size: 14px; margin-top: 6px; }
.sh-cards {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 18px;
}
.sh-card {
  text-align: left;
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 12px;
  cursor: pointer;
  min-width: 0;
  color: inherit;
  transition: transform var(--sl-dur-card) var(--sl-ease), border-color var(--sl-dur-card) var(--sl-ease), background var(--sl-dur-card) var(--sl-ease);
}
.sh-card:hover {
  transform: translateY(-4px);
  border-color: #88a6ef66;
  background: linear-gradient(150deg, #1a2c50f8, #101b36fa) padding-box, var(--sl-chrome) border-box;
}
.sh-status {
  display: inline-flex;
  gap: 6px;
  align-items: center;
  font-size: 14px;
  color: #b7c7e3;
}
.sh-card h2 { font-size: 22px; margin-top: 2px; }
.sh-card p { font-size: 14px; line-height: 1.65; flex: 1; overflow-wrap: anywhere; }
.sh-action {
  display: inline-flex;
  gap: 8px;
  align-items: center;
  font-size: 14px;
  font-weight: 600;
  color: #c1dcff;
  margin-top: 10px;
}

.sl-document-surface {
  background: #10182b;
  border-radius: var(--sl-radius-lg);
  color: var(--ss-text);
  isolation: isolate;
}
.sl-document-surface p,
.sl-document-surface li { font-size: 16px; line-height: 1.55; }
.sl-document-surface th,
.sl-document-surface td { font-size: 14px; font-variant-numeric: tabular-nums; }

.md-doc {
  font: 400 16px/1.65 var(--sl-font-body);
  color: #eaf0fa;
  overflow-wrap: anywhere;
}
.md-with-outline {
  display: grid;
  grid-template-columns: minmax(160px, 210px) minmax(0, 1fr);
  gap: 32px;
  align-items: start;
}
.md-outline {
  display: flex;
  flex-direction: column;
  gap: 10px;
  position: sticky;
  top: 96px;
  max-height: calc(100dvh - 120px);
  overflow: auto;
  font-size: 14px;
  line-height: 1.45;
  padding: 18px;
  border: 1px solid rgba(151, 177, 219, .18);
  border-radius: 16px;
  background: rgba(13, 24, 45, .92);
}
.md-outline strong { font-size: 14px; margin-bottom: 6px; color: var(--text); }
.md-outline a { color: #b5c8e8; text-decoration: none; }
.md-outline a:hover { color: #edf4ff; text-decoration: underline; }
.md-content { max-width: 68ch; min-width: 0; width: 100%; margin: 0 auto; }
.md-content p { font-size: 16px; line-height: 1.65; margin: 0 0 1.1em; white-space: pre-line; color: inherit; }
.md-heading { display: flex; gap: 12px; align-items: baseline; flex-wrap: wrap; margin: 1.6em 0 .7em; }
.md-heading:first-child { margin-top: 0; }
.md-heading h2, .md-heading h3, .md-heading h4 { margin: 0; scroll-margin-top: 105px; }
.md-copy-btn {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 4px 6px;
  color: #b8c9e5;
  font-size: 13px;
  white-space: nowrap;
}
.md-copy-btn:hover { color: #fff; }
.md-content ul, .md-content ol { padding-left: 1.5em; margin: 0 0 1.2em; }
.md-content li { font-size: 16px; padding: 3px 0; line-height: 1.6; }
.md-table-wrap {
  overflow-x: auto;
  margin: 1em 0 1.5em;
  width: 100%;
  border: 1px solid rgba(153, 178, 218, .25);
  border-radius: 12px;
}
.md-content table { width: 100%; border-collapse: collapse; font-size: 14px; font-variant-numeric: tabular-nums; }
.md-content th, .md-content td {
  padding: 12px 14px;
  text-align: left;
  vertical-align: top;
  border-bottom: 1px solid rgba(153, 178, 218, .16);
}
.md-content th { font-weight: 600; background: rgba(80, 112, 175, .12); }
.md-content tr:last-child td { border-bottom: 0; }
.md-content a { color: #a9caff; text-decoration: underline; text-underline-offset: 3px; }
.md-content code { font-size: 14px; background: rgba(82, 116, 174, .14); padding: 2px 6px; border-radius: 4px; }
.md-content pre {
  font-size: 14px;
  max-width: 100%;
  overflow: auto;
  padding: 18px;
  border: 1px solid rgba(153, 178, 218, .2);
  border-radius: 12px;
  background: rgba(4, 11, 24, .65);
  white-space: pre-wrap;
}
.md-content pre code { padding: 0; background: none; }
.md-content blockquote {
  margin: 1em 0;
  padding: 12px 18px;
  border-left: 3px solid #739fff;
  background: rgba(70, 100, 158, .1);
  white-space: pre-line;
}
.md-content hr { border: 0; border-top: 1px solid rgba(153, 178, 218, .25); margin: 1.8em 0; }

.dr-reader {
  padding: clamp(18px, 3vw, 34px);
  border: 1px solid rgba(147, 174, 216, .25);
  border-radius: var(--sl-radius-card);
  background: #10182b;
}
.dr-header {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  gap: 20px;
  margin-bottom: 24px;
  padding-bottom: 20px;
  border-bottom: 1px solid rgba(147, 174, 216, .2);
  flex-wrap: wrap;
}
.dr-meta { display: flex; gap: 10px; flex-wrap: wrap; align-items: center; margin-top: 10px; font-size: 14px; color: var(--dim); }
.dr-actions { display: flex; gap: 10px; flex-wrap: wrap; align-items: center; }
.dr-history {
  padding: 20px;
  border: 1px solid rgba(147, 174, 216, .25);
  border-radius: 16px;
  margin-bottom: 24px;
  background: rgba(7, 16, 32, .4);
}
.dr-version {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 18px;
  padding: 16px 0;
  border-top: 1px solid rgba(147, 174, 216, .2);
  flex-wrap: wrap;
}
.dr-version:first-of-type { border-top: 0; }
.dr-version > div:first-child { display: flex; flex-direction: column; gap: 5px; }
.dr-version span { font-size: 14px; color: #b7c7df; }
.dr-source-details {
  padding: 16px;
  margin-top: 24px;
  border: 1px solid #486689;
  border-radius: 14px;
  background: #16314a33;
}
.dr-source-details summary { cursor: pointer; font-size: 15px; font-weight: 600; color: #d5e6ff; }
.dr-passport-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 12px;
  margin: 16px 0;
  font-size: 14px;
}

.collection-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  margin-bottom: 20px;
  flex-wrap: wrap;
}
.search-input {
  display: flex;
  align-items: center;
  gap: 10px;
  border: 1px solid #3b5578;
  border-radius: 12px;
  padding: 0 12px;
  background: #0d1b30;
  flex: 1;
  min-width: 240px;
  max-width: 460px;
  color: #aac4e7;
}
.search-input input {
  border: 0;
  background: transparent;
  margin: 0;
  padding: 11px 0;
  outline: 0;
  min-width: 0;
  font-size: 16px;
  min-height: 44px;
}
.resource-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 20px;
}
.resource-card {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 10px;
  transition: transform var(--sl-dur-card) var(--sl-ease), border-color var(--sl-dur-card) var(--sl-ease);
}
.resource-card:hover { transform: translateY(-4px); border-color: #88a6ef66; }
.resource-card-top {
  display: flex;
  align-items: center;
  justify-content: space-between;
  width: 100%;
  gap: 10px;
  flex-wrap: wrap;
}
.resource-title {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  text-align: left;
  width: 100%;
  font-size: 20px;
  line-height: 1.35;
  font-weight: 600;
  padding: 0;
  min-height: 44px;
  overflow-wrap: anywhere;
  color: var(--text);
}
.resource-title:hover { color: #a9dbff; }
.resource-meta {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  font-size: 14px;
  color: #adbfda;
  font-variant-numeric: tabular-nums;
}
.resource-card-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  flex-wrap: wrap;
  border-top: 1px solid #8aace438;
  padding-top: 12px;
  margin-top: auto;
  width: 100%;
}
.triage-actions { display: flex; gap: 8px; flex-wrap: wrap; }
.triage-btn {
  min-height: 38px;
  padding: 6px 12px;
  font-size: 13px;
  border-radius: var(--sl-radius-pill);
  border: 1px solid #48648f;
  background: #142442;
  color: #cde1fb;
}
.triage-btn:hover { background: #203763; border-color: #7ea6e0; }

.tw-workspace { display: grid; gap: 22px; }
.tw-intro {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 24px;
  flex-wrap: wrap;
  background: radial-gradient(ellipse at top right, rgba(86, 107, 255, .14), transparent 65%), linear-gradient(150deg, #101a2ff8, #0c1428fa);
}
.tw-project-list { display: grid; gap: 12px; }
.tw-project-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 16px 20px;
  border: 1px solid rgba(143, 166, 214, .22);
  border-radius: 16px;
  background: rgba(20, 34, 65, .45);
  text-align: left;
  width: 100%;
  flex-wrap: wrap;
}
.tw-project-row:hover { border-color: #88a6ef66; background: rgba(34, 57, 106, .45); }
.tw-project-row.selected { border-color: #74d8ff88; background: rgba(44, 78, 146, .35); }
.tw-project-row strong { font-size: 17px; display: block; color: var(--text); }
.tw-project-row small { font-size: 14px; color: var(--dim); }

.tw-hero { display: grid; gap: 16px; }
.tw-actions { display: flex; gap: 10px; flex-wrap: wrap; align-items: center; }
.tw-docs {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 12px;
}
.tw-doc-card {
  display: flex;
  gap: 14px;
  text-align: left;
  padding: 18px;
  border: 1px solid rgba(143, 166, 214, .18);
  border-radius: 14px;
  background: rgba(37, 65, 113, .14);
  color: inherit;
  align-items: flex-start;
  cursor: pointer;
  min-width: 0;
}
.tw-doc-card:hover { background: rgba(65, 99, 172, .22); border-color: rgba(137, 167, 255, .45); }
.tw-doc-card > div { display: grid; gap: 5px; flex: 1; min-width: 0; }
.tw-doc-card strong { overflow-wrap: anywhere; font-size: 16px; }
.tw-doc-card p { font-size: 14px; color: var(--dim); }
.tw-doc-card small { font-size: 13px; color: #a7bedd; }

.pc-comparison {
  border: 1px solid var(--border);
  border-radius: var(--sl-radius-card);
  padding: clamp(18px, 3vw, 30px);
  background: #0c162b;
  color: #eaf7ff;
  min-width: 0;
}
.pc-heading { display: flex; justify-content: space-between; gap: 22px; align-items: flex-start; flex-wrap: wrap; }
.pc-legend { display: flex; gap: 9px; flex-wrap: wrap; margin: 20px 0 16px; }
.pc-chip {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  border: 1px solid currentColor;
  border-radius: 999px;
  font-size: 14px;
  font-weight: 600;
  padding: 5px 10px;
  white-space: nowrap;
}
.pc-chip.matches { color: #a6f3dd; background: #1a5144; }
.pc-chip.short { color: #ffdba0; background: #77512233; }
.pc-chip.missing { color: #ffb8bf; background: #6b203333; }
.pc-chip.pending { color: #ccdaff; background: #2b3b6833; }
.pc-truth-note {
  padding: 14px 16px;
  border-left: 3px solid #7298e9;
  background: #101e36;
  font-size: 15px;
  color: #c8d6ee;
  margin-bottom: 16px;
}
.pc-controls { display: flex; flex-wrap: wrap; gap: 18px; margin: 18px 0; }
.pc-toggle { display: inline-flex; align-items: center; gap: 8px; font-size: 14px; cursor: pointer; margin: 0; }
.pc-toggle input { width: 18px; height: 18px; accent-color: #3268f5; flex: none; margin: 0; min-height: 0; }
.pc-source-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 16px; margin-bottom: 18px; }
.pc-source {
  display: grid;
  gap: 12px;
  padding: 18px;
  border: 1px solid #314969;
  border-radius: 16px;
  background: #101e36;
  min-width: 0;
}
.pc-source-actions { display: flex; align-items: flex-end; justify-content: space-between; gap: 12px; flex-wrap: wrap; }
.pc-source-actions label { flex: 1; min-width: 140px; margin: 0; font-size: 14px; }
.pc-rows { display: grid; gap: 20px; }
.pc-row { border: 1px solid #314969; border-radius: 18px; overflow: hidden; background: #0a1428; }
.pc-row-head {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 16px;
  padding: 16px 18px;
  background: #122039;
  flex-wrap: wrap;
}
.pc-paired { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 1px; background: #314969; }
.pc-pane { min-width: 0; background: #0c162b; }
.pc-pane-label { display: grid; gap: 4px; padding: 12px 18px; border-bottom: 1px solid #263b59; font-size: 14px; }
.pc-pane-body { padding: 18px; font-size: 15px; line-height: 1.55; overflow-wrap: anywhere; }
.pc-pane-body.synced { max-height: 440px; overflow: auto; }
.pc-assessment { padding: 14px 18px; border-top: 1px solid #263b59; font-size: 14px; color: #c8d6ee; }
.pc-word-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 20px; padding: 14px 18px; border-top: 1px solid #263b59; }
.pc-word-grid del { text-decoration: line-through; background: #713c4a44; color: #ffced4; }
.pc-word-grid ins { text-decoration: underline; text-underline-offset: 3px; background: #1b5e4b55; color: #b6f4de; }

.connection-hero {
  display: flex;
  align-items: center;
  gap: 24px;
  margin-bottom: 24px;
  flex-wrap: wrap;
  background: radial-gradient(ellipse at 90% 0, #7367c638, transparent 60%), linear-gradient(115deg, #214a6d8a, #14203be6);
}
.connection-symbol {
  width: 64px;
  min-width: 64px;
  height: 64px;
  display: grid;
  place-items: center;
  border: 1px solid #6489b7;
  border-radius: 20px;
  background: #2c507744;
  color: #b9dcff;
}
.connection-hero > div:nth-child(2) { flex: 1; min-width: 240px; }
.settings-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 24px;
  margin-bottom: 24px;
  align-items: start;
}
.endpoint {
  display: flex;
  align-items: center;
  gap: 12px;
  border: 1px solid #4b6b94;
  border-radius: 12px;
  padding: 12px 14px;
  background: #0b192d;
  margin: 18px 0;
  flex-wrap: wrap;
}
.endpoint code { flex: 1; min-width: 180px; font-size: 14px; overflow-wrap: anywhere; color: #e2f2ff; }
.connection-steps { margin-top: 18px; display: grid; gap: 12px; }
.connection-steps > div { display: flex; align-items: flex-start; gap: 12px; }
.connection-steps span {
  display: grid;
  place-items: center;
  width: 30px;
  min-width: 30px;
  height: 30px;
  font-size: 14px;
  border: 1px solid #54739b;
  background: #1d3859;
  border-radius: 50%;
  color: #d2e7ff;
}

.pl-safety {
  display: flex;
  align-items: flex-start;
  gap: 12px;
  border: 1px solid #80bea333;
  border-radius: 12px;
  background: #163c3d38;
  padding: 15px 18px;
  margin-bottom: 20px;
}
.pl-safety p { font-size: 14px; color: #c4f4e0; }
.pl-prompt-box {
  width: 100%;
  min-height: 210px;
  font-size: 13px;
  line-height: 1.6;
}

label { display: block; color: #d0e1f5; font-size: 14px; margin-bottom: 18px; font-weight: 600; }
input, textarea, select {
  display: block;
  width: 100%;
  max-width: 100%;
  padding: 11px 14px;
  margin-top: 8px;
  background: #060e22b3;
  color: #edf7ff;
  border: 1px solid #7697c66b;
  border-radius: 10px;
  min-height: 46px;
  font-size: 16px;
  font-weight: 400;
  box-shadow: inset 0 1px 3px #01040f35;
  color-scheme: dark;
}
input::placeholder, textarea::placeholder { color: #9aafcd; }
textarea { resize: vertical; line-height: 1.6; }
.checkbox-row {
  display: flex !important;
  align-items: center;
  gap: 10px !important;
  min-height: 44px;
  margin: 10px 0;
  cursor: pointer;
}
.checkbox-row input { width: 18px !important; height: 18px !important; min-height: 0; margin: 0; accent-color: #3268f5; flex: none; }
.field-help { display: block; color: #b0c4e0; font-size: 14px; font-weight: 400; line-height: 1.5; margin-top: 6px; }
.form-two { display: grid; grid-template-columns: 1fr 1fr; gap: 18px; }

.modal {
  position: fixed;
  inset: 0;
  margin: auto;
  background: radial-gradient(ellipse at 100% 0, #44487938, transparent 55%), #10182b;
  border: 1px solid #a1c2f177;
  border-radius: 22px;
  padding: 28px;
  color: #eaf7ff;
  width: min(580px, calc(100% - 32px));
  max-height: 90dvh;
  overflow: auto;
  box-shadow: 0 24px 100px #000b;
}
.modal.wide { width: min(880px, calc(100% - 32px)); }
.modal::backdrop { background: #020817dc; }
.modal-heading { display: flex; align-items: flex-start; justify-content: space-between; gap: 16px; margin-bottom: 16px; }
.modal-heading h2 { font-size: 24px; }
.modal-actions {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 12px;
  border-top: 1px solid #91b0dc3b;
  padding-top: 18px;
  margin-top: 20px;
}
.editor-tabs { display: flex; gap: 8px; border-bottom: 1px solid rgba(147, 174, 216, .22); margin-bottom: 14px; }
.editor-tabs button {
  padding: 10px 18px;
  font-size: 14px;
  color: #b8c9e4;
  border-bottom: 2px solid transparent;
}
.editor-tabs button[aria-selected="true"] {
  color: #eef5ff;
  border-bottom-color: #93b4ff;
  font-weight: 600;
}
.editor-preview {
  padding: 20px;
  background: #0c1428;
  border: 1px solid rgba(147, 174, 216, .23);
  border-radius: 14px;
  min-height: 240px;
}

.empty-state {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 12px;
  padding: 28px 24px;
}
.empty-state p { max-width: 52ch; font-size: 16px; color: var(--ss-text-2); }
.inline-warning {
  padding: 14px 16px;
  border: 1px solid #9b784f;
  border-radius: 12px;
  background: #946a2d1a;
  color: #ffd9a2;
  font-size: 14px;
  margin: 12px 0;
}

.sl-footer {
  margin-top: 56px;
  padding-top: 24px;
  border-top: 1px solid #a4c0ed26;
  display: flex;
  gap: 24px;
  justify-content: space-between;
  align-items: center;
  flex-wrap: wrap;
  color: #b0c4df;
  font-size: 14px;
}
.sl-footer > span { display: inline-flex; align-items: center; gap: 8px; }

.skeleton > div {
  height: 20px;
  background: #8cb5fb16;
  border-radius: 8px;
  margin: 18px 0;
  max-width: 75%;
}

@media (max-width: 1050px) {
  .sl-private { display: none; }
  .sl-header { gap: 14px; padding: 14px 20px; }
  .sl-nav-btn { padding: 9px 10px; }
  .sh-cards, .settings-grid { grid-template-columns: 1fr; }
}
@media (max-width: 760px) {
  .sl-header { justify-content: space-between; flex-wrap: wrap; }
  .sl-menu-toggle { display: inline-flex; }
  .sl-nav { display: none; }
  .sl-nav-open { display: grid; width: 100%; flex-basis: 100%; grid-template-columns: 1fr 1fr; }
  .sl-main { width: calc(100% - 32px); padding: 20px 0 110px; }
  .page-heading { flex-direction: column; align-items: stretch; min-height: 0; gap: 18px; padding-top: 14px; }
  .page-heading-right { justify-content: space-between; }
  .resource-grid, .tw-docs, .pc-source-grid, .pc-paired, .pc-word-grid, .form-two, .dr-passport-grid, .md-with-outline {
    grid-template-columns: 1fr;
  }
  .md-outline { position: static; max-height: 180px; }
  .sl-stage { width: 96px; height: 96px; flex-basis: 96px; }
}
@media (prefers-reduced-motion: reduce) {
  .streamline-app::before, .streamline-app::after, .streamline-app .sl-hero-streaks { animation: none !important; }
  .sh-card:hover, .resource-card:hover, .button:hover { transform: none; }
}
"###;

const UI_JS: &str = r###"(function () {
  "use strict";

  var STORAGE_SLOT = "streamline_workspace_key";
  var CALM_STORAGE = "streamline-calm";
  var MOTION_STORAGE = "streamline-motion-paused";

  var VIEW_META = {
    home: {
      title: "Home",
      subtitle: "Keep your background, writing guidance and project documents in one place."
    },
    profile: {
      title: "My profile",
      subtitle: "Read your background, improve your profile and manage communication guidance."
    },
    projects: {
      title: "Projects",
      subtitle: "See what you planned beside what actually got done."
    },
    library: {
      title: "Library",
      subtitle: "Browse your standards, runbooks and archived Explorer shelves."
    },
    lessons: {
      title: "Lessons",
      subtitle: "Reusable forward lessons captured across builds and audits."
    },
    apps: {
      title: "Connected apps",
      subtitle: "Choose how AI assistants connect to your workspace."
    },
    technical: {
      title: "Technical details (for setup help)",
      subtitle: "Connection information for your AI app or setup helper."
    },
    settings: {
      title: "Settings",
      subtitle: "Manage your workspace, downloads and display preferences."
    }
  };

  var TRIO_ROLE_META = {
    blueprint: { name: "BLUEPRINT", desc: "What this project should deliver." },
    handhold: { name: "HANDHOLD", desc: "Ordered steps, constraints, and OWN guardrails." },
    as_built: { name: "AS-BUILT", desc: "Actual work, evidence, and remaining gaps." },
    how_it_works: { name: "HOW-IT-WORKS", desc: "Plain-English release review and operational truth." },
    fix_record: { name: "Fix Record", desc: "Captured repair or recovery notes." },
    sync_report: { name: "Plan Check", desc: "Latest alignment check between plan and results." }
  };

  var PROJECT_STATUS_LABELS = {
    STAGED: { label: "Staged", cls: "blue" },
    BUILDING: { label: "In build", cls: "blue" },
    CLEANING: { label: "In build", cls: "blue" },
    SCANNING: { label: "In build", cls: "blue" },
    ACCEPTING: { label: "In build", cls: "blue" },
    DOC_PENDING_OWNER: { label: "Waiting for HOW-IT-WORKS review", cls: "amber" },
    AUDIT_PENDING: { label: "Security audit pending", cls: "amber" },
    DONE: { label: "Done", cls: "green" },
    IMPASSE: { label: "Needs attention", cls: "danger" },
    QUARANTINED: { label: "Needs attention", cls: "danger" }
  };

  var state = {
    cred: "",
    unlocked: false,
    loading: false,
    busy: false,
    view: "home",
    profileTab: "Background",
    libraryShelf: "all",
    libraryTier: "all",
    librarySearch: "",
    lessonSearch: "",
    lessonTag: "all",
    calm: false,
    motionPaused: false,
    mobileNav: false,
    workspace: null,
    bootstrap: null,
    shelves: [],
    resources: [],
    projects: [],
    selectedProjectId: "",
    selectedBundle: null,
    lessons: [],
    skillsInfo: null,
    profileCachedId: "",
    profileCachedBody: "",
    readerResource: null,
    readerBody: "",
    readerHistory: [],
    readerHistoryOpen: false,
    readerPassport: null,
    readerCompareOut: null,
    compareOpen: false,
    compareDiffsOnly: false,
    compareSyncScroll: true,
    comparePlanRev: null,
    compareActualRev: null,
    comparePlanHistory: [],
    compareActualHistory: [],
    compareCheckResult: null,
    editorContext: null,
    editorMode: "edit",
    presenceCtrl: null,
    presencePhase: "ready",
    presenceLabel: "Ready for your next action"
  };

  function byId(id) { return document.getElementById(id); }

  function el(tag, cls, text) {
    var node = document.createElement(tag);
    if (cls) node.className = cls;
    if (text !== undefined && text !== null) node.textContent = String(text);
    return node;
  }

  function clearEl(node) {
    while (node.firstChild) node.removeChild(node.firstChild);
  }

  function iconEl(name, size) {
    var bank = byId("sl-icon-bank");
    if (!bank || !bank.content) return document.createTextNode("");
    var tpl = bank.content.querySelector('[data-icon="' + name + '"]');
    if (!tpl) return document.createTextNode("");
    var cloned = tpl.cloneNode(true);
    var px = String(size || 16);
    cloned.setAttribute("width", px);
    cloned.setAttribute("height", px);
    return cloned;
  }

  function formatDate(iso) {
    if (!iso) return "Recently";
    var d = new Date(iso);
    if (!Number.isFinite(d.getTime())) return String(iso);
    return d.toLocaleString(undefined, { month: "short", day: "numeric", hour: "numeric", minute: "2-digit" });
  }

  function slugify(text) {
    return String(text || "")
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-+|-+$/g, "")
      .slice(0, 48);
  }

  function humanError(err) {
    if (!err) return "The change could not be confirmed.";
    var code = err.code || "";
    var msg = err.message || String(err);
    var map = {
      UNAUTHORIZED: "That workspace key was not recognized. Check your key and try again.",
      FORBIDDEN: "This workspace key does not have permission for that section.",
      FORBIDDEN_WRITE: "This workspace key has read-only access.",
      NOT_FOUND: "That item could not be found.",
      REVISION_CONFLICT: "This document changed since you opened it. Refresh and review the latest version.",
      CONFLICT: "A conflicting record already exists. Review the current item before retrying.",
      INVALID_STATE: msg
    };
    return map[code] || msg;
  }

  function showError(msg) {
    var box = byId("banner-error");
    var txt = byId("banner-error-text");
    txt.textContent = msg;
    box.hidden = false;
    setPresence("blocked", "A request needs attention");
  }

  function showNotice(msg) {
    var box = byId("banner-notice");
    var txt = byId("banner-notice-text");
    txt.textContent = msg;
    box.hidden = false;
  }

  function clearBanners() {
    byId("banner-error").hidden = true;
    byId("banner-notice").hidden = true;
  }

  function setPresence(phase, label) {
    state.presencePhase = phase || "ready";
    state.presenceLabel = label || "Ready for your next action";
    var stage = byId("presence-stage-btn");
    var labelEl = byId("presence-phase-label");
    if (stage) stage.setAttribute("data-streamline-phase", state.presencePhase);
    if (labelEl) labelEl.textContent = state.presenceLabel;
    if (state.presenceCtrl && typeof state.presenceCtrl.setPhase === "function") {
      state.presenceCtrl.setPhase(state.presencePhase);
    }
  }

  function updateMotionControls() {
    var btn = byId("presence-pause-btn");
    var note = byId("presence-motion-note");
    var reduced = window.matchMedia && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    if (btn) {
      btn.textContent = state.motionPaused ? "Resume motion" : "Pause motion";
    }
    if (note) {
      if (reduced) {
        note.textContent = "Reduced motion - static view";
        note.hidden = false;
      } else if (state.motionPaused) {
        note.textContent = "Animation paused";
        note.hidden = false;
      } else {
        note.hidden = true;
      }
    }
    if (state.presenceCtrl && typeof state.presenceCtrl.setPaused === "function") {
      state.presenceCtrl.setPaused(state.motionPaused);
    }
  }

  function mountPresenceIfReady() {
    var api = window.StreamlinePresencePlugin;
    var stage = byId("presence-stage-btn");
    var bodyCanvas = byId("presence-body-canvas");
    var fieldCanvas = byId("presence-field-canvas");
    if (!api || !stage || !bodyCanvas || !fieldCanvas || state.presenceCtrl) return;
    try {
      state.presenceCtrl = api.mountStreamlinePresence(stage, bodyCanvas, fieldCanvas, {
        phase: state.presencePhase,
        paused: state.motionPaused,
        onError: function () {
          state.presenceCtrl = null;
          stage.hidden = true;
          var fallback = byId("presence-fallback-img");
          if (fallback) fallback.hidden = false;
        }
      });
      updateMotionControls();
    } catch (_e) {
      state.presenceCtrl = null;
    }
  }

  function applyCalmMode(enabled) {
    state.calm = Boolean(enabled);
    var root = byId("app-root");
    if (root) root.setAttribute("data-calm", state.calm ? "true" : "false");
    try { localStorage.setItem(CALM_STORAGE, String(state.calm)); } catch (_e) {}
  }

  async function callToolRaw(toolName, payload, overrideCred) {
    var activeCred = overrideCred || state.cred;
    var headers = { "Content-Type": "application/json" };
    if (activeCred) headers["X-Streamline-Credential"] = activeCred;
    var resp = await fetch("/api/tools/" + encodeURIComponent(toolName), {
      method: "POST",
      headers: headers,
      body: JSON.stringify(payload || {})
    });
    var json = await resp.json();
    if (!resp.ok || !json.ok) {
      var errObj = (json && json.error) || { code: "REQUEST_FAILED", message: "Request failed" };
      var err = new Error(errObj.message || "Request failed");
      err.code = errObj.code || "REQUEST_FAILED";
      throw err;
    }
    return json.data;
  }

  async function callTool(toolName, payload, overrideCred) {
    try {
      return await callToolRaw(toolName, payload, overrideCred);
    } catch (err) {
      if (err && err.code === "RULES_NOT_ACKNOWLEDGED") {
        var boot = await callToolRaw("bootstrap", {}, overrideCred);
        if (boot && boot.rules_sha256) {
          await callToolRaw("ack_rules", { rules_sha256: boot.rules_sha256 }, overrideCred);
        }
        return await callToolRaw(toolName, payload, overrideCred);
      }
      throw err;
    }
  }

  async function runAction(label, fn) {
    state.busy = true;
    setPresence("acting", label || "Saving your requested change");
    clearBanners();
    try {
      var result = await fn();
      setPresence("success", "Ready for your next action");
      return result;
    } catch (err) {
      showError(humanError(err));
      return null;
    } finally {
      state.busy = false;
    }
  }

  function copyText(text, successMsg) {
    if (navigator.clipboard && navigator.clipboard.writeText) {
      navigator.clipboard.writeText(text).then(function () {
        showNotice(successMsg || "Copied to clipboard.");
      }).catch(function () {
        showNotice("Select and copy the text directly.");
      });
    } else {
      showNotice("Select and copy the text directly.");
    }
  }

  function downloadFile(content, filename, mime) {
    var blob = new Blob([content], { type: mime || "application/json" });
    var url = URL.createObjectURL(blob);
    var a = document.createElement("a");
    a.href = url;
    a.download = filename;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    setTimeout(function () { URL.revokeObjectURL(url); }, 1000);
  }

  function safeDocumentHref(value) {
    var href = String(value || "").trim();
    if (/[\u0000-\u0020\\]/.test(href)) return null;
    if (/^(?:https?:|mailto:)/i.test(href) || /^#[^\s]*$/.test(href) || /^\/(?!\/)[^\s]*$/.test(href)) {
      return href;
    }
    return null;
  }

  function tableCells(line) {
    var trimmed = line.trim().replace(/^\|/, "").replace(/\|$/, "");
    var cells = [];
    var cell = "";
    var escaped = false;
    for (var i = 0; i < trimmed.length; i++) {
      var ch = trimmed.charAt(i);
      if (escaped) { cell += ch; escaped = false; }
      else if (ch === "\\") { escaped = true; }
      else if (ch === "|") { cells.push(cell.trim()); cell = ""; }
      else { cell += ch; }
    }
    cells.push(cell.trim());
    return cells;
  }

  function isTableRule(line) {
    return /^\s*\|?\s*:?-{3,}:?\s*(?:\|\s*:?-{3,}:?\s*)+\|?\s*$/.test(line);
  }

  function parseDocumentMarkdown(body) {
    var lines = String(body || "").replace(/\r\n?/g, "\n").split("\n");
    var blocks = [];
    var i = 0;
    var headingCount = 0;
    while (i < lines.length) {
      var line = lines[i];
      if (!line.trim()) { i++; continue; }
      if (/^\s*(```|~~~)/.test(line)) {
        var fence = line.trim().slice(0, 3);
        var codeLines = [];
        i++;
        while (i < lines.length && !lines[i].trim().startsWith(fence)) {
          codeLines.push(lines[i++]);
        }
        if (i < lines.length) i++;
        blocks.push({ type: "code", text: codeLines.join("\n") });
        continue;
      }
      var h = line.match(/^(#{1,6})\s+(.+?)\s*#*\s*$/);
      if (h) {
        var start = i;
        var level = h[1].length;
        var end = i + 1;
        while (end < lines.length) {
          var next = lines[end].match(/^(#{1,6})\s+/);
          if (next && next[1].length <= level) break;
          end++;
        }
        headingCount++;
        blocks.push({
          type: "heading",
          level: level,
          text: h[2],
          id: "section-" + headingCount,
          source: lines.slice(start, end).join("\n")
        });
        i++;
        continue;
      }
      if (/^\s*(?:---+|\*\*\*+|___+)\s*$/.test(line)) {
        blocks.push({ type: "rule" });
        i++;
        continue;
      }
      if (i + 1 < lines.length && line.includes("|") && isTableRule(lines[i + 1])) {
        var rows = [tableCells(line)];
        i += 2;
        while (i < lines.length && lines[i].includes("|") && lines[i].trim()) {
          rows.push(tableCells(lines[i++]));
        }
        blocks.push({ type: "table", rows: rows });
        continue;
      }
      if (/^\s*>\s?/.test(line)) {
        var qLines = [];
        while (i < lines.length && /^\s*>/.test(lines[i])) {
          qLines.push(lines[i++].replace(/^\s*>\s?/, ""));
        }
        blocks.push({ type: "quote", text: qLines.join("\n") });
        continue;
      }
      var listMatch = line.match(/^\s*(?:([-+*])|\d+[.)])\s+(.+)$/);
      if (listMatch) {
        var items = [];
        var ordered = !listMatch[1];
        while (i < lines.length) {
          var itemMatch = lines[i].match(/^\s*(?:([-+*])|\d+[.)])\s+(.+)$/);
          if (!itemMatch || (!itemMatch[1]) !== ordered) break;
          items.push(itemMatch[2]);
          i++;
        }
        blocks.push({ type: "list", ordered: ordered, items: items });
        continue;
      }
      var pLines = [line];
      i++;
      while (
        i < lines.length &&
        lines[i].trim() &&
        !/^(?:#{1,6}\s|\s*[>*+-]\s|\s*\d+[.)]\s|\s*```|\s*~~~)/.test(lines[i]) &&
        !(i + 1 < lines.length && isTableRule(lines[i + 1]))
      ) {
        pLines.push(lines[i++]);
      }
      blocks.push({ type: "paragraph", text: pLines.join("\n") });
    }
    return blocks;
  }

  function appendInlineMarkdown(parent, text) {
    var src = String(text || "");
    var token = /(`[^`\n]+`|\*\*[^*]+\*\*|__[^_]+__|~~[^~]+~~|\[[^\]]+\]\([^\s)]+\)|\*[^*\n]+\*|<br\s*\/?\s*>)/gi;
    var offset = 0;
    var m;
    while ((m = token.exec(src)) !== null) {
      if (m.index > offset) {
        parent.appendChild(document.createTextNode(src.slice(offset, m.index)));
      }
      var val = m[0];
      if (val.startsWith("`")) {
        parent.appendChild(el("code", "", val.slice(1, -1)));
      } else if (val.startsWith("**") || val.startsWith("__")) {
        parent.appendChild(el("strong", "", val.slice(2, -2)));
      } else if (val.startsWith("~~")) {
        parent.appendChild(el("s", "", val.slice(2, -2)));
      } else if (/^<br/i.test(val)) {
        parent.appendChild(document.createElement("br"));
      } else if (val.startsWith("[")) {
        var parts = val.match(/^\[([^\]]+)\]\(([^)]+)\)$/);
        var href = parts ? safeDocumentHref(parts[2]) : null;
        if (href && parts) {
          var a = el("a", "", parts[1]);
          a.setAttribute("href", href);
          if (/^https?:/i.test(href)) {
            a.setAttribute("target", "_blank");
            a.setAttribute("rel", "noopener noreferrer");
          }
          parent.appendChild(a);
        } else {
          parent.appendChild(document.createTextNode(parts ? parts[1] : val));
        }
      } else {
        parent.appendChild(el("em", "", val.slice(1, -1)));
      }
      offset = token.lastIndex;
    }
    if (offset < src.length) {
      parent.appendChild(document.createTextNode(src.slice(offset)));
    }
  }

  function renderMarkdownDocument(container, body, showOutline) {
    clearEl(container);
    var blocks = parseDocumentMarkdown(body);
    var headings = blocks.filter(function (b) { return b.type === "heading"; });
    var wrap = el("div", "md-doc" + (showOutline && headings.length ? " md-with-outline" : ""));
    wrap.setAttribute("data-reading-surface", "");
    var prefix = "md-" + Math.random().toString(36).slice(2, 7);

    if (showOutline && headings.length > 0) {
      var nav = el("nav", "md-outline");
      nav.setAttribute("aria-label", "Document sections");
      nav.appendChild(el("strong", "", "On this page"));
      headings.forEach(function (h) {
        var link = el("a", "", h.text);
        link.setAttribute("href", "#" + prefix + "-" + h.id);
        if (h.level > 2) link.style.paddingInlineStart = "14px";
        nav.appendChild(link);
      });
      wrap.appendChild(nav);
    }

    var article = el("article", "md-content");
    if (!String(body || "").trim()) {
      article.appendChild(el("p", "", "This document is empty."));
    } else {
      blocks.forEach(function (block) {
        if (block.type === "heading") {
          var hWrap = el("div", "md-heading");
          var lvl = Math.min(6, Math.max(2, block.level || 2));
          var hEl = document.createElement("h" + lvl);
          hEl.id = prefix + "-" + block.id;
          appendInlineMarkdown(hEl, block.text);
          hWrap.appendChild(hEl);
          var copyBtn = el("button", "md-copy-btn", "Copy section");
          copyBtn.type = "button";
          copyBtn.addEventListener("click", function () {
            copyText(block.source || block.text || "", "Section copied.");
          });
          hWrap.appendChild(copyBtn);
          article.appendChild(hWrap);
        } else if (block.type === "code") {
          var pre = document.createElement("pre");
          pre.appendChild(el("code", "", block.text));
          article.appendChild(pre);
        } else if (block.type === "rule") {
          article.appendChild(document.createElement("hr"));
        } else if (block.type === "quote") {
          var bq = document.createElement("blockquote");
          appendInlineMarkdown(bq, block.text);
          article.appendChild(bq);
        } else if (block.type === "table" && block.rows && block.rows.length) {
          var tWrap = el("div", "md-table-wrap");
          var table = document.createElement("table");
          var thead = document.createElement("thead");
          var headRow = document.createElement("tr");
          block.rows[0].forEach(function (cell) {
            var th = document.createElement("th");
            th.setAttribute("scope", "col");
            appendInlineMarkdown(th, cell);
            headRow.appendChild(th);
          });
          thead.appendChild(headRow);
          table.appendChild(thead);
          var tbody = document.createElement("tbody");
          block.rows.slice(1).forEach(function (row) {
            var tr = document.createElement("tr");
            row.forEach(function (cell) {
              var td = document.createElement("td");
              appendInlineMarkdown(td, cell);
              tr.appendChild(td);
            });
            tbody.appendChild(tr);
          });
          table.appendChild(tbody);
          tWrap.appendChild(table);
          article.appendChild(tWrap);
        } else if (block.type === "list") {
          var listEl = document.createElement(block.ordered ? "ol" : "ul");
          (block.items || []).forEach(function (item) {
            var li = document.createElement("li");
            var chk = item.match(/^\[([ xX])\]\s*(.*)$/);
            if (chk) {
              li.appendChild(el("span", "", (chk[1] === " " ? "[ ] " : "[x] ")));
              appendInlineMarkdown(li, chk[2]);
            } else {
              appendInlineMarkdown(li, item);
            }
            listEl.appendChild(li);
          });
          article.appendChild(listEl);
        } else {
          var p = document.createElement("p");
          appendInlineMarkdown(p, block.text);
          article.appendChild(p);
        }
      });
    }

    wrap.appendChild(article);
    container.appendChild(wrap);
  }

  function extractMarkdownSections(body) {
    var result = [];
    var heading = "Overview";
    var lines = [];
    var counts = {};
    var fenced = false;
    function flush() {
      var txt = lines.join("\n").trim();
      if (!txt) return;
      var secId = heading.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || "section";
      counts[secId] = (counts[secId] || 0) + 1;
      result.push({ heading: heading, text: txt, secId: secId, occurrence: counts[secId] });
    }
    String(body || "").replace(/\r\n?/g, "\n").split("\n").forEach(function (line) {
      if (/^\s*(```|~~~)/.test(line)) fenced = !fenced;
      var h = !fenced ? line.match(/^#{1,6}\s+(.+?)\s*$/) : null;
      if (h) { flush(); heading = h[1].replace(/[*_`]/g, "").trim(); lines = []; }
      else { lines.push(line); }
    });
    flush();
    return result;
  }

  function diffWords(planText, actualText) {
    var split = function (s) { return String(s || "").match(/\s+|\S+/g) || []; };
    var left = split(planText);
    var right = split(actualText);
    var p = left.filter(function (w) { return !/^\s+$/.test(w); });
    var a = right.filter(function (w) { return !/^\s+$/.test(w); });
    var sameP = new Set();
    var sameA = new Set();
    if (p.length * a.length <= 80000) {
      var width = a.length + 1;
      var table = new Uint16Array((p.length + 1) * width);
      for (var i = p.length - 1; i >= 0; i--) {
        for (var j = a.length - 1; j >= 0; j--) {
          table[i * width + j] = p[i] === a[j]
            ? table[(i + 1) * width + j + 1] + 1
            : Math.max(table[(i + 1) * width + j], table[i * width + j + 1]);
        }
      }
      var x = 0, y = 0;
      while (x < p.length && y < a.length) {
        if (p[x] === a[y]) { sameP.add(x++); sameA.add(y++); }
        else if (table[(x + 1) * width + y] >= table[x * width + y + 1]) x++;
        else y++;
      }
    } else {
      var start = 0;
      while (start < p.length && start < a.length && p[start] === a[start]) {
        sameP.add(start); sameA.add(start); start++;
      }
    }
    function mark(tokens, same) {
      var idx = 0;
      var out = [];
      tokens.forEach(function (tok) {
        var changed = /^\s+$/.test(tok) ? false : !same.has(idx++);
        var prev = out[out.length - 1];
        if (prev && prev.changed === changed) prev.text += tok;
        else out.push({ text: tok, changed: changed });
      });
      return out;
    }
    return { plan: mark(left, sameP), actual: mark(right, sameA) };
  }

  function buildComparisonRows(planBody, actualBody, bundle) {
    var rows = [];
    var syncReports = (bundle && bundle.sync_reports) || [];
    var latestReport = syncReports.length ? syncReports[syncReports.length - 1] : null;
    var requirements = (bundle && bundle.requirements) || [];

    if (latestReport && Array.isArray(latestReport.paired_rows) && latestReport.paired_rows.length) {
      latestReport.paired_rows.forEach(function (pr) {
        rows.push({
          id: pr.req_id,
          label: pr.req_id,
          status: pr.matched ? "matches" : (pr.observed && pr.observed.trim() ? "short" : "missing"),
          statusText: pr.matched ? "Matched (" + pr.stage + ")" : (pr.observed && pr.observed.trim() ? "Differs (" + pr.stage + ")" : "Missing"),
          planRef: "Requirement " + pr.req_id,
          actualRef: "Stage " + pr.stage,
          planText: pr.planned || "",
          actualText: pr.observed || "",
          reason: pr.matched ? "Recorded as-built item matches planned requirement." : "Review requirement evidence against the planned baseline."
        });
      });
    } else if (requirements.length) {
      requirements.forEach(function (req) {
        rows.push({
          id: req.req_id,
          label: req.req_id + " (" + req.stage + ")",
          status: req.stage === "L" || req.stage === "A" ? "matches" : "pending",
          statusText: "Stage " + req.stage,
          planRef: req.source_id || "Requirement",
          actualRef: req.evidence_ref ? "Evidence: " + req.evidence_ref : "No evidence yet",
          planText: req.statement || "",
          actualText: req.evidence_ref || "",
          reason: req.evidence_ref ? "Requirement tracked through stage " + req.stage + "." : "Awaiting recorded evidence."
        });
      });
    }

    var pSecs = extractMarkdownSections(planBody);
    var aSecs = extractMarkdownSections(actualBody);
    var usedActual = new Set();

    pSecs.forEach(function (left) {
      var right = aSecs.find(function (s) { return s.secId === left.secId && s.occurrence === left.occurrence; }) ||
                  aSecs.find(function (s) { return !usedActual.has(s) && s.secId === left.secId; });
      if (right) usedActual.add(right);
      var aText = right ? right.text : "";
      var norm = function (t) { return String(t || "").replace(/\s+/g, " ").trim(); };
      var status = !aText ? "missing" : (norm(left.text) === norm(aText) ? "matches" : "pending");
      var statusText = !aText ? "Missing in As-Built" : (status === "matches" ? "Matches plan" : "Needs review");
      rows.push({
        id: "sec-" + left.secId + "-" + left.occurrence,
        label: left.heading,
        status: status,
        statusText: statusText,
        planRef: left.heading,
        actualRef: right ? right.heading : "Not recorded in As-Built",
        planText: left.text,
        actualText: aText,
        reason: !aText
          ? "Planned section has no corresponding section in As-Built."
          : (status === "matches" ? "Saved section text matches." : "Wording differs between Blueprint and As-Built; review evidence below.")
      });
    });

    aSecs.forEach(function (right) {
      if (usedActual.has(right)) return;
      rows.push({
        id: "act-" + right.secId + "-" + right.occurrence,
        label: right.heading,
        status: "pending",
        statusText: "Added in As-Built",
        planRef: "Not in Blueprint",
        actualRef: right.heading,
        planText: "",
        actualText: right.text,
        reason: "Recorded in As-Built without a matching Blueprint heading."
      });
    });

    return { rows: rows, latestReport: latestReport };
  }

  async function refreshAllData() {
    var ws = await callTool("get_workspace", {});
    state.workspace = ws;
    state.shelves = Array.isArray(ws.shelves) ? ws.shelves : [];
    state.projects = Array.isArray(ws.projects) ? ws.projects : [];

    var results = await Promise.allSettled([
      callTool("list_resources", { include_deleted: false }),
      callTool("query_lessons", { limit: 100 }),
      callTool("list_skills", {})
    ]);
    state.resources = results[0].status === "fulfilled" && Array.isArray(results[0].value) ? results[0].value : [];
    state.lessons = results[1].status === "fulfilled" && Array.isArray(results[1].value) ? results[1].value : [];
    state.skillsInfo = results[2].status === "fulfilled" ? results[2].value : null;

    if (!state.selectedProjectId && state.projects.length) {
      state.selectedProjectId = state.projects[0].project_id;
    }
    if (state.selectedProjectId) {
      try {
        state.selectedBundle = await callTool("get_project_bundle", { project_id: state.selectedProjectId });
      } catch (_e) {
        state.selectedBundle = null;
      }
    } else {
      state.selectedBundle = null;
    }
    var profs = classifyResources().profiles;
    if (profs.length) {
      try {
        var profFull = await callTool("read_resource", { resource_id: profs[0].id });
        state.profileCachedId = profs[0].id;
        state.profileCachedBody = profFull.body_md || "";
      } catch (_e) {
        state.profileCachedId = "";
        state.profileCachedBody = "";
      }
    } else {
      state.profileCachedId = "";
      state.profileCachedBody = "";
    }
  }

  async function unlockWithCredential(rawCred, saveToStorage) {
    var trimmed = String(rawCred || "").trim();
    if (!trimmed) {
      showError("Enter your workspace key to open your private workspace.");
      return false;
    }
    state.loading = true;
    state.cred = trimmed;
    setPresence("observing", "Opening your workspace");
    clearBanners();
    render();
    try {
      var boot = await callToolRaw("bootstrap", {}, trimmed);
      state.bootstrap = boot;
      if (boot && !boot.rules_acked && boot.rules_sha256) {
        await callToolRaw("ack_rules", { rules_sha256: boot.rules_sha256 }, trimmed);
      }
      await refreshAllData();
      state.unlocked = true;
      if (saveToStorage) {
        try { localStorage.setItem(STORAGE_SLOT, trimmed); } catch (_e) {}
      }
      setPresence("ready", "Ready for your next action");
      return true;
    } catch (err) {
      state.unlocked = false;
      state.cred = "";
      try { localStorage.removeItem(STORAGE_SLOT); } catch (_e) {}
      showError(humanError(err));
      return false;
    } finally {
      state.loading = false;
      render();
    }
  }

  function navigate(nextView) {
    state.view = nextView;
    state.readerResource = null;
    state.readerHistory = [];
    state.readerHistoryOpen = false;
    state.mobileNav = false;
    clearBanners();
    render();
    window.scrollTo({ top: 0, behavior: "auto" });
  }

  async function openDocumentReader(res, openHistory) {
    var resData = await runAction("Opening document", async function () {
      return await callTool("read_resource", { resource_id: res.id });
    });
    if (!resData) return;
    state.readerResource = resData.resource;
    state.readerBody = resData.body_md || "";
    state.readerPassport = resData.passport || null;
    state.readerHistoryOpen = Boolean(openHistory);
    state.readerHistory = [];
    state.readerCompareOut = null;
    if (openHistory) {
      try {
        state.readerHistory = await callTool("resource_history", {
          shelf_slug: resData.resource.shelf_slug,
          slug: resData.resource.slug
        });
      } catch (_e) {
        state.readerHistory = [];
      }
    }
    render();
    window.scrollTo({ top: 0, behavior: "auto" });
  }

  function profilePromptText(source, exclusions) {
    var scope = source === "documents"
      ? "Review only the documents or pasted excerpts I provide for this task."
      : source === "current"
        ? "Review only this conversation. Do not search other sessions or files."
        : "Review this conversation and earlier authorized work conversations that this host makes available. State accessible sources first.";
    var excLine = exclusions && exclusions.trim()
      ? "Leave out these topics or sources: " + exclusions.trim() + "\n\n"
      : "";
    return "Help me build or improve my Streamline profile from work I have already done, rather than starting with a long questionnaire.\n\n" +
      scope + "\n\n" + excLine +
      "Identify supported facts about my roles, recurring work, tools, projects, responsibilities, audiences, goals and constraints. Cite the document or conversation behind each claim.\n\n" +
      "Show a short 'What I found / What needs confirmation / What I could not access' review. Keep communication preferences separate from biography. Discover existing authorized resources with list_resources and read snapshots before proposing edits.";
  }

  function classifyResources() {
    var nonDeleted = state.resources.filter(function (r) { return !r.deleted_at; });
    var profiles = nonDeleted.filter(function (r) {
      return r.shelf_slug === "profile" ||
        /profile|background|bio/i.test(r.slug) ||
        /profile/i.test(r.title);
    });
    var commSkills = nonDeleted.filter(function (r) {
      return r.shelf_slug === "skills" || /skill|communication|memo|email|writing/i.test(r.slug);
    });
    var prefs = nonDeleted.filter(function (r) {
      return r.shelf_slug === "preferences" || /pref|preference|voice|tone|style/i.test(r.slug);
    });
    return {
      all: nonDeleted,
      profiles: profiles,
      skills: commSkills,
      preferences: prefs
    };
  }

  function renderHeaderAndHeading() {
    var navBtns = document.querySelectorAll("#main-nav .sl-nav-btn");
    navBtns.forEach(function (btn) {
      var v = btn.getAttribute("data-view");
      var active = (state.view === v) || (state.view === "technical" && v === "apps");
      btn.classList.toggle("selected", active);
      if (active) btn.setAttribute("aria-current", "page");
      else btn.removeAttribute("aria-current");
    });

    var nav = byId("main-nav");
    var menuBtn = byId("mobile-menu-btn");
    if (nav) nav.className = state.mobileNav ? "sl-nav-open" : "sl-nav";
    if (menuBtn) menuBtn.setAttribute("aria-expanded", state.mobileNav ? "true" : "false");

    var privacyText = byId("workspace-privacy-text");
    var connPill = byId("workspace-conn-pill");
    if (privacyText) privacyText.textContent = "Private workspace";
    if (connPill) connPill.hidden = !state.unlocked;

    var titleEl = byId("page-title");
    var subEl = byId("page-subtitle");
    if (state.readerResource) {
      titleEl.textContent = state.readerResource.title;
      subEl.textContent = "Read your document and its saved versions.";
    } else {
      var meta = VIEW_META[state.view] || VIEW_META.home;
      titleEl.textContent = meta.title;
      subEl.textContent = meta.subtitle;
    }
  }

  function renderHome(root) {
    var cls = classifyResources();
    var home = el("div", "sh-home");

    var welcome = el("section", "panel sh-welcome");
    welcome.appendChild(el("span", "eyebrow", "Start here"));
    welcome.appendChild(el("h2", "", "Continue your work"));
    welcome.appendChild(el("p", "", "Keep your background, writing instructions and project documents here. Use them from the AI you already work with."));

    var conn = el("div", "sh-connection");
    conn.appendChild(iconEl("link", 20));
    var connText = el("div");
    var principal = state.workspace && state.workspace.principal_handle ? state.workspace.principal_handle : "Owner";
    connText.appendChild(el("strong", "", "Workspace ready for " + principal));
    var mcpEndpoint = window.location.origin + "/mcp";
    connText.appendChild(el("p", "", "Connected AI endpoint: " + mcpEndpoint));
    conn.appendChild(connText);
    var connBtn = el("button", "button secondary");
    connBtn.type = "button";
    connBtn.appendChild(document.createTextNode("Open Connected apps"));
    connBtn.appendChild(iconEl("arrow-right", 15));
    connBtn.addEventListener("click", function () { navigate("apps"); });
    conn.appendChild(connBtn);
    welcome.appendChild(conn);
    home.appendChild(welcome);

    var cards = el("div", "sh-cards");

    function addCard(iconName, title, statusText, desc, ctaLabel, onClick) {
      var card = el("button", "panel sh-card");
      card.type = "button";
      card.appendChild(iconEl(iconName, 25));
      var st = el("span", "sh-status");
      if (statusText === "Saved") st.appendChild(iconEl("check", 14));
      st.appendChild(document.createTextNode(statusText));
      card.appendChild(st);
      card.appendChild(el("h2", "", title));
      card.appendChild(el("p", "", desc));
      var act = el("span", "sh-action");
      act.appendChild(document.createTextNode(ctaLabel));
      act.appendChild(iconEl("arrow-right", 16));
      card.appendChild(act);
      card.addEventListener("click", onClick);
      cards.appendChild(card);
    }

    var pCount = cls.profiles.length;
    addCard(
      "user",
      "My profile",
      pCount ? (pCount === 1 ? "Saved" : pCount + " saved") : "Start here",
      pCount ? cls.profiles[0].title : "Let your AI build a first draft from work you choose, or write your profile directly.",
      pCount ? "Open my profile" : "Build my profile",
      function () { state.profileTab = "Background"; navigate("profile"); }
    );

    var sCount = cls.skills.length;
    addCard(
      "book",
      "Communication skills",
      sCount ? (sCount + " saved") : "Add when you need one",
      "Separate instructions for emails, memos, reports or different audiences. Your AI chooses what fits.",
      "Open communication skills",
      function () { state.profileTab = "Communication skills"; navigate("profile"); }
    );

    var projCount = state.projects.length;
    addCard(
      "layers",
      "Projects",
      projCount ? (projCount + " project" + (projCount === 1 ? "" : "s")) : "No active projects yet",
      "Compare planned blueprints beside what was built, track requirements, and approve HOW-IT-WORKS releases.",
      "Open projects",
      function () { navigate("projects"); }
    );

    var totalDocs = cls.all.length;
    var shelfCount = state.shelves.length;
    addCard(
      "file",
      "Library & shelves",
      totalDocs + " documents across " + shelfCount + " shelves",
      "Browse your standards, runbooks, architecture notes, inbox, and archived Explorer shelves.",
      "Open Library",
      function () { navigate("library"); }
    );

    home.appendChild(cards);
    root.appendChild(home);
  }

  function renderDocumentReader(root) {
    var res = state.readerResource;
    var reader = el("section", "dr-reader sl-document-surface");

    var header = el("div", "dr-header");
    var left = el("div");
    left.appendChild(el("h2", "", res.title));
    var meta = el("div", "dr-meta");
    meta.appendChild(el("span", "tag blue", res.shelf_slug + " / " + res.slug));
    meta.appendChild(el("span", "tag", "Version " + res.current_rev));
    meta.appendChild(el("span", "tag", res.lifecycle_tier));
    meta.appendChild(el("span", "", "Updated " + formatDate(res.updated_at)));
    left.appendChild(meta);
    header.appendChild(left);

    var actions = el("div", "dr-actions");
    var backBtn = el("button", "button secondary", "Back");
    backBtn.type = "button";
    backBtn.addEventListener("click", function () {
      state.readerResource = null;
      render();
    });
    actions.appendChild(backBtn);

    var histBtn = el("button", "button secondary", state.readerHistoryOpen ? "Hide history" : "History");
    histBtn.type = "button";
    histBtn.addEventListener("click", async function () {
      state.readerHistoryOpen = !state.readerHistoryOpen;
      if (state.readerHistoryOpen && !state.readerHistory.length) {
        var hist = await runAction("Loading version history", async function () {
          return await callTool("resource_history", { shelf_slug: res.shelf_slug, slug: res.slug });
        });
        if (hist) state.readerHistory = hist;
      }
      render();
    });
    actions.appendChild(histBtn);

    var editBtn = el("button", "button primary", "Edit");
    editBtn.type = "button";
    editBtn.addEventListener("click", function () {
      openEditorModal({
        mode: "update",
        shelf_slug: res.shelf_slug,
        slug: res.slug,
        title: res.title,
        body_md: state.readerBody || "",
        expected_rev: res.current_rev,
        project_id: res.project_id || null,
        trio_role: res.trio_role || null,
        lifecycle_tier: res.lifecycle_tier,
        triage_state: res.triage_state
      });
    });
    actions.appendChild(editBtn);
    header.appendChild(actions);
    reader.appendChild(header);

    if (state.readerHistoryOpen) {
      var histBox = el("div", "dr-history");
      histBox.appendChild(el("h3", "", "Saved version history"));
      if (!state.readerHistory.length) {
        histBox.appendChild(el("p", "", "No earlier versions found."));
      } else {
        state.readerHistory.slice().reverse().forEach(function (rev) {
          var row = el("div", "dr-version");
          var info = el("div");
          info.appendChild(el("strong", "", "Version " + rev.rev + (rev.rev === res.current_rev ? " (Current)" : "")));
          info.appendChild(el("span", "", (rev.summary || "Saved revision") + " - " + rev.author_principal + " - " + formatDate(rev.created_at)));
          row.appendChild(info);
          if (rev.rev !== res.current_rev) {
            var rBtns = el("div", "dr-actions");
            var cmpRevBtn = el("button", "button secondary", "Compare with current");
            cmpRevBtn.type = "button";
            cmpRevBtn.addEventListener("click", async function () {
              var diffOut = await runAction("Comparing versions", async function () {
                return await callTool("compare_resources", {
                  shelf_slug: res.shelf_slug,
                  slug: res.slug,
                  left_rev: rev.rev,
                  right_rev: res.current_rev
                });
              });
              if (diffOut) {
                state.readerCompareOut = diffOut;
                render();
              }
            });
            rBtns.appendChild(cmpRevBtn);
            var restoreBtn = el("button", "button secondary", "Restore as new version");
            restoreBtn.type = "button";
            restoreBtn.addEventListener("click", function () {
              openConfirmModal(
                "Restore version " + rev.rev + "?",
                "\"" + res.title + "\" will get a new version containing version " + rev.rev + ". Earlier versions stay in history.",
                "Restore as new version",
                async function () {
                  var restored = await runAction("Restoring version", async function () {
                    return await callTool("restore_resource", {
                      shelf_slug: res.shelf_slug,
                      slug: res.slug,
                      target_rev: rev.rev,
                      summary: "Restore version " + rev.rev + " from UI"
                    });
                  });
                  if (restored) {
                    await refreshAllData();
                    await openDocumentReader(restored.resource || res, true);
                    showNotice("Earlier version restored as a new version.");
                  }
                }
              );
            });
            rBtns.appendChild(restoreBtn);
            row.appendChild(rBtns);
          }
          histBox.appendChild(row);
        });
      }
      if (state.readerCompareOut) {
        var cBox = el("div", "pc-truth-note");
        cBox.style.marginTop = "16px";
        cBox.textContent = "Revision diff (v" + state.readerCompareOut.left_rev + " vs v" + state.readerCompareOut.right_rev + "): " + (state.readerCompareOut.summary || "") + " (Added lines: " + (state.readerCompareOut.added_lines || []).length + ", Removed lines: " + (state.readerCompareOut.removed_lines || []).length + ")";
        histBox.appendChild(cBox);
      }
      reader.appendChild(histBox);
    }

    var mdMount = el("div");
    renderMarkdownDocument(mdMount, state.readerBody || "", true);
    reader.appendChild(mdMount);

    var passportDetails = el("details", "dr-source-details");
    passportDetails.appendChild(el("summary", "", "Source Passport"));
    var pp = state.readerPassport;
    if (pp) {
      var grid = el("div", "dr-passport-grid");
      grid.appendChild(el("div", "", "Steward: " + pp.steward));
      grid.appendChild(el("div", "", "Freshness: " + pp.freshness_state));
      grid.appendChild(el("div", "", "Last verified: " + formatDate(pp.last_verified_at)));
      grid.appendChild(el("div", "", "Lineage: " + pp.lineage_ref));
      grid.appendChild(el("div", "", "Access scope: " + pp.access_scope));
      grid.appendChild(el("div", "", "Next action: " + pp.next_action));
      passportDetails.appendChild(grid);
    } else {
      passportDetails.appendChild(el("p", "field-help", "No Source Passport recorded yet for this document."));
    }
    var ppForm = el("form");
    ppForm.style.marginTop = "14px";
    var ppRow = el("div", "form-two");
    var ppStewardLbl = el("label", "", "Steward");
    var ppStewardInp = document.createElement("input");
    ppStewardInp.type = "text";
    ppStewardInp.value = (pp && pp.steward) || "owner";
    ppStewardLbl.appendChild(ppStewardInp);
    ppRow.appendChild(ppStewardLbl);
    var ppFreshLbl = el("label", "", "Freshness state");
    var ppFreshSel = document.createElement("select");
    ["verified", "stale", "drift", "unknown"].forEach(function (st) {
      var opt = document.createElement("option");
      opt.value = st;
      opt.textContent = st;
      if ((pp && pp.freshness_state) === st) opt.selected = true;
      ppFreshSel.appendChild(opt);
    });
    ppFreshLbl.appendChild(ppFreshSel);
    ppRow.appendChild(ppFreshLbl);
    ppForm.appendChild(ppRow);
    var ppNextLbl = el("label", "", "Next action");
    var ppNextInp = document.createElement("input");
    ppNextInp.type = "text";
    ppNextInp.value = (pp && pp.next_action) || "none";
    ppNextLbl.appendChild(ppNextInp);
    ppForm.appendChild(ppNextLbl);
    var ppSaveBtn = el("button", "button secondary", "Update Source Passport");
    ppSaveBtn.type = "submit";
    ppForm.appendChild(ppSaveBtn);
    ppForm.addEventListener("submit", async function (e) {
      e.preventDefault();
      var savedPp = await runAction("Updating Source Passport", async function () {
        return await callTool("update_source_passport", {
          resource_id: res.id,
          steward: ppStewardInp.value.trim() || "owner",
          freshness_state: ppFreshSel.value,
          lineage_ref: (pp && pp.lineage_ref) || ("ui:" + res.shelf_slug + "/" + res.slug),
          next_action: ppNextInp.value.trim() || "none"
        });
      });
      if (savedPp) {
        state.readerPassport = savedPp;
        showNotice("Source Passport updated.");
        render();
      }
    });
    passportDetails.appendChild(ppForm);
    reader.appendChild(passportDetails);
    root.appendChild(reader);
  }

  function renderProfileView(root) {
    var cls = classifyResources();
    var stack = el("div", "section-stack");

    var tabBar = el("div", "tabs");
    tabBar.setAttribute("role", "group");
    tabBar.setAttribute("aria-label", "Profile sections");
    ["Background", "Communication skills", "Preferences", "Improve my profile"].forEach(function (t) {
      var btn = el("button", "button " + (state.profileTab === t ? "primary" : "secondary"), t);
      btn.type = "button";
      btn.addEventListener("click", function () {
        state.profileTab = t;
        render();
      });
      tabBar.appendChild(btn);
    });
    stack.appendChild(tabBar);

    if (state.profileTab === "Background") {
      var panel = el("section", "panel sl-document-surface");
      var primary = cls.profiles[0] || null;
      if (primary) {
        var topRow = el("div", "row");
        var headLeft = el("div");
        headLeft.appendChild(el("h2", "", primary.title));
        headLeft.appendChild(el("p", "", "Version " + primary.current_rev + " - " + primary.shelf_slug + " / " + primary.slug));
        topRow.appendChild(headLeft);
        var headRight = el("div", "dr-actions");
        var hBtn = el("button", "button secondary", "History");
        hBtn.type = "button";
        hBtn.addEventListener("click", function () { void openDocumentReader(primary, true); });
        headRight.appendChild(hBtn);
        var eBtn = el("button", "button primary", "Edit profile");
        eBtn.type = "button";
        eBtn.addEventListener("click", function () { void openDocumentReader(primary, false); });
        headRight.appendChild(eBtn);
        var addPBtn = el("button", "button secondary", "+ Add profile note");
        addPBtn.type = "button";
        addPBtn.addEventListener("click", function () {
          openEditorModal({ mode: "create", shelf_slug: "standards", slug: "owner-profile", title: "My Background Profile", body_md: "" });
        });
        headRight.appendChild(addPBtn);
        topRow.appendChild(headRight);
        panel.appendChild(topRow);
        var mdInline = el("div");
        renderMarkdownDocument(mdInline, state.profileCachedId === primary.id ? state.profileCachedBody : "", true);
        panel.appendChild(mdInline);
      } else {
        var empty = el("div", "empty-state");
        empty.appendChild(el("h2", "", "Build your profile"));
        empty.appendChild(el("p", "", "Bring a profile you have, or let your AI help create one from work you choose."));
        var acts = el("div", "dr-actions");
        var copyPromptBtn = el("button", "button primary", "Build with my AI");
        copyPromptBtn.type = "button";
        copyPromptBtn.addEventListener("click", function () {
          copyText(profilePromptText("conversations", ""), "Profile prompt copied. Paste it into your connected AI.");
        });
        acts.appendChild(copyPromptBtn);
        var writeBtn = el("button", "button secondary", "Write my profile");
        writeBtn.type = "button";
        writeBtn.addEventListener("click", function () {
          openEditorModal({
            mode: "create",
            shelf_slug: "standards",
            slug: "owner-profile",
            title: "My Profile",
            body_md: "# Background\n\n## Roles and focus\n\n## Working tools\n\n## Constraints\n"
          });
        });
        acts.appendChild(writeBtn);
        empty.appendChild(acts);
        panel.appendChild(empty);
      }
      stack.appendChild(panel);
    } else if (state.profileTab === "Communication skills" || state.profileTab === "Preferences") {
      var isSkill = state.profileTab === "Communication skills";
      var items = isSkill ? cls.skills : cls.preferences;
      var listPanel = el("section", "panel");
      var lRow = el("div", "row");
      lRow.appendChild(el("h2", "", state.profileTab));
      var newBtn = el("button", "button primary", isSkill ? "+ Add communication skill" : "+ Add preference");
      newBtn.type = "button";
      newBtn.addEventListener("click", function () {
        openEditorModal({
          mode: "create",
          shelf_slug: isSkill ? "skills" : "standards",
          slug: isSkill ? "comm-skill-" + Date.now().toString(36) : "writing-preferences",
          title: isSkill ? "New Communication Skill" : "Communication Preferences",
          body_md: isSkill
            ? "# Audience and goal\n\n## Structure\n\n## Guardrails\n"
            : "# Writing and collaboration preferences\n\n- Keep answers concise and verified.\n"
        });
      });
      lRow.appendChild(newBtn);
      listPanel.appendChild(lRow);
      if (!items.length) {
        var em = el("div", "empty-state");
        em.appendChild(el("h3", "", isSkill ? "No communication skills yet" : "No preferences saved yet"));
        em.appendChild(el("p", "", isSkill
          ? "A skill tells your AI how to structure a particular email, memo, or technical report."
          : "Preferences tell your AI how you like decisions, updates, and formatting handled."));
        listPanel.appendChild(em);
      } else {
        var grid = el("div", "resource-grid");
        items.forEach(function (item) {
          var card = el("article", "panel resource-card");
          card.appendChild(el("span", "tag blue", item.shelf_slug + " / " + item.slug));
          var titleBtn = el("button", "resource-title", item.title);
          titleBtn.type = "button";
          titleBtn.addEventListener("click", function () { void openDocumentReader(item, false); });
          card.appendChild(titleBtn);
          var meta = el("div", "resource-meta");
          meta.appendChild(el("span", "", "Version " + item.current_rev));
          meta.appendChild(el("span", "", formatDate(item.updated_at)));
          card.appendChild(meta);
          grid.appendChild(card);
        });
        listPanel.appendChild(grid);
      }
      stack.appendChild(listPanel);
    } else {
      var impPanel = el("section", "panel");
      impPanel.appendChild(el("h2", "", "Improve my profile with my AI"));
      impPanel.appendChild(el("p", "field-help", "Copy this grounded prompt into your connected AI assistant so it reviews only authorized sources and proposes updates for your review."));
      var safety = el("div", "pl-safety");
      safety.appendChild(el("p", "", "Nothing is overwritten automatically. You review any candidate draft and save it as a new append-only version."));
      impPanel.appendChild(safety);

      var srcLabel = el("label", "", "Source scope");
      var srcSelect = document.createElement("select");
      [
        ["conversations", "Earlier authorized conversations and this chat"],
        ["documents", "Only documents or excerpts I paste"],
        ["current", "Only this conversation"]
      ].forEach(function (opt) {
        var o = document.createElement("option");
        o.value = opt[0];
        o.textContent = opt[1];
        srcSelect.appendChild(o);
      });
      srcLabel.appendChild(srcSelect);
      impPanel.appendChild(srcLabel);

      var excLabel = el("label", "", "Topics to exclude (optional)");
      var excInput = document.createElement("input");
      excInput.type = "text";
      excInput.placeholder = "e.g. personal health, old projects";
      excLabel.appendChild(excInput);
      impPanel.appendChild(excLabel);

      var promptArea = el("textarea", "pl-prompt-box");
      promptArea.readOnly = true;
      promptArea.value = profilePromptText(srcSelect.value, excInput.value);
      srcSelect.addEventListener("change", function () { promptArea.value = profilePromptText(srcSelect.value, excInput.value); });
      excInput.addEventListener("input", function () { promptArea.value = profilePromptText(srcSelect.value, excInput.value); });
      impPanel.appendChild(promptArea);

      var impActions = el("div", "dr-actions");
      impActions.style.marginTop = "16px";
      var buildBtn = el("button", "button primary", "Build with my AI (Copy prompt)");
      buildBtn.type = "button";
      buildBtn.addEventListener("click", function () {
        copyText(promptArea.value, "Profile builder prompt copied. Paste into your connected AI.");
      });
      impActions.appendChild(buildBtn);
      impPanel.appendChild(impActions);
      stack.appendChild(impPanel);
    }

    root.appendChild(stack);
  }

  function renderProjectsView(root) {
    var wrap = el("div", "tw-workspace");

    var intro = el("section", "panel tw-intro");
    var introCopy = el("div");
    introCopy.appendChild(el("span", "eyebrow", "Trio+ projects"));
    introCopy.appendChild(el("h2", "", "Plan beside what actually got done"));
    introCopy.appendChild(el("p", "", "Create structured Trio+ project packages, lock your plan baseline, compare Blueprint against As-Built, and approve HOW-IT-WORKS releases."));
    intro.appendChild(introCopy);

    var introBtns = el("div", "tw-actions");
    var newProjBtn = el("button", "button primary", "+ New Trio+ project");
    newProjBtn.type = "button";
    newProjBtn.addEventListener("click", openCreateProjectModal);
    introBtns.appendChild(newProjBtn);
    intro.appendChild(introBtns);
    wrap.appendChild(intro);

    if (!state.projects.length) {
      var empty = el("section", "panel empty-state");
      empty.appendChild(el("h3", "", "No Trio+ projects yet"));
      empty.appendChild(el("p", "", "Start your first project package to keep Blueprint, Handhold, As-Built, and HOW-IT-WORKS together."));
      var startBtn = el("button", "button primary", "+ New Trio+ project");
      startBtn.type = "button";
      startBtn.addEventListener("click", openCreateProjectModal);
      empty.appendChild(startBtn);
      wrap.appendChild(empty);
      root.appendChild(wrap);
      return;
    }

    var listSection = el("section", "panel");
    listSection.appendChild(el("h3", "", "All Trio+ projects (" + state.projects.length + ")"));
    var pList = el("div", "tw-project-list");
    pList.style.marginTop = "14px";
    state.projects.forEach(function (proj) {
      var stKey = String(proj.state || "").toUpperCase();
      var stInfo = PROJECT_STATUS_LABELS[stKey] || { label: proj.state, cls: "blue" };
      var row = el("button", "tw-project-row" + (proj.project_id === state.selectedProjectId ? " selected" : ""));
      row.type = "button";
      var left = el("div");
      left.appendChild(el("strong", "", proj.title));
      left.appendChild(el("small", "", proj.project_id + " - " + proj.profile + " (D" + proj.difficulty + "/R" + proj.risk + ")"));
      row.appendChild(left);
      var badges = el("div", "tw-actions");
      badges.appendChild(el("span", "tag " + stInfo.cls, stInfo.label));
      if (proj.plan_locked_rev) badges.appendChild(el("span", "tag green", "Plan locked v" + proj.plan_locked_rev));
      row.appendChild(badges);
      row.addEventListener("click", async function () {
        state.selectedProjectId = proj.project_id;
        state.compareOpen = false;
        state.comparePlanRev = null;
        state.compareActualRev = null;
        state.compareCheckResult = null;
        var bundle = await runAction("Opening project", async function () {
          return await callTool("get_project_bundle", { project_id: proj.project_id });
        });
        if (bundle) state.selectedBundle = bundle;
        render();
      });
      pList.appendChild(row);
    });
    listSection.appendChild(pList);
    wrap.appendChild(listSection);

    var bundle = state.selectedBundle;
    if (bundle && bundle.project) {
      var proj = bundle.project;
      var stKey = String(proj.state || "").toUpperCase();
      var stInfo = PROJECT_STATUS_LABELS[stKey] || { label: proj.state, cls: "blue" };
      var hero = el("section", "panel tw-hero");
      var hTop = el("div", "row");
      var hLeft = el("div");
      hLeft.appendChild(el("h2", "", proj.title));
      hLeft.appendChild(el("p", "field-help", "Project ID: " + proj.project_id + " - Profile " + proj.profile + " (D" + proj.difficulty + " / R" + proj.risk + ") - Work kind: " + proj.work_kind));
      hTop.appendChild(hLeft);
      var hBadges = el("div", "tw-actions");
      hBadges.appendChild(el("span", "tag " + stInfo.cls, stInfo.label));
      if (proj.plan_locked_rev) {
        hBadges.appendChild(el("span", "tag green", "Plan locked at rev " + proj.plan_locked_rev));
      } else {
        hBadges.appendChild(el("span", "tag", "Plan unlocked"));
      }
      hTop.appendChild(hBadges);
      hero.appendChild(hTop);

      if (stKey === "AUDIT_PENDING") {
        var auditNote = el("div", "inline-warning");
        auditNote.textContent = "HOW-IT-WORKS approved. Security audit is now pending before GitHub release.";
        hero.appendChild(auditNote);
      }

      var actBar = el("div", "tw-actions");
      var cmpBtn = el("button", "button primary", state.compareOpen ? "Hide Plan vs As-Built" : "Compare Plan vs As-Built");
      cmpBtn.type = "button";
      cmpBtn.id = "compare-plan-btn";
      cmpBtn.addEventListener("click", async function () {
        state.compareOpen = !state.compareOpen;
        if (state.compareOpen) {
          await loadComparisonHistories(bundle);
        }
        render();
      });
      actBar.appendChild(cmpBtn);

      if (!proj.plan_locked_rev) {
        var lockBtn = el("button", "button secondary", "Lock plan");
        lockBtn.type = "button";
        lockBtn.id = "lock-plan-btn";
        lockBtn.addEventListener("click", function () {
          openConfirmModal(
            "Lock plan for \"" + proj.title + "\"?",
            "Locking freezes the current plan baseline and OWN decisions so later edits record divergences instead of changing scope silently.",
            "Lock plan",
            async function () {
              var locked = await runAction("Locking plan", async function () {
                return await callTool("lock_project_plan", { project_id: proj.project_id });
              });
              if (locked) {
                await refreshAllData();
                state.selectedBundle = locked;
                showNotice("Plan locked. Later changes remain tracked.");
                render();
              }
            }
          );
        });
        actBar.appendChild(lockBtn);
      }

      if (stKey === "DOC_PENDING_OWNER") {
        var approveBtn = el("button", "button primary", "Approve HOW-IT-WORKS");
        approveBtn.type = "button";
        approveBtn.id = "approve-how-it-works-btn";
        approveBtn.addEventListener("click", async function () {
          var updatedBundle = await runAction("Approving HOW-IT-WORKS", async function () {
            return await callTool("approve_release", { project_id: proj.project_id });
          });
          if (updatedBundle) {
            await refreshAllData();
            state.selectedBundle = updatedBundle;
            var nextState = String((updatedBundle.project && updatedBundle.project.state) || "").toUpperCase();
            if (nextState === "AUDIT_PENDING") {
              showNotice("HOW-IT-WORKS approved. Security audit is now pending before GitHub release.");
            } else {
              showNotice("HOW-IT-WORKS approved and project marked done.");
            }
            render();
          }
        });
        actBar.appendChild(approveBtn);
      }

      hero.appendChild(actBar);

      var docsGrid = el("div", "tw-docs");
      (bundle.resources || []).forEach(function (res) {
        var roleKey = res.trio_role || "";
        var rMeta = TRIO_ROLE_META[roleKey] || { name: roleKey || "Document", desc: "Project document." };
        var dCard = el("button", "tw-doc-card");
        dCard.type = "button";
        var dText = el("div");
        dText.appendChild(el("strong", "", rMeta.name + ": " + res.title));
        dText.appendChild(el("p", "", rMeta.desc));
        dText.appendChild(el("small", "", "Rev " + res.current_rev + " - Updated " + formatDate(res.updated_at)));
        dCard.appendChild(dText);
        dCard.addEventListener("click", function () { void openDocumentReader(res, false); });
        docsGrid.appendChild(dCard);
      });
      hero.appendChild(docsGrid);

      if ((bundle.own_decisions && bundle.own_decisions.length) || (bundle.divergences && bundle.divergences.length)) {
        var extra = el("details", "dr-source-details");
        extra.appendChild(el("summary", "", "OWN decisions (" + (bundle.own_decisions || []).length + ") and divergences (" + (bundle.divergences || []).length + ")"));
        (bundle.own_decisions || []).forEach(function (own) {
          extra.appendChild(el("p", "field-help", own.own_id + ": " + own.statement + (own.locked_at ? " (Locked)" : "")));
        });
        (bundle.divergences || []).forEach(function (dv) {
          extra.appendChild(el("p", "field-help", dv.div_id + " [" + dv.status + "]: Designed \"" + dv.designed + "\" vs Observed \"" + dv.observed + "\""));
        });
        hero.appendChild(extra);
      }

      wrap.appendChild(hero);

      if (state.compareOpen) {
        wrap.appendChild(renderPlanComparison(bundle));
      }
    }

    root.appendChild(wrap);
  }

  async function loadComparisonHistories(bundle) {
    var resources = bundle.resources || [];
    var planRes = resources.find(function (r) { return r.trio_role === "blueprint"; });
    var actualRes = resources.find(function (r) { return r.trio_role === "as_built"; }) ||
                    resources.find(function (r) { return r.trio_role === "fix_record"; });
    var results = await Promise.allSettled([
      planRes ? callTool("resource_history", { shelf_slug: planRes.shelf_slug, slug: planRes.slug }) : Promise.resolve([]),
      actualRes ? callTool("resource_history", { shelf_slug: actualRes.shelf_slug, slug: actualRes.slug }) : Promise.resolve([])
    ]);
    state.comparePlanHistory = results[0].status === "fulfilled" && Array.isArray(results[0].value) ? results[0].value : [];
    state.compareActualHistory = results[1].status === "fulfilled" && Array.isArray(results[1].value) ? results[1].value : [];
    if (planRes && !state.comparePlanRev) {
      state.comparePlanRev = bundle.project.plan_locked_rev || planRes.current_rev;
    }
    if (actualRes && !state.compareActualRev) {
      state.compareActualRev = actualRes.current_rev;
    }
  }

  function renderPlanComparison(bundle) {
    var comp = el("section", "pc-comparison");
    var resources = bundle.resources || [];
    var planRes = resources.find(function (r) { return r.trio_role === "blueprint"; });
    var actualRes = resources.find(function (r) { return r.trio_role === "as_built"; }) ||
                    resources.find(function (r) { return r.trio_role === "fix_record"; });

    var head = el("div", "pc-heading");
    var headText = el("div");
    headText.appendChild(el("h2", "", "Plan vs As-Built"));
    headText.appendChild(el("p", "", "Read the saved Blueprint beside recorded As-Built work and requirement alignment."));
    head.appendChild(headText);

    if (actualRes) {
      var checkBtn = el("button", "button primary", "Run sync check");
      checkBtn.type = "button";
      checkBtn.addEventListener("click", async function () {
        var actualEntry = state.compareActualHistory.find(function (r) { return r.rev === state.compareActualRev; });
        var syncOut = await runAction("Checking plan alignment", async function () {
          return await callTool("check_sync", {
            project_id: bundle.project.project_id,
            as_built_markdown: actualEntry ? actualEntry.body_md : undefined,
            as_built_slug: actualRes.slug
          });
        });
        if (syncOut) {
          state.compareCheckResult = syncOut;
          showNotice("Plan alignment check completed.");
          render();
        }
      });
      head.appendChild(checkBtn);
    }
    comp.appendChild(head);

    var legend = el("div", "pc-legend");
    [
      ["matches", "Matched"],
      ["pending", "Needs review"],
      ["short", "Short of plan"],
      ["missing", "Missing"]
    ].forEach(function (pair) {
      legend.appendChild(el("span", "pc-chip " + pair[0], pair[1]));
    });
    comp.appendChild(legend);

    if (!planRes && !actualRes) {
      comp.appendChild(el("p", "pc-truth-note", "Blueprint and As-Built documents are not yet present in this project."));
      return comp;
    }

    var controls = el("div", "pc-controls");
    var diffLabel = el("label", "pc-toggle");
    var diffChk = document.createElement("input");
    diffChk.type = "checkbox";
    diffChk.checked = state.compareDiffsOnly;
    diffChk.addEventListener("change", function () { state.compareDiffsOnly = diffChk.checked; render(); });
    diffLabel.appendChild(diffChk);
    diffLabel.appendChild(document.createTextNode("Show only differences"));
    controls.appendChild(diffLabel);

    var syncLabel = el("label", "pc-toggle");
    var syncChk = document.createElement("input");
    syncChk.type = "checkbox";
    syncChk.checked = state.compareSyncScroll;
    syncChk.addEventListener("change", function () { state.compareSyncScroll = syncChk.checked; render(); });
    syncLabel.appendChild(syncChk);
    syncLabel.appendChild(document.createTextNode("Keep paired rows together"));
    controls.appendChild(syncLabel);
    comp.appendChild(controls);

    var srcGrid = el("div", "pc-source-grid");
    var pRevObj = state.comparePlanHistory.find(function (r) { return r.rev === state.comparePlanRev; }) || state.comparePlanHistory[state.comparePlanHistory.length - 1];
    var aRevObj = state.compareActualHistory.find(function (r) { return r.rev === state.compareActualRev; }) || state.compareActualHistory[state.compareActualHistory.length - 1];

    function buildSourceBox(title, resObj, history, selectedRev, onRevChange) {
      var box = el("div", "pc-source");
      box.appendChild(el("h3", "", title + (resObj ? ": " + resObj.title : " (missing)")));
      if (resObj && history.length) {
        var sa = el("div", "pc-source-actions");
        var lbl = el("label", "", title + " version");
        var sel = document.createElement("select");
        history.forEach(function (h) {
          var o = document.createElement("option");
          o.value = String(h.rev);
          o.textContent = "Version " + h.rev + (bundle.project.plan_locked_rev === h.rev && title === "Blueprint" ? " (Locked plan)" : "");
          if (h.rev === selectedRev) o.selected = true;
          sel.appendChild(o);
        });
        sel.addEventListener("change", function () { onRevChange(Number(sel.value)); });
        lbl.appendChild(sel);
        sa.appendChild(lbl);
        var openDoc = el("button", "button secondary", "Open");
        openDoc.type = "button";
        openDoc.addEventListener("click", function () { void openDocumentReader(resObj, false); });
        sa.appendChild(openDoc);
        box.appendChild(sa);
      }
      return box;
    }

    srcGrid.appendChild(buildSourceBox("Blueprint", planRes, state.comparePlanHistory, pRevObj ? pRevObj.rev : 1, function (v) { state.comparePlanRev = v; render(); }));
    srcGrid.appendChild(buildSourceBox("As-Built", actualRes, state.compareActualHistory, aRevObj ? aRevObj.rev : 1, function (v) { state.compareActualRev = v; render(); }));
    comp.appendChild(srcGrid);

    var planBody = pRevObj ? pRevObj.body_md : "";
    var actualBody = aRevObj ? aRevObj.body_md : "";
    var effectiveBundle = state.compareCheckResult
      ? Object.assign({}, bundle, { sync_reports: [state.compareCheckResult] })
      : bundle;
    var built = buildComparisonRows(planBody, actualBody, effectiveBundle);
    var rows = state.compareDiffsOnly
      ? built.rows.filter(function (r) { return r.status !== "matches"; })
      : built.rows;

    if (!rows.length) {
      comp.appendChild(el("p", "pc-truth-note", "No differences in this view."));
      return comp;
    }

    var rowsWrap = el("div", "pc-rows");
    rows.forEach(function (row) {
      var art = el("article", "pc-row");
      var rHead = el("header", "pc-row-head");
      rHead.appendChild(el("h3", "", row.label));
      rHead.appendChild(el("span", "pc-chip " + row.status, row.statusText));
      art.appendChild(rHead);

      var paired = el("div", "pc-paired");
      var leftPaneBody = el("div", "pc-pane-body" + (state.compareSyncScroll ? " synced" : ""));
      var rightPaneBody = el("div", "pc-pane-body" + (state.compareSyncScroll ? " synced" : ""));

      var leftPane = el("div", "pc-pane");
      var lLabel = el("div", "pc-pane-label");
      lLabel.appendChild(el("strong", "", "Blueprint"));
      lLabel.appendChild(el("span", "", row.planRef));
      leftPane.appendChild(lLabel);
      if (row.planText) renderMarkdownDocument(leftPaneBody, row.planText, false);
      else leftPaneBody.appendChild(el("p", "", "No matching plan text."));
      leftPane.appendChild(leftPaneBody);

      var rightPane = el("div", "pc-pane");
      var rLabel = el("div", "pc-pane-label");
      rLabel.appendChild(el("strong", "", "As-Built"));
      rLabel.appendChild(el("span", "", row.actualRef));
      rightPane.appendChild(rLabel);
      if (row.actualText) renderMarkdownDocument(rightPaneBody, row.actualText, false);
      else rightPaneBody.appendChild(el("p", "", "No matching actual text recorded."));
      rightPane.appendChild(rightPaneBody);

      leftPaneBody.addEventListener("scroll", function () {
        if (state.compareSyncScroll) rightPaneBody.scrollTop = leftPaneBody.scrollTop;
      });
      rightPaneBody.addEventListener("scroll", function () {
        if (state.compareSyncScroll) leftPaneBody.scrollTop = rightPaneBody.scrollTop;
      });

      paired.appendChild(leftPane);
      paired.appendChild(rightPane);
      art.appendChild(paired);

      art.appendChild(el("div", "pc-assessment", row.reason));

      if (row.planText && row.actualText && row.status !== "matches") {
        var wd = diffWords(row.planText, row.actualText);
        var det = el("details");
        var sum = el("summary", "", "Inspect word changes");
        sum.style.padding = "12px 18px";
        sum.style.cursor = "pointer";
        det.appendChild(sum);
        var wGrid = el("div", "pc-word-grid");
        var wLeft = el("div");
        wLeft.appendChild(el("h4", "", "In Blueprint"));
        var wLP = el("p");
        wd.plan.forEach(function (tok) {
          wLP.appendChild(tok.changed ? el("del", "", tok.text) : document.createTextNode(tok.text));
        });
        wLeft.appendChild(wLP);
        var wRight = el("div");
        wRight.appendChild(el("h4", "", "In As-Built"));
        var wRP = el("p");
        wd.actual.forEach(function (tok) {
          wRP.appendChild(tok.changed ? el("ins", "", tok.text) : document.createTextNode(tok.text));
        });
        wRight.appendChild(wRP);
        wGrid.appendChild(wLeft);
        wGrid.appendChild(wRight);
        det.appendChild(wGrid);
        art.appendChild(det);
      }

      rowsWrap.appendChild(art);
    });
    comp.appendChild(rowsWrap);
    return comp;
  }

  function renderLibraryView(root) {
    var stack = el("div", "section-stack");
    var toolbar = el("section", "panel");

    var shelfBar = el("div", "tabs");
    var allBtn = el("button", "button " + (state.libraryShelf === "all" ? "primary" : "secondary"), "All (" + state.resources.length + ")");
    allBtn.type = "button";
    allBtn.addEventListener("click", function () { state.libraryShelf = "all"; render(); });
    shelfBar.appendChild(allBtn);

    state.shelves.forEach(function (sh) {
      var btn = el("button", "button " + (state.libraryShelf === sh.slug ? "primary" : "secondary"), sh.title + " (" + sh.total_count + ")");
      btn.type = "button";
      btn.addEventListener("click", function () { state.libraryShelf = sh.slug; render(); });
      shelfBar.appendChild(btn);
    });
    toolbar.appendChild(shelfBar);

    var controls = el("div", "collection-toolbar");
    var searchWrap = el("label", "search-input");
    var searchInp = document.createElement("input");
    searchInp.type = "search";
    searchInp.placeholder = "Filter documents by title or slug...";
    searchInp.value = state.librarySearch;
    searchInp.addEventListener("input", function () {
      state.librarySearch = searchInp.value;
      renderLibraryCardsOnly();
    });
    searchWrap.appendChild(searchInp);
    controls.appendChild(searchWrap);

    var tierSelect = document.createElement("select");
    tierSelect.style.width = "auto";
    tierSelect.style.margin = "0";
    [
      ["all", "All lifecycle tiers"],
      ["active", "Active"],
      ["dormant", "Dormant"],
      ["legacy", "Legacy"],
      ["needs_triage", "Needs triage"],
      ["retired", "Retired"]
    ].forEach(function (opt) {
      var o = document.createElement("option");
      o.value = opt[0];
      o.textContent = opt[1];
      if (state.libraryTier === opt[0]) o.selected = true;
      tierSelect.appendChild(o);
    });
    tierSelect.addEventListener("change", function () {
      state.libraryTier = tierSelect.value;
      renderLibraryCardsOnly();
    });
    controls.appendChild(tierSelect);

    var newDocBtn = el("button", "button primary", "+ New document");
    newDocBtn.type = "button";
    newDocBtn.addEventListener("click", function () {
      openEditorModal({
        mode: "create",
        shelf_slug: state.libraryShelf !== "all" ? state.libraryShelf : "standards",
        slug: "",
        title: "",
        body_md: ""
      });
    });
    controls.appendChild(newDocBtn);
    toolbar.appendChild(controls);
    stack.appendChild(toolbar);

    var gridContainer = el("div");
    gridContainer.id = "library-cards-container";
    stack.appendChild(gridContainer);
    root.appendChild(stack);
    renderLibraryCardsOnly();
  }

  function renderLibraryCardsOnly() {
    var container = byId("library-cards-container");
    if (!container) return;
    clearEl(container);
    var q = state.librarySearch.trim().toLowerCase();
    var filtered = state.resources.filter(function (r) {
      if (state.libraryShelf !== "all" && r.shelf_slug !== state.libraryShelf) return false;
      if (state.libraryTier === "needs_triage" && r.triage_state !== "needs_triage") return false;
      if (state.libraryTier !== "all" && state.libraryTier !== "needs_triage" && r.lifecycle_tier !== state.libraryTier) return false;
      if (q && !(r.title + " " + r.slug + " " + r.shelf_slug).toLowerCase().includes(q)) return false;
      return true;
    });

    if (!filtered.length) {
      var empty = el("section", "panel empty-state");
      empty.appendChild(el("h3", "", "No matching documents"));
      empty.appendChild(el("p", "", "Try clearing your search or choose another shelf filter."));
      container.appendChild(empty);
      return;
    }

    var grid = el("div", "resource-grid");
    filtered.forEach(function (r) {
      var card = el("article", "panel resource-card");
      var top = el("div", "resource-card-top");
      top.appendChild(el("span", "tag blue", r.shelf_slug + " / " + r.slug));
      var tierCls = r.lifecycle_tier === "active" ? "green" : (r.lifecycle_tier === "dormant" ? "amber" : "");
      top.appendChild(el("span", "tag " + tierCls, r.lifecycle_tier + (r.triage_state === "needs_triage" ? " - Needs triage" : "")));
      card.appendChild(top);

      var tBtn = el("button", "resource-title", r.title);
      tBtn.type = "button";
      tBtn.addEventListener("click", function () { void openDocumentReader(r, false); });
      card.appendChild(tBtn);

      var meta = el("div", "resource-meta");
      meta.appendChild(el("span", "", "Rev " + r.current_rev));
      meta.appendChild(el("span", "", formatDate(r.updated_at)));
      card.appendChild(meta);

      var footer = el("div", "resource-card-footer");
      var tri = el("div", "triage-actions");
      if (r.lifecycle_tier !== "active" || r.triage_state === "needs_triage") {
        var promote = el("button", "triage-btn", "Promote to Active");
        promote.type = "button";
        promote.addEventListener("click", async function () {
          var ok = await runAction("Updating lifecycle tier", async function () {
            return await callTool("set_lifecycle_tier", {
              shelf_slug: r.shelf_slug,
              slug: r.slug,
              lifecycle_tier: "active",
              triage_state: "reviewed"
            });
          });
          if (ok) {
            await refreshAllData();
            showNotice("Document marked Active and reviewed.");
            render();
          }
        });
        tri.appendChild(promote);
      }
      if (r.lifecycle_tier !== "dormant") {
        var keepDormant = el("button", "triage-btn", "Keep Dormant");
        keepDormant.type = "button";
        keepDormant.addEventListener("click", async function () {
          var ok = await runAction("Marking Dormant", async function () {
            return await callTool("set_lifecycle_tier", {
              shelf_slug: r.shelf_slug,
              slug: r.slug,
              lifecycle_tier: "dormant",
              triage_state: "reviewed"
            });
          });
          if (ok) {
            await refreshAllData();
            showNotice("Document kept as Dormant.");
            render();
          }
        });
        tri.appendChild(keepDormant);
      }
      if (r.lifecycle_tier !== "retired") {
        var archiveBtn = el("button", "triage-btn", "Archive Retired");
        archiveBtn.type = "button";
        archiveBtn.addEventListener("click", async function () {
          var ok = await runAction("Archiving document", async function () {
            return await callTool("set_lifecycle_tier", {
              shelf_slug: r.shelf_slug,
              slug: r.slug,
              lifecycle_tier: "retired",
              triage_state: "reviewed"
            });
          });
          if (ok) {
            await refreshAllData();
            showNotice("Document archived as Retired.");
            render();
          }
        });
        tri.appendChild(archiveBtn);
      }
      footer.appendChild(tri);

      var openBtn = el("button", "text-button", "Read ->");
      openBtn.type = "button";
      openBtn.addEventListener("click", function () { void openDocumentReader(r, false); });
      footer.appendChild(openBtn);
      card.appendChild(footer);
      grid.appendChild(card);
    });
    container.appendChild(grid);
  }

  function renderLessonsView(root) {
    var stack = el("div", "section-stack");
    var bar = el("section", "panel");
    var controls = el("div", "collection-toolbar");
    var searchWrap = el("label", "search-input");
    var inp = document.createElement("input");
    inp.type = "search";
    inp.placeholder = "Search lessons, observed facts, or forward rules...";
    inp.value = state.lessonSearch;
    inp.addEventListener("input", function () {
      state.lessonSearch = inp.value;
      renderLessonCardsOnly();
    });
    searchWrap.appendChild(inp);
    controls.appendChild(searchWrap);

    var addBtn = el("button", "button primary", "+ Record lesson");
    addBtn.type = "button";
    addBtn.addEventListener("click", function () { byId("lesson-modal").showModal(); });
    controls.appendChild(addBtn);
    bar.appendChild(controls);

    var tagSet = new Set();
    state.lessons.forEach(function (l) { (l.tags || []).forEach(function (t) { tagSet.add(t); }); });
    if (tagSet.size) {
      var tagTabs = el("div", "tabs");
      var allTag = el("button", "button " + (state.lessonTag === "all" ? "primary" : "secondary"), "All tags");
      allTag.type = "button";
      allTag.addEventListener("click", function () { state.lessonTag = "all"; render(); });
      tagTabs.appendChild(allTag);
      Array.from(tagSet).sort().forEach(function (tag) {
        var tBtn = el("button", "button " + (state.lessonTag === tag ? "primary" : "secondary"), tag);
        tBtn.type = "button";
        tBtn.addEventListener("click", function () { state.lessonTag = tag; render(); });
        tagTabs.appendChild(tBtn);
      });
      bar.appendChild(tagTabs);
    }
    stack.appendChild(bar);

    var listMount = el("div");
    listMount.id = "lessons-cards-mount";
    stack.appendChild(listMount);
    root.appendChild(stack);
    renderLessonCardsOnly();
  }

  function renderLessonCardsOnly() {
    var container = byId("lessons-cards-mount");
    if (!container) return;
    clearEl(container);
    var q = state.lessonSearch.trim().toLowerCase();
    var rows = state.lessons.filter(function (l) {
      if (state.lessonTag !== "all" && (!l.tags || l.tags.indexOf(state.lessonTag) === -1)) return false;
      if (q && !(l.observed_fact + " " + l.forward_rule + " " + l.cause_note + " " + (l.tags || []).join(" ")).toLowerCase().includes(q)) return false;
      return true;
    });
    if (!rows.length) {
      var empty = el("section", "panel empty-state");
      empty.appendChild(el("h3", "", "No forward lessons match"));
      empty.appendChild(el("p", "", "Record reusable findings across builds and audits to prevent repeat mistakes."));
      container.appendChild(empty);
      return;
    }
    var grid = el("div", "resource-grid");
    rows.forEach(function (l) {
      var card = el("article", "panel resource-card");
      var top = el("div", "resource-card-top");
      top.appendChild(el("span", "tag blue", l.cause_kind || "lesson"));
      top.appendChild(el("span", "tag green", l.validation_state || "validated"));
      card.appendChild(top);
      card.appendChild(el("h3", "", l.forward_rule));
      card.appendChild(el("p", "", "Observed: " + l.observed_fact));
      if (l.cause_note) card.appendChild(el("p", "field-help", "Cause: " + l.cause_note));
      var meta = el("div", "resource-meta");
      if (l.tags && l.tags.length) meta.appendChild(el("span", "", l.tags.join(", ")));
      if (l.project_id) meta.appendChild(el("span", "", "Project: " + l.project_id));
      meta.appendChild(el("span", "", formatDate(l.created_at)));
      card.appendChild(meta);
      grid.appendChild(card);
    });
    container.appendChild(grid);
  }

  function renderConnectedAppsView(root) {
    var stack = el("div", "section-stack");
    var mcpUrl = window.location.origin + "/mcp";

    var hero = el("section", "panel connection-hero");
    var sym = el("div", "connection-symbol");
    sym.textContent = "MCP";
    hero.appendChild(sym);
    var copy = el("div");
    copy.appendChild(el("span", "eyebrow", "Model Context Protocol"));
    copy.appendChild(el("h2", "", "Connect your AI assistant"));
    copy.appendChild(el("p", "", "Use this private MCP endpoint and downloadable skills package in Claude, Codex, Cursor, or Gemini."));
    hero.appendChild(copy);
    var techBtn = el("button", "button secondary", "Technical details (for setup help)");
    techBtn.type = "button";
    techBtn.addEventListener("click", function () { navigate("technical"); });
    hero.appendChild(techBtn);
    stack.appendChild(hero);

    var grid = el("div", "settings-grid");

    var left = el("section", "panel");
    left.appendChild(el("h2", "", "MCP server address"));
    left.appendChild(el("p", "", "Copy your workspace MCP URL and authenticate with your granted credential header."));
    var endpoint = el("div", "endpoint");
    endpoint.appendChild(el("code", "", mcpUrl));
    var copyUrlBtn = el("button", "button primary", "Copy URL");
    copyUrlBtn.type = "button";
    copyUrlBtn.addEventListener("click", function () {
      copyText(mcpUrl, "MCP URL copied to clipboard.");
    });
    endpoint.appendChild(copyUrlBtn);
    left.appendChild(endpoint);

    var steps = el("div", "connection-steps");
    [
      "Copy the MCP URL above into your AI assistant connector settings.",
      "Provide your granted credential in the Authorization Bearer or X-Streamline-Credential header.",
      "Your assistant calls bootstrap and respects your active shelf and tag permissions."
    ].forEach(function (stepText, idx) {
      var row = el("div");
      row.appendChild(el("span", "", String(idx + 1)));
      row.appendChild(el("p", "", stepText));
      steps.appendChild(row);
    });
    left.appendChild(steps);
    grid.appendChild(left);

    var right = el("section", "panel");
    right.appendChild(el("h2", "", "Portable Streamline skills"));
    right.appendChild(el("p", "", "Download the bundled workflow guides so any connected assistant follows your Trio+ and profile conventions."));
    var dlLink = el("a", "button primary", "Download streamline-skills.zip");
    dlLink.setAttribute("href", "/downloads/streamline-skills.zip");
    dlLink.setAttribute("download", "streamline-skills.zip");
    dlLink.style.marginTop = "16px";
    right.appendChild(dlLink);

    var bundled = (state.skillsInfo && state.skillsInfo.bundled_skills) || [];
    if (bundled.length) {
      var ul = el("ul");
      ul.style.marginTop = "18px";
      ul.style.paddingLeft = "20px";
      bundled.forEach(function (sk) {
        var li = el("li");
        li.textContent = (sk.name || sk.slug || "Skill") + (sk.summary ? " - " + sk.summary : "");
        ul.appendChild(li);
      });
      right.appendChild(ul);
    }
    grid.appendChild(right);
    stack.appendChild(grid);
    root.appendChild(stack);
  }

  function renderTechnicalDetailsView(root) {
    var ws = state.workspace || {};
    var stack = el("div", "section-stack");
    var card = el("section", "panel");
    var top = el("div", "row");
    top.appendChild(el("h2", "", "Workspace access summary"));
    var back = el("button", "button secondary", "Back to Connected apps");
    back.type = "button";
    back.addEventListener("click", function () { navigate("apps"); });
    top.appendChild(back);
    card.appendChild(top);

    var list = el("div", "dr-passport-grid");
    list.appendChild(el("div", "", "Principal handle: " + (ws.principal_handle || "unknown")));
    list.appendChild(el("div", "", "Role: " + (ws.principal_role || "unknown")));
    list.appendChild(el("div", "", "Write access: " + (ws.can_write ? "Enabled" : "Read-only")));
    list.appendChild(el("div", "", "Dispatch access: " + (ws.can_dispatch ? "Enabled" : "Disabled")));
    list.appendChild(el("div", "", "Public-safe mode: " + (ws.public_safe_mode ? "On" : "Off")));
    list.appendChild(el("div", "", "Granted tags: " + ((ws.granted_tags || []).join(", ") || "none")));
    card.appendChild(list);
    stack.appendChild(card);
    root.appendChild(stack);
  }

  function renderSettingsView(root) {
    var grid = el("div", "settings-grid");

    var disp = el("section", "panel");
    disp.appendChild(el("h2", "", "Display preferences"));
    disp.appendChild(el("p", "", "Adjust cosmic background motion and visual density."));
    var calmRow = el("label", "checkbox-row");
    var calmChk = document.createElement("input");
    calmChk.type = "checkbox";
    calmChk.id = "calm-mode-checkbox";
    calmChk.checked = state.calm;
    calmChk.addEventListener("change", function () {
      applyCalmMode(calmChk.checked);
      showNotice(calmChk.checked ? "Calm mode enabled." : "Calm mode disabled.");
    });
    calmRow.appendChild(calmChk);
    calmRow.appendChild(document.createTextNode("Calm mode (hide decorative nebula, grain, and streaks)"));
    disp.appendChild(calmRow);

    var lockBrowserBtn = el("button", "button danger", "Lock workspace on this browser");
    lockBrowserBtn.type = "button";
    lockBrowserBtn.style.marginTop = "16px";
    lockBrowserBtn.addEventListener("click", function () {
      try { localStorage.removeItem(STORAGE_SLOT); } catch (_e) {}
      state.cred = "";
      state.unlocked = false;
      state.workspace = null;
      state.resources = [];
      state.projects = [];
      state.selectedBundle = null;
      state.readerResource = null;
      setPresence("ready", "Ready for your next action");
      render();
      showNotice("Workspace locked on this browser.");
    });
    disp.appendChild(lockBrowserBtn);
    grid.appendChild(disp);

    var vault = el("section", "panel");
    vault.appendChild(el("h2", "", "Workspace export and vault sync"));
    vault.appendChild(el("p", "", "Synchronize your local Obsidian vault mirror or download a JSON archive of your authorized workspace."));
    var vBtns = el("div", "dr-actions");
    vBtns.style.marginTop = "18px";

    var syncBtn = el("button", "button primary", "Sync Obsidian vault");
    syncBtn.type = "button";
    syncBtn.addEventListener("click", async function () {
      var res = await runAction("Syncing Obsidian vault", async function () {
        return await callTool("sync_vault", {});
      });
      if (res) {
        await refreshAllData();
        showNotice("Obsidian vault sync complete.");
        render();
      }
    });
    vBtns.appendChild(syncBtn);

    var exportBtn = el("button", "button secondary", "Download workspace");
    exportBtn.type = "button";
    exportBtn.addEventListener("click", async function () {
      var data = await runAction("Preparing workspace export", async function () {
        return await callTool("export_workspace", { include_bodies: true });
      });
      if (data) {
        downloadFile(JSON.stringify(data, null, 2), "Streamline_workspace.json", "application/json");
        showNotice("Workspace exported to Streamline_workspace.json.");
      }
    });
    vBtns.appendChild(exportBtn);

    var zipLink = el("a", "button secondary", "Download skills archive");
    zipLink.setAttribute("href", "/downloads/streamline-skills.zip");
    zipLink.setAttribute("download", "streamline-skills.zip");
    vBtns.appendChild(zipLink);

    vault.appendChild(vBtns);
    grid.appendChild(vault);
    root.appendChild(grid);
  }

  function render() {
    renderHeaderAndHeading();
    var unlockSec = byId("unlock-section");
    var viewEl = byId("app-view");

    if (!state.unlocked) {
      unlockSec.hidden = false;
      viewEl.hidden = true;
      return;
    }

    unlockSec.hidden = true;
    viewEl.hidden = false;
    clearEl(viewEl);

    if (state.loading) {
      var sk = el("section", "panel skeleton");
      sk.setAttribute("role", "status");
      sk.appendChild(el("h2", "", "Opening your workspace"));
      sk.appendChild(el("div"));
      sk.appendChild(el("div"));
      sk.appendChild(el("div"));
      viewEl.appendChild(sk);
      return;
    }

    if (state.readerResource) {
      renderDocumentReader(viewEl);
      return;
    }

    if (state.view === "home") renderHome(viewEl);
    else if (state.view === "profile") renderProfileView(viewEl);
    else if (state.view === "projects") renderProjectsView(viewEl);
    else if (state.view === "library") renderLibraryView(viewEl);
    else if (state.view === "lessons") renderLessonsView(viewEl);
    else if (state.view === "apps") renderConnectedAppsView(viewEl);
    else if (state.view === "technical") renderTechnicalDetailsView(viewEl);
    else if (state.view === "settings") renderSettingsView(viewEl);
  }

  function openEditorModal(ctx) {
    state.editorContext = ctx;
    state.editorMode = "edit";
    var dlg = byId("editor-modal");
    byId("editor-modal-title").textContent = ctx.mode === "update" ? "Edit document" : "Create document";
    var shelfSel = byId("editor-shelf");
    clearEl(shelfSel);
    var shelves = state.shelves.length ? state.shelves : [
      { slug: "standards", title: "Standards" },
      { slug: "runbooks", title: "Runbooks" },
      { slug: "architecture", title: "Architecture" },
      { slug: "projects", title: "Projects" },
      { slug: "skills", title: "Skills" },
      { slug: "inbox", title: "Inbox" },
      { slug: "archive", title: "Archive" }
    ];
    shelves.forEach(function (sh) {
      var o = document.createElement("option");
      o.value = sh.slug;
      o.textContent = sh.title + " (" + sh.slug + ")";
      if (sh.slug === ctx.shelf_slug) o.selected = true;
      shelfSel.appendChild(o);
    });
    shelfSel.disabled = ctx.mode === "update";
    var slugInp = byId("editor-slug");
    slugInp.value = ctx.slug || "";
    slugInp.disabled = ctx.mode === "update";
    byId("editor-title").value = ctx.title || "";
    byId("editor-body").value = ctx.body_md || "";
    byId("editor-summary").value = ctx.mode === "update" ? "Update document from UI" : "Create document from UI";
    syncEditorTab("edit");
    dlg.showModal();
  }

  function syncEditorTab(mode) {
    state.editorMode = mode;
    var editTab = byId("editor-tab-edit");
    var prevTab = byId("editor-tab-preview");
    var bodyField = byId("editor-body-wrap");
    var prevBox = byId("editor-preview-box");
    editTab.setAttribute("aria-selected", mode === "edit" ? "true" : "false");
    prevTab.setAttribute("aria-selected", mode === "preview" ? "true" : "false");
    bodyField.hidden = mode !== "edit";
    prevBox.hidden = mode !== "preview";
    if (mode === "preview") {
      renderMarkdownDocument(prevBox, byId("editor-body").value, false);
    }
  }

  function openCreateProjectModal() {
    var dlg = byId("create-project-modal");
    byId("cp-title").value = "";
    byId("cp-id").value = "";
    byId("cp-id").dataset.manual = "";
    byId("cp-kind").value = "software";
    byId("cp-diff").value = "2";
    byId("cp-risk").value = "2";
    byId("cp-has-ui").checked = true;
    byId("cp-github-release").checked = false;
    byId("cp-mod-budget").checked = false;
    byId("cp-mod-schedule").checked = false;
    byId("cp-own").value = "";
    dlg.showModal();
  }

  function openConfirmModal(title, message, confirmLabel, onConfirm) {
    var dlg = byId("confirm-modal");
    byId("confirm-modal-title").textContent = title;
    byId("confirm-modal-body").textContent = message;
    var okBtn = byId("confirm-modal-ok");
    okBtn.textContent = confirmLabel || "Confirm";
    okBtn.onclick = async function () {
      dlg.close();
      await onConfirm();
    };
    dlg.showModal();
  }

  function bindEvents() {
    byId("brand-home-btn").addEventListener("click", function () { navigate("home"); });
    byId("mobile-menu-btn").addEventListener("click", function () {
      state.mobileNav = !state.mobileNav;
      renderHeaderAndHeading();
    });

    document.querySelectorAll("#main-nav .sl-nav-btn").forEach(function (btn) {
      btn.addEventListener("click", function () {
        if (!state.unlocked) return;
        navigate(btn.getAttribute("data-view") || "home");
      });
    });

    byId("banner-error-dismiss").addEventListener("click", function () { byId("banner-error").hidden = true; });
    byId("banner-notice-dismiss").addEventListener("click", function () { byId("banner-notice").hidden = true; });

    byId("unlock-form").addEventListener("submit", async function (e) {
      e.preventDefault();
      var credVal = byId("workspace-cred-input").value;
      var ok = await unlockWithCredential(credVal, true);
      if (ok) byId("workspace-cred-input").value = "";
    });

    byId("presence-stage-btn").addEventListener("click", function () {
      if (state.presenceCtrl && typeof state.presenceCtrl.reactToClick === "function") {
        state.presenceCtrl.reactToClick();
      }
    });

    byId("presence-pause-btn").addEventListener("click", function () {
      state.motionPaused = !state.motionPaused;
      try { localStorage.setItem(MOTION_STORAGE, String(state.motionPaused)); } catch (_e) {}
      updateMotionControls();
    });

    byId("editor-tab-edit").addEventListener("click", function () { syncEditorTab("edit"); });
    byId("editor-tab-preview").addEventListener("click", function () { syncEditorTab("preview"); });
    byId("editor-close").addEventListener("click", function () { byId("editor-modal").close(); });
    byId("editor-cancel").addEventListener("click", function () { byId("editor-modal").close(); });

    byId("editor-title").addEventListener("input", function () {
      var slugInp = byId("editor-slug");
      if (!slugInp.disabled && !slugInp.dataset.manual) {
        slugInp.value = slugify(byId("editor-title").value);
      }
    });
    byId("editor-slug").addEventListener("input", function () {
      byId("editor-slug").dataset.manual = "1";
    });

    byId("editor-form").addEventListener("submit", async function (e) {
      e.preventDefault();
      var ctx = state.editorContext || {};
      var shelf = byId("editor-shelf").value;
      var slug = byId("editor-slug").value.trim() || slugify(byId("editor-title").value);
      var title = byId("editor-title").value.trim();
      var body = byId("editor-body").value;
      var summary = byId("editor-summary").value.trim() || "Save document";
      if (!title || !slug) {
        showError("Provide both a title and a slug.");
        return;
      }
      var saved = await runAction("Saving document", async function () {
        return await callTool("save_resource", {
          shelf_slug: shelf,
          slug: slug,
          title: title,
          body_md: body,
          project_id: ctx.project_id || undefined,
          trio_role: ctx.trio_role || undefined,
          lifecycle_tier: ctx.lifecycle_tier || "active",
          triage_state: ctx.triage_state || "reviewed",
          summary: summary,
          expected_rev: ctx.mode === "update" ? ctx.expected_rev : undefined
        });
      });
      if (saved) {
        byId("editor-modal").close();
        await refreshAllData();
        if (state.readerResource) {
          await openDocumentReader(saved, state.readerHistoryOpen);
        } else {
          render();
        }
        showNotice("Saved version " + saved.current_rev + " of \"" + saved.title + "\".");
      }
    });

    byId("cp-title").addEventListener("input", function () {
      var idInp = byId("cp-id");
      if (!idInp.dataset.manual) idInp.value = slugify(byId("cp-title").value);
    });
    byId("cp-id").addEventListener("input", function () {
      byId("cp-id").dataset.manual = "1";
    });
    byId("cp-close").addEventListener("click", function () { byId("create-project-modal").close(); });
    byId("cp-cancel").addEventListener("click", function () { byId("create-project-modal").close(); });

    byId("create-project-form").addEventListener("submit", async function (e) {
      e.preventDefault();
      var title = byId("cp-title").value.trim();
      var pid = byId("cp-id").value.trim() || slugify(title);
      var diff = Math.max(0, Math.min(4, Number(byId("cp-diff").value) || 0));
      var risk = Math.max(0, Math.min(4, Number(byId("cp-risk").value) || 0));
      var workKind = byId("cp-kind").value;
      var hasUi = byId("cp-has-ui").checked;
      var githubRelease = byId("cp-github-release").checked;
      var modules = [];
      if (byId("cp-mod-budget").checked) modules.push("budget");
      if (byId("cp-mod-schedule").checked) modules.push("schedule");
      var ownStmt = byId("cp-own").value.trim();
      var initialOwn = ownStmt ? [{ own_id: "OWN-1", statement: ownStmt }] : [];

      if (!title || !pid) {
        showError("Provide a project title and project ID.");
        return;
      }

      var bundle = await runAction("Creating Trio+ project", async function () {
        return await callTool("create_trio_project", {
          project_id: pid,
          title: title,
          github_release: githubRelease,
          opt_in_modules: modules,
          initial_own_decisions: initialOwn,
          assessment: {
            d1_scope: diff,
            d2_interfaces: diff,
            d3_uncertainty: diff,
            d4_ui: hasUi ? diff : 0,
            d5_novelty: diff,
            r1_exposure: risk,
            r2_irreversibility: risk,
            r3_blast_radius: risk,
            r4_evidence: risk,
            r5_custody: risk,
            work_kind: workKind,
            has_ui: hasUi,
            is_game: false,
            under_accepted_design: false
          }
        });
      });

      if (bundle) {
        byId("create-project-modal").close();
        state.selectedProjectId = bundle.project.project_id;
        state.selectedBundle = bundle;
        await refreshAllData();
        state.view = "projects";
        render();
        showNotice("Created Trio+ project \"" + bundle.project.title + "\".");
      }
    });

    byId("lesson-close").addEventListener("click", function () { byId("lesson-modal").close(); });
    byId("lesson-cancel").addEventListener("click", function () { byId("lesson-modal").close(); });
    byId("lesson-form").addEventListener("submit", async function (e) {
      e.preventDefault();
      var observed = byId("ls-observed").value.trim();
      var forward = byId("ls-forward").value.trim();
      var causeKind = byId("ls-kind").value.trim() || "process";
      var causeNote = byId("ls-cause").value.trim();
      var tags = byId("ls-tags").value.split(",").map(function (t) { return t.trim(); }).filter(Boolean);
      var pid = byId("ls-project").value.trim() || undefined;
      if (!observed || !forward) {
        showError("Provide both an observed fact and a forward rule.");
        return;
      }
      var saved = await runAction("Recording lesson", async function () {
        return await callTool("record_lesson", {
          project_id: pid,
          tags: tags,
          observed_fact: observed,
          cause_kind: causeKind,
          cause_note: causeNote,
          forward_rule: forward
        });
      });
      if (saved) {
        byId("lesson-modal").close();
        byId("ls-observed").value = "";
        byId("ls-forward").value = "";
        byId("ls-cause").value = "";
        await refreshAllData();
        render();
        showNotice("Lesson recorded.");
      }
    });

    byId("confirm-modal-cancel").addEventListener("click", function () { byId("confirm-modal").close(); });
    byId("confirm-modal-close").addEventListener("click", function () { byId("confirm-modal").close(); });

    window.addEventListener("streamline:presence-plugin-ready", mountPresenceIfReady);
  }

  function init() {
    try {
      state.calm = localStorage.getItem(CALM_STORAGE) === "true";
      state.motionPaused = localStorage.getItem(MOTION_STORAGE) === "true";
    } catch (_e) {}
    applyCalmMode(state.calm);
    bindEvents();
    mountPresenceIfReady();
    updateMotionControls();

    var storedCred = "";
    try { storedCred = localStorage.getItem(STORAGE_SLOT) || ""; } catch (_e) {}
    if (storedCred.trim()) {
      void unlockWithCredential(storedCred, false);
    } else {
      render();
    }
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", init);
  } else {
    init();
  }
})();
"###;

/// Renders the Streamline Web UI at `GET /`.
pub async fn index_handler(
    State(state): State<AppState>,
    Query(_query): Query<UiQueryParams>,
) -> Html<String> {
    let has_presence_plugin = match state
        .config
        .presence_plugin_path()
        .map(std::path::Path::to_path_buf)
    {
        Some(plugin_dir) => {
            tokio::task::spawn_blocking(move || crate::is_verified_presence_plugin(&plugin_dir))
                .await
                .unwrap_or(false)
        }
        None => false,
    };

    let presence_stage_inner = if has_presence_plugin {
        r#"<canvas id="presence-field-canvas" class="sl-field" aria-hidden="true"></canvas><canvas id="presence-body-canvas" class="sl-body" aria-hidden="true"></canvas><img id="presence-fallback-img" class="sl-fallback-mark" src="/_streamline/assets/mark.svg" alt="Streamline orb" width="88" height="88" hidden>"#
    } else {
        r#"<img id="presence-fallback-img" class="sl-fallback-mark" src="/_streamline/assets/mark.svg" alt="Streamline orb" width="88" height="88">"#
    };

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
  <title>Streamline</title>
  <style>{ui_css}</style>
</head>
<body>
  <div id="app-root" class="dashboard streamline-app sl-root" data-calm="false">
    <a class="skip-link" href="#main-content">Skip to content</a>

    <header class="sl-header">
      <button id="brand-home-btn" type="button" class="sl-wordmark">Streamline</button>
      <button id="mobile-menu-btn" type="button" class="icon-button sl-menu-toggle" aria-label="Open navigation" aria-expanded="false">
        <svg width="21" height="21" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><line x1="4" x2="20" y1="12" y2="12"/><line x1="4" x2="20" y1="6" y2="6"/><line x1="4" x2="20" y1="18" y2="18"/></svg>
      </button>
      <nav id="main-nav" class="sl-nav" aria-label="Main navigation">
        <button type="button" class="sl-nav-btn selected" data-view="home" aria-current="page">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="m12.83 2.18a2 2 0 0 0-1.66 0L2.6 6.08a1 1 0 0 0 0 1.83l8.58 3.91a2 2 0 0 0 1.66 0l8.58-3.9a1 1 0 0 0 0-1.83Z"/><path d="m22 17.65-9.17 4.16a2 2 0 0 1-1.66 0L2 17.65"/><path d="m22 12.65-9.17 4.16a2 2 0 0 1-1.66 0L2 12.65"/></svg>
          <span>Home</span>
        </button>
        <button type="button" class="sl-nav-btn" data-view="profile">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="8" r="5"/><path d="M20 21a8 8 0 0 0-16 0"/></svg>
          <span>My profile</span>
        </button>
        <button type="button" class="sl-nav-btn" data-view="projects">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z"/><path d="M14 2v4a2 2 0 0 0 2 2h4"/><path d="M10 9H8"/><path d="M16 13H8"/><path d="M16 17H8"/></svg>
          <span>Projects</span>
        </button>
        <button type="button" class="sl-nav-btn" data-view="library">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 7v14"/><path d="M3 18a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1h5a4 4 0 0 1 4 4 4 4 0 0 1 4-4h5a1 1 0 0 1 1 1v13a1 1 0 0 1-1 1h-6a3 3 0 0 0-3 3 3 3 0 0 0-3-3z"/></svg>
          <span>Library</span>
        </button>
        <button type="button" class="sl-nav-btn" data-view="lessons">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M9.937 15.5A2 2 0 0 0 8.5 14.063l-6.135-1.582a.5.5 0 0 1 0-.962L8.5 9.936A2 2 0 0 0 9.937 8.5l1.582-6.135a.5.5 0 0 1 .963 0L14.063 8.5A2 2 0 0 0 15.5 9.937l6.135 1.581a.5.5 0 0 1 0 .964L15.5 14.063a2 2 0 0 0-1.437 1.437l-1.582 6.135a.5.5 0 0 1-.963 0z"/></svg>
          <span>Lessons</span>
        </button>
        <button type="button" class="sl-nav-btn" data-view="apps">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M9 17H7A5 5 0 0 1 7 7h2"/><path d="M15 7h2a5 5 0 1 1 0 10h-2"/><line x1="8" x2="16" y1="12" y2="12"/></svg>
          <span>Connected apps</span>
        </button>
        <button type="button" class="sl-nav-btn" data-view="settings">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 7h-9"/><path d="M14 17H5"/><circle cx="17" cy="17" r="3"/><circle cx="7" cy="7" r="3"/></svg>
          <span>Settings</span>
        </button>
      </nav>
      <span class="sl-private">
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="16" r="1"/><rect x="3" y="10" width="18" height="12" rx="2"/><path d="M7 10V7a5 5 0 0 1 10 0v3"/></svg>
        <span id="workspace-privacy-text">Private workspace</span>
        <span id="workspace-conn-pill" class="sl-conn-pill" hidden><span class="status-dot green"></span>Connected</span>
      </span>
    </header>

    <main id="main-content" class="sl-main">
      <div class="page-heading sl-hero">
        <span class="sl-hero-streaks" aria-hidden="true"></span>
        <div>
          <h1 id="page-title">Home</h1>
          <p id="page-subtitle">Keep your background, writing guidance and project documents in one place.</p>
        </div>
        <div id="streamline-presence-slot" class="page-heading-right sl-presence-original-font">
          <div class="sl-presence">
            <button id="presence-stage-btn" type="button" class="sl-stage presence-body" data-streamline-phase="ready" aria-label="Animate Streamline" title="Click to animate Streamline">
              {presence_stage_inner}
            </button>
            <div class="sl-caption">
              <span class="brand-wordmark">streamline</span>
              <span id="presence-phase-label" role="status">Ready for your next action</span>
              <span id="presence-motion-note" class="sl-motion-note" hidden></span>
              <button id="presence-pause-btn" type="button" class="sl-pause" data-streamline-motion-control>Pause motion</button>
            </div>
          </div>
        </div>
      </div>

      <div id="banner-error" class="banner error" role="alert" hidden>
        <span id="banner-error-text"></span>
        <button id="banner-error-dismiss" type="button" class="text-button">Dismiss</button>
      </div>
      <div id="banner-notice" class="banner success" role="status" hidden>
        <span id="banner-notice-text"></span>
        <button id="banner-notice-dismiss" type="button" class="text-button" aria-label="Dismiss">Dismiss</button>
      </div>

      <section id="unlock-section" class="panel sl-unlock-panel">
        <span class="eyebrow">Private workspace</span>
        <h2>Open your private workspace</h2>
        <p>Paste your workspace key once on this browser. Streamline keeps your session ready locally so you can read your profile, compare plans, and review documents.</p>
        <form id="unlock-form" class="sl-unlock-form">
          <label>Workspace key
            <input id="workspace-cred-input" type="password" autocomplete="off" spellcheck="false" placeholder="Enter your workspace key">
          </label>
          <div class="sl-unlock-actions">
            <button id="unlock-submit-btn" type="submit" class="button primary">Open workspace</button>
          </div>
        </form>
      </section>

      <div id="app-view" hidden></div>

      <footer class="sl-footer">
        <span>Streamline</span>
        <span>
          <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="16" r="1"/><rect x="3" y="10" width="18" height="12" rx="2"/><path d="M7 10V7a5 5 0 0 1 10 0v3"/></svg>
          <span>Your documents, in your account</span>
        </span>
      </footer>
    </main>

    <dialog id="editor-modal" class="modal wide">
      <div class="modal-heading">
        <h2 id="editor-modal-title">Edit document</h2>
        <button id="editor-close" type="button" class="icon-button" aria-label="Close">X</button>
      </div>
      <form id="editor-form">
        <div class="form-two">
          <label>Shelf<select id="editor-shelf"></select></label>
          <label>Slug<input id="editor-slug" type="text" placeholder="e.g. writing-guide"></label>
        </div>
        <label>Title<input id="editor-title" type="text" required placeholder="Document title"></label>
        <div class="editor-tabs" role="tablist">
          <button id="editor-tab-edit" type="button" role="tab" aria-selected="true">Edit</button>
          <button id="editor-tab-preview" type="button" role="tab" aria-selected="false">Preview</button>
        </div>
        <div id="editor-body-wrap">
          <label>Markdown content<textarea id="editor-body" rows="12" placeholder="# Heading"></textarea></label>
        </div>
        <div id="editor-preview-box" class="editor-preview sl-document-surface" hidden></div>
        <label>Version summary<input id="editor-summary" type="text" placeholder="What changed?"></label>
        <div class="modal-actions">
          <button id="editor-cancel" type="button" class="button secondary">Cancel</button>
          <button type="submit" class="button primary">Save version</button>
        </div>
      </form>
    </dialog>

    <dialog id="create-project-modal" class="modal wide">
      <div class="modal-heading">
        <h2>New Trio+ project</h2>
        <button id="cp-close" type="button" class="icon-button" aria-label="Close">X</button>
      </div>
      <form id="create-project-form">
        <div class="form-two">
          <label>Project title<input id="cp-title" type="text" required placeholder="e.g. Release preparation"></label>
          <label>Project ID<input id="cp-id" type="text" required placeholder="release-preparation"></label>
        </div>
        <div class="form-two">
          <label>Work kind
            <select id="cp-kind">
              <option value="software">Software</option>
              <option value="council">Council</option>
              <option value="procedure">Procedure</option>
              <option value="general">General</option>
            </select>
          </label>
          <label>Difficulty (0-4)
            <select id="cp-diff">
              <option value="0">D0 - Trivial</option>
              <option value="1">D1 - Focused</option>
              <option value="2" selected>D2 - Standard</option>
              <option value="3">D3 - Complex</option>
              <option value="4">D4 - Frontier</option>
            </select>
          </label>
        </div>
        <div class="form-two">
          <label>Risk (0-4)
            <select id="cp-risk">
              <option value="0">R0 - Local</option>
              <option value="1">R1 - Low</option>
              <option value="2" selected>R2 - Moderate</option>
              <option value="3">R3 - High</option>
              <option value="4">R4 - Critical</option>
            </select>
          </label>
          <label>Initial OWN decision (optional)<input id="cp-own" type="text" placeholder="e.g. Keep public docs free of hostnames"></label>
        </div>
        <label class="checkbox-row"><input id="cp-has-ui" type="checkbox" checked>Includes user interface</label>
        <label class="checkbox-row"><input id="cp-github-release" type="checkbox">GitHub public release (requires security audit after HOW-IT-WORKS)</label>
        <details id="optional-modules-details">
          <summary>Optional planning modules</summary>
          <label class="checkbox-row"><input id="cp-mod-budget" type="checkbox" value="budget">Budget module</label>
          <label class="checkbox-row"><input id="cp-mod-schedule" type="checkbox" value="schedule">Schedule module</label>
        </details>
        <div class="modal-actions">
          <button id="cp-cancel" type="button" class="button secondary">Cancel</button>
          <button type="submit" class="button primary">Create project</button>
        </div>
      </form>
    </dialog>

    <dialog id="lesson-modal" class="modal">
      <div class="modal-heading">
        <h2>Record forward lesson</h2>
        <button id="lesson-close" type="button" class="icon-button" aria-label="Close">X</button>
      </div>
      <form id="lesson-form">
        <label>Observed fact<textarea id="ls-observed" rows="3" required placeholder="What happened?"></textarea></label>
        <label>Forward rule<textarea id="ls-forward" rows="3" required placeholder="What rule prevents this next time?"></textarea></label>
        <div class="form-two">
          <label>Cause kind<input id="ls-kind" type="text" value="process"></label>
          <label>Project ID (optional)<input id="ls-project" type="text" placeholder="e.g. v011-release"></label>
        </div>
        <label>Cause note<input id="ls-cause" type="text" placeholder="Why did it occur?"></label>
        <label>Tags (comma-separated)<input id="ls-tags" type="text" placeholder="ui, security, release"></label>
        <div class="modal-actions">
          <button id="lesson-cancel" type="button" class="button secondary">Cancel</button>
          <button type="submit" class="button primary">Save lesson</button>
        </div>
      </form>
    </dialog>

    <dialog id="confirm-modal" class="modal">
      <div class="modal-heading">
        <h2 id="confirm-modal-title">Confirm</h2>
        <button id="confirm-modal-close" type="button" class="icon-button" aria-label="Close">X</button>
      </div>
      <p id="confirm-modal-body"></p>
      <div class="modal-actions">
        <button id="confirm-modal-cancel" type="button" class="button secondary">Cancel</button>
        <button id="confirm-modal-ok" type="button" class="button primary">Confirm</button>
      </div>
    </dialog>
    <template id="sl-icon-bank">
      <svg data-icon="user" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="8" r="5"/><path d="M20 21a8 8 0 0 0-16 0"/></svg>
      <svg data-icon="book" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 7v14"/><path d="M3 18a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1h5a4 4 0 0 1 4 4 4 4 0 0 1 4-4h5a1 1 0 0 1 1 1v13a1 1 0 0 1-1 1h-6a3 3 0 0 0-3 3 3 3 0 0 0-3-3z"/></svg>
      <svg data-icon="layers" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="m12.83 2.18a2 2 0 0 0-1.66 0L2.6 6.08a1 1 0 0 0 0 1.83l8.58 3.91a2 2 0 0 0 1.66 0l8.58-3.9a1 1 0 0 0 0-1.83Z"/><path d="m22 17.65-9.17 4.16a2 2 0 0 1-1.66 0L2 17.65"/><path d="m22 12.65-9.17 4.16a2 2 0 0 1-1.66 0L2 12.65"/></svg>
      <svg data-icon="file" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z"/><path d="M14 2v4a2 2 0 0 0 2 2h4"/><path d="M10 9H8"/><path d="M16 13H8"/><path d="M16 17H8"/></svg>
      <svg data-icon="link" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M9 17H7A5 5 0 0 1 7 7h2"/><path d="M15 7h2a5 5 0 1 1 0 10h-2"/><line x1="8" x2="16" y1="12" y2="12"/></svg>
      <svg data-icon="arrow-right" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M5 12h14"/><path d="m12 5 7 7-7 7"/></svg>
      <svg data-icon="check" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 6 9 17l-5-5"/></svg>
      <svg data-icon="history" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8"/><path d="M3 3v5h5"/><path d="M12 7v5l4 2"/></svg>
      <svg data-icon="lock" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="16" r="1"/><rect x="3" y="10" width="18" height="12" rx="2"/><path d="M7 10V7a5 5 0 0 1 10 0v3"/></svg>
      <svg data-icon="shield" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z"/><path d="m9 12 2 2 4-4"/></svg>
      <svg data-icon="copy" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect width="14" height="14" x="8" y="8" rx="2" ry="2"/><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/></svg>
      <svg data-icon="download" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" x2="12" y1="15" y2="3"/></svg>
    </template>
  </div>

  <script>{ui_js}</script>
  {presence_script_tag}
</body>
</html>
"##,
        ui_css = UI_CSS,
        ui_js = UI_JS,
        presence_stage_inner = presence_stage_inner,
        presence_script_tag = presence_script_tag,
    );

    Html(html)
}
