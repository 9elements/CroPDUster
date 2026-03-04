# Task 01 — Flash Layout Changes

## Status: ✅ Done

## Objective
Update both `bootloader/memory.x` and `application/memory.x` to reflect the new
flash layout that shrinks ACTIVE and DFU partitions to make room for the ekv CONFIG
region.

## Changes Required

### `bootloader/memory.x`
- ACTIVE: `0x10007000`, `256K` (was `512K`)
- DFU:    `0x10047000`, `256K` (was `0x10087000`, `516K`)
- CONFIG: `0x10087000`, `256K` (new region)
- Add linker symbols: `__bootloader_config_start`, `__bootloader_config_end`

### `application/memory.x`
- FLASH (active): `0x10007000`, `256K` (was `512K`)
- DFU:   `0x10047000`, `256K` (was `0x10087000`, `516K`)
- CONFIG: `0x10087000`, `256K` (new region)
- Add linker symbols for CONFIG region

## Checklist
- [x] Update `bootloader/memory.x`
- [x] Update `application/memory.x`
- [x] Verify address arithmetic: 0x10007000 + 256K = 0x10047000 ✓
- [x] Verify address arithmetic: 0x10047000 + 256K = 0x10087000 ✓
- [x] Verify address arithmetic: 0x10087000 + 256K = 0x100C7000, fits in 2MB flash (0x10200000) ✓

## Notes
- `scripts/combine_binaries.py` reads ELF PT_LOAD segments — no changes needed there
- `DFU_START` and `DFU_SIZE` constants in `application/src/main.rs` will be updated in Task 4
- ekv page size: 4096 bytes → 64 pages in 256 KB CONFIG region

## Log

### `bootloader/memory.x`
- ACTIVE: `LENGTH = 512K` → `LENGTH = 256K` (origin unchanged at `0x10007000`)
- DFU: `ORIGIN = 0x10087000, LENGTH = 516K` → `ORIGIN = 0x10047000, LENGTH = 256K`
- CONFIG: new region added — `ORIGIN = 0x10087000, LENGTH = 256K`
- Added linker symbols `__bootloader_config_start` and `__bootloader_config_end`

### `application/memory.x`
- FLASH: `LENGTH = 512K` → `LENGTH = 256K` (origin unchanged at `0x10007000`)
- DFU: `ORIGIN = 0x10087000, LENGTH = 516K` → `ORIGIN = 0x10047000, LENGTH = 256K`
- CONFIG: new region added — `ORIGIN = 0x10087000, LENGTH = 256K`
- Added linker symbols `__bootloader_config_start` and `__bootloader_config_end`
