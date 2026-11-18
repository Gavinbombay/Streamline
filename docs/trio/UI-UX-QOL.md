# Streamline UI, UX, and Quality-of-Life Specification

Version: `0.1.0-d1`

## 1. Preserved Visual Identity and Tokens (`OWN-005`)

Streamline's established dark palette and readable typography are locked:

```css
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
}
```

### Accessibility and Readability Floor
- **Body copy**: `16px / 1.6`, crisp contrast (`#edf5ff` primary, `#b4c5e0` secondary; no reduced-opacity grey-on-navy text).
- **Metadata floor**: `14px` minimum across stamps, badges, and breadcrumbs (`16px` for `Last used` and footer lines; zero `9px` to `13px` micro-text).
- **Interactive targets**: `44px` minimum height/width (`48px` sidebar nav items), `3px solid #4ccfff` `:focus-visible` outline at `4px` offset, keyboard skip-link to `#main-content`.
- **Reduced motion**: Respects `prefers-reduced-motion: reduce` and includes an explicit pause toggle on header animations.

## 2. Pluggable Presence Boundary (`OWN-006`)

- **Open-Source Default**: Renders a clean, accessible SVG status mark (`docs/assets/streamline-mark.png` when supplied, with a crisp built-in vector mark fallback) reflecting real local server state (`Idle`, `Active Request`, `Conveyor Running`, `Action Needed`) with zero simulated cognition or fake audio activity.
- **Private Local Plugin Hook**: When `presence_plugin_dir` is set in `StreamlineConfig` on an operator's private host (such as `/srv/streamline/private-presence`), `/_streamline/presence-plugin/*` serves the operator's private `presence.js` canvas bundle into the same mount container (`<div id="streamline-presence-slot">`), keeping the original Presence renderer 100% private and out of the public git repository.

## 3. Unified 3-Pane Workspace Layout

Combines Streamline's navigation with the 3-region reader shell:

1. **Left Sidebar (`264px` fixed)**:
   - Brand lockup + Presence/status slot
   - Active Workspace indicator (`Rule Gate` status + Vault sync health)
   - Primary destinations: **Home**, **Projects (Trio+)**, **Shelves (Governance)**, **My Profile & Skills**, **Conveyor & Inbox (`DOC-PENDING-OWNER`)**, **Connected Apps** (with a single **Technical details** destination for diagnostics).
2. **Center Main Region (`max-width: 1480px`)**:
   - **Projects view**: Scalable Trio+ creation (`P0` to `P4`, optional tracking collapsed by default, budget never auto-checked), **Lock plan version** button right beside **Compare**, expanded changed-word diff highlights, and `D I T A L` progress pills.
   - **Shelves view**: Filterable list by shelf/tag with freshness badges (`Verified`, `Stale`, `Drift`, `Unknown`).
   - **Owner Approval Inbox**: Dedicated queue for runs in `DOC_PENDING_OWNER`, displaying `HOW-IT-WORKS.md` in plain English alongside the cross-model `check_sync` verdict and a single **Approve Release** / **Request Fix** control.
3. **Right Docked Panel (collapsible under `1100px`)**:
   - Switches cleanly via `?panel=read|history|related|passport`:
     - `read`: Clean rendered Markdown reader
     - `history`: Revision timeline with diff and one-click restore
     - `related`: Linked requirements, `DIV`s, and sync reports
     - `passport`: Source Passport (steward, last verified timestamp, lineage, required tag, next action)

## 4. Empty, Error, and Uncertain States (`L7`)

- **Empty shelf or project list**: Clear plain-English explanation plus a single primary button (`New Trio+ Project` or `Import Markdown Vault`), never blank table headers over zero rows.
- **Unverified / Unknown state**: Displays `UNKNOWN` badge explicitly; never fabricates green checkmarks, fake dollar figures, or synthetic sync badges.
- **Redaction blocked (`422`)**: Highlights the offending field/line number without echoing the sensitive value back into logs or toast history.
- **Conveyor Impasse**: Displays `ESCALATION.md` summary, exact 2-strike or missing-input reason, and a one-step owner decision prompt.
