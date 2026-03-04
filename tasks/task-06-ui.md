# Task 06 — Web UI Rewrite

## Status: ⏳ Pending

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
- [ ] Login modal
- [ ] First-login password change modal
- [ ] Status bar (IP, version)
- [ ] 8 relay cards in 2×4 grid
- [ ] Sensor bar (temperature, voltage)
- [ ] Admin panel accordion
  - [ ] Password change form
  - [ ] Port rename fields
  - [ ] User management table
  - [ ] OTA upload with progress
  - [ ] Factory reset with confirmation
- [ ] sessionStorage credential handling
- [ ] 401 handler → re-login
- [ ] Auto-refresh sensors

## Log
<!-- Agent fills this in -->
