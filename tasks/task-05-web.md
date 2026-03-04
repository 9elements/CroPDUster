# Task 05 — picoserve Web Layer

## Status: ⏳ Pending

## Objective
Implement the picoserve router, AppState, all HTTP handlers, and the web task.

## Files to Create

### `application/src/web/mod.rs`
- `AppState` struct
- `AppWithStateBuilder` impl → builds full router
- `web_task` (pool_size = 4)
- `WEB_TASK_POOL_SIZE = 4`
- `CONFIG: picoserve::Config` static

### `application/src/web/status.rs`
`GET /api/status` → `{"version":"1.0.0","first_login":bool}`

### `application/src/web/gpio.rs`
- `GET /api/gpio/:pin` → `{"pin":u8,"state":bool,"name":"...","allowed":bool}`
- `POST /api/gpio/:pin/toggle` → `{"pin":u8,"state":bool}` (ACL check)
- `POST /api/gpio/:pin/set` body: `{"state":bool}` → `{"pin":u8,"state":bool}` (ACL check)

### `application/src/web/sensors.rs`
`GET /api/sensors` → `{"temperature_c":f32,"voltage_v":f32,"current_a":[f32;8]}`

### `application/src/web/admin.rs`
- `POST /api/port/:n/name` body: `{"name":"..."}` → sets port name (admin only)
- `GET /api/port/:n/name` → `{"name":"..."}`
- `POST /api/admin/password` body: `{"old":"...","new":"..."}` → change password
- `POST /api/admin/users` body: `{"username":"...","password":"...","ports":u8,"admin":bool}`
- `POST /api/admin/users/:user/ports` body: `{"ports":u8}`
- `DELETE /api/admin/users/:user`
- `POST /api/admin/reset` → format ekv + seed defaults + SCB::sys_reset()

### `application/src/web/update.rs`
`POST /api/update` — streaming OTA (admin only):
1. Extract `AuthUser` (admin check)
2. Get `content_length` from request
3. `db.lock_flash()` — exclusive flash access
4. Create `FirmwareUpdater` from linker symbols
5. `body.reader().with_different_timeout(60s)` — override short request timeout
6. Stream body → write to DFU in ERASE_SIZE chunks
7. `mark_updated()` → `SCB::sys_reset()`

## Router Structure
```rust
Router::new()
  .route("/",                              get(serve_index))
  .route("/api/status",                    get(handle_status))
  .route("/api/sensors",                   get(handle_sensors))
  .route("/api/gpio/:pin",                 get(handle_gpio_get))
  .route("/api/gpio/:pin/toggle",          post(handle_gpio_toggle))
  .route("/api/gpio/:pin/set",             post(handle_gpio_set))
  .route("/api/port/:n/name",              get(handle_port_name_get).post(handle_port_name_set))
  .route("/api/admin/password",            post(handle_password_change))
  .route("/api/admin/users",               post(handle_create_user))
  .route("/api/admin/users/:user/ports",   post(handle_set_user_ports))
  .route("/api/admin/users/:user",         delete(handle_delete_user))
  .route("/api/admin/reset",               post(handle_factory_reset))
  .route("/api/update",                    post(handle_firmware_update))
```

## JSON Serialization
Use picoserve's built-in JSON support (`json` feature) via `serde_json_core` for
response serialization. For request bodies, use `serde_json_core::from_slice`.

## Checklist
- [ ] Create `application/src/web/mod.rs`
- [ ] Create `application/src/web/status.rs`
- [ ] Create `application/src/web/gpio.rs`
- [ ] Create `application/src/web/sensors.rs`
- [ ] Create `application/src/web/admin.rs`
- [ ] Create `application/src/web/update.rs`

## Log
<!-- Agent fills this in -->
