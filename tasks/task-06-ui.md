# Task 06 — Web UI Rewrite

## Status: ✅ Done

## Objective
Rewrite `application/src/index.html` as a complete single-page PDU admin interface.

## Requirements

### Authentication
- Login modal shown on page load if no credentials in sessionStorage
- Credentials stored as base64 in sessionStorage (cleared on tab close)
- All fetch() calls include `Authorization: Basic <b64>` header
- On 401 response: clear credentials, re-show login modal
- First-login modal: password change dialog, shown when `status.first_login == true`
  - Cannot be dismissed without changing password

### Layout (single page, no frameworks)
```
┌─ PDU Controller ─────────────────────── IP: x.x.x.x │ v1.0.0 ─┐
├─ Relays ──────────────────────────────────────────────────────────┤
│  2×4 grid of relay cards (ports 0–7):                            │
│  ┌──────────┐  Port name (editable by admin)                     │
│  │  [ON]    │  Toggle button (green=ON, grey=OFF)                │
│  │  0.0 A   │  Current stub                                      │
│  └──────────┘  Disabled if user lacks ACL for this port          │
├─ Sensors ─────────────────────────────────────────────────────────┤
│  🌡 23.5°C        ⚡ 0.0 V   (auto-refreshed every 10s)          │
├─ Admin Panel ▾ (collapsed accordion, admin-only) ─────────────────┤
│  · Change password                                                │
│  · Rename ports (inline)                                         │
│  · User management (table: name, admin checkbox, port checkboxes)│
│  · Firmware upload (file picker + XHR progress bar)              │
│  · Factory reset (confirmation dialog)                           │
└───────────────────────────────────────────────────────────────────┘
```

### Functionality
- On load: fetch `/api/status` (detect first_login), then fetch all 8 GPIO states
- Sensor refresh every 10 seconds via `/api/sensors`
- Toggle button click: `POST /api/gpio/:pin/toggle`, update button state from response
- Port names fetched from `/api/port/:n/name` on load
- Admin panel: only rendered/fetched if `AuthUser.is_admin`
- OTA upload: XHR with `upload.onprogress` for progress bar display

### Technology
- Vanilla HTML5/CSS3/JS — no external CDN dependencies
- Inline CSS (no separate stylesheet file)
- No build step, single `<script>` block
- Responsive: works on mobile (for emergency remote toggling)
- Dark-mode friendly (CSS variables)

## Checklist
- [x] Login modal
- [x] First-login password change modal
- [x] Status bar (IP, version)
- [x] 8 relay cards in 2×4 grid
- [x] Sensor bar (temperature, voltage)
- [x] Admin panel accordion
  - [x] Password change form
  - [x] Port rename fields
  - [x] User management table
  - [x] OTA upload with progress
  - [x] Factory reset with confirmation
- [x] sessionStorage credential handling
- [x] 401 handler → re-login
- [x] Auto-refresh sensors

## Log

`application/src/index.html` rewritten as a complete single-page app. All checklist items implemented:
- Login modal with sessionStorage credential caching + 401 auto-re-login
- First-login password change modal (undismissable)
- Status bar showing version
- 8 relay cards in responsive grid (ON=green, OFF=grey, disabled if ACL denies port)
- Sensor bar with temperature + voltage, auto-refreshed every 10 s
- Admin accordion (collapsed by default):
  - Password change form (updates sessionStorage creds on success)
  - Port rename table (inline text fields, saved per-port via POST)
  - New user creation with per-port checkboxes and admin checkbox
  - OTA firmware upload via XHR with progress bar
  - Factory reset with `confirm()` dialog
- Vanilla JS, no external deps, dark-mode CSS variables, mobile-responsive
