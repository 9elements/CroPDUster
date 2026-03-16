use anyhow::{bail, Context, Result};
use base64::Engine;
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use xshell::{cmd, Shell};

// ── CLI definition ────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "xtask", about = "Build and flash helpers for the PDU project")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Build bootloader, application, or both (default: both)
    Build {
        /// Build only the bootloader
        #[arg(long)]
        bootloader: bool,
        /// Build only the application
        #[arg(long)]
        application: bool,
    },
    /// Combine bootloader + application ELFs into build/combined.uf2
    Combine,
    /// Build everything and produce the combined UF2 (build + combine)
    Dist,
    /// Flash firmware to the device
    Flash {
        /// Flash only the bootloader
        #[arg(long)]
        bootloader: bool,
        /// Flash only the application
        #[arg(long)]
        application: bool,
        /// Use probe-rs instead of UF2 drag-and-drop (attaches RTT for live logging)
        #[arg(long)]
        probe: bool,
        /// Build the application with the 'debug' feature (panic-probe + RTT logging).
        /// Automatically implied by --probe.
        #[arg(long)]
        debug: bool,
        /// OTA upload to a running device at the given IP address
        #[arg(long, value_name = "IP")]
        ota: Option<String>,
    },
    /// Verify that all required tools are installed
    CheckTools,
    /// Remove all build artifacts
    Clean,
}

// ── Paths ─────────────────────────────────────────────────────────────────────

fn workspace_root() -> PathBuf {
    // __file__ is xtask/src/main.rs → go up two levels
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask has a parent workspace directory")
        .to_path_buf()
}

fn build_dir(root: &Path) -> PathBuf {
    root.join("build")
}

fn bootloader_elf(root: &Path) -> PathBuf {
    root.join("build/bootloader.elf")
}

fn application_elf(root: &Path) -> PathBuf {
    root.join("build/application.elf")
}

fn application_debug_elf(root: &Path) -> PathBuf {
    root.join("build/application-debug.elf")
}

fn bootloader_uf2(root: &Path) -> PathBuf {
    root.join("build/bootloader.uf2")
}

fn application_uf2(root: &Path) -> PathBuf {
    root.join("build/application.uf2")
}

fn combined_uf2(root: &Path) -> PathBuf {
    root.join("build/combined.uf2")
}

// ── Build ─────────────────────────────────────────────────────────────────────

fn build_bootloader(sh: &Shell, root: &Path) -> Result<()> {
    eprintln!("→ Building bootloader…");
    let build_dir = build_dir(root);
    std::fs::create_dir_all(&build_dir)?;

    let _dir = sh.push_dir(root.join("bootloader"));
    cmd!(sh, "cargo build --release").run()?;

    let src = root.join("target/thumbv6m-none-eabi/release/pdu-rp-bootloader");
    let dst = bootloader_elf(root);
    std::fs::copy(&src, &dst)
        .with_context(|| format!("copy {} → {}", src.display(), dst.display()))?;

    let uf2 = bootloader_uf2(root);
    cmd!(sh, "elf2uf2-rs {dst} {uf2}").run()?;
    eprintln!("  ✓ {}", uf2.display());
    Ok(())
}

fn build_application(sh: &Shell, root: &Path) -> Result<()> {
    eprintln!("→ Building application…");
    let build_dir = build_dir(root);
    std::fs::create_dir_all(&build_dir)?;

    let _dir = sh.push_dir(root.join("application"));
    cmd!(sh, "cargo build --release").run()?;

    let src = root.join("target/thumbv6m-none-eabi/release/pdu-rp-application");
    let dst = application_elf(root);
    std::fs::copy(&src, &dst)
        .with_context(|| format!("copy {} → {}", src.display(), dst.display()))?;

    let uf2 = application_uf2(root);

    cmd!(sh, "elf2uf2-rs {dst} {uf2}").run()?;
    eprintln!("  ✓ {}", uf2.display());
    Ok(())
}

/// Build the application with the `debug` feature (panic-probe + full defmt RTT logging).
/// The resulting ELF is stored separately as `build/application-debug.elf` so it
/// doesn't overwrite the production UF2 artefacts.
fn build_application_debug(sh: &Shell, root: &Path) -> Result<()> {
    eprintln!("→ Building application (debug / probe-rs)…");
    std::fs::create_dir_all(build_dir(root))?;

    let _dir = sh.push_dir(root.join("application"));
    cmd!(
        sh,
        "cargo build --release --no-default-features --features debug"
    )
    .run()?;

    let src = root.join("target/thumbv6m-none-eabi/release/pdu-rp-application");
    let dst = application_debug_elf(root);
    std::fs::copy(&src, &dst)
        .with_context(|| format!("copy {} → {}", src.display(), dst.display()))?;

    eprintln!("  ✓ {}", dst.display());
    Ok(())
}

// ── ELF parsing ──────────────────────────────────────────────────────────────

/// Extract PT_LOAD segments from a 32-bit little-endian ELF and produce a flat
/// binary image.  Only segments with `p_paddr >= base_addr` and non-zero
/// `p_filesz` are included.  Gaps are filled with `0xFF`.
fn elf_to_binary(elf_path: &Path, base_addr: u32) -> Result<Vec<u8>> {
    let elf =
        std::fs::read(elf_path).with_context(|| format!("reading ELF {}", elf_path.display()))?;

    if elf.len() < 0x34 || &elf[..4] != b"\x7fELF" {
        bail!("not a valid ELF file: {}", elf_path.display());
    }

    let e_phoff = u32::from_le_bytes(elf[0x1c..0x20].try_into().unwrap()) as usize;
    let e_phnum = u16::from_le_bytes(elf[0x2c..0x2e].try_into().unwrap()) as usize;

    struct Segment {
        offset: usize,
        data: Vec<u8>,
    }

    let mut segments: Vec<Segment> = Vec::new();
    let mut max_extent: usize = 0;

    for i in 0..e_phnum {
        let ph = e_phoff + i * 32;
        if ph + 32 > elf.len() {
            break;
        }
        let p_type = u32::from_le_bytes(elf[ph..ph + 4].try_into().unwrap());
        if p_type != 1 {
            continue; // skip non-PT_LOAD
        }
        let p_offset = u32::from_le_bytes(elf[ph + 4..ph + 8].try_into().unwrap()) as usize;
        let p_paddr = u32::from_le_bytes(elf[ph + 12..ph + 16].try_into().unwrap());
        let p_filesz = u32::from_le_bytes(elf[ph + 16..ph + 20].try_into().unwrap()) as usize;

        if p_paddr < base_addr || p_filesz == 0 {
            continue;
        }

        let off = (p_paddr - base_addr) as usize;
        let end = p_offset + p_filesz;
        if end > elf.len() {
            bail!(
                "ELF segment at paddr {:#010x} extends past end of file",
                p_paddr
            );
        }

        segments.push(Segment {
            offset: off,
            data: elf[p_offset..end].to_vec(),
        });
        max_extent = max_extent.max(off + p_filesz);
    }

    if segments.is_empty() {
        bail!(
            "no loadable segments with paddr >= {:#010x} in {}",
            base_addr,
            elf_path.display()
        );
    }

    let mut bin = vec![0xFFu8; max_extent];
    for seg in &segments {
        bin[seg.offset..seg.offset + seg.data.len()].copy_from_slice(&seg.data);
    }
    Ok(bin)
}

// ── UF2 generation ───────────────────────────────────────────────────────────

const UF2_MAGIC0: u32 = 0x0A324655;
const UF2_MAGIC1: u32 = 0x9E5D5157;
const UF2_MAGIC_END: u32 = 0x0AB16F30;
const UF2_FLAG_FAMILY: u32 = 0x00002000;
const RP2040_FAMILY_ID: u32 = 0xe48bff56;
const UF2_PAYLOAD_SIZE: usize = 256;

/// Convert a flat binary image to UF2 format for RP2040.
fn binary_to_uf2(data: &[u8], base_addr: u32) -> Vec<u8> {
    let num_blocks = data.len().div_ceil(UF2_PAYLOAD_SIZE);
    let mut uf2 = Vec::with_capacity(num_blocks * 512);

    for (i, chunk) in data.chunks(UF2_PAYLOAD_SIZE).enumerate() {
        let mut block = [0u8; 512];
        let addr = base_addr + (i * UF2_PAYLOAD_SIZE) as u32;

        // Header
        block[0..4].copy_from_slice(&UF2_MAGIC0.to_le_bytes());
        block[4..8].copy_from_slice(&UF2_MAGIC1.to_le_bytes());
        block[8..12].copy_from_slice(&UF2_FLAG_FAMILY.to_le_bytes());
        block[12..16].copy_from_slice(&addr.to_le_bytes());
        block[16..20].copy_from_slice(&(UF2_PAYLOAD_SIZE as u32).to_le_bytes());
        block[20..24].copy_from_slice(&(i as u32).to_le_bytes());
        block[24..28].copy_from_slice(&(num_blocks as u32).to_le_bytes());
        block[28..32].copy_from_slice(&RP2040_FAMILY_ID.to_le_bytes());

        // Payload (256 bytes, zero-padded for short final chunk)
        block[32..32 + chunk.len()].copy_from_slice(chunk);

        // Final magic
        block[508..512].copy_from_slice(&UF2_MAGIC_END.to_le_bytes());

        uf2.extend_from_slice(&block);
    }
    uf2
}

// ── Combine ──────────────────────────────────────────────────────────────────

/// Application offset within the combined binary (matches ACTIVE partition
/// start relative to flash base: 0x10007000 - 0x10000000 = 0x7000).
const APP_FLASH_OFFSET: usize = 0x7000;
const FLASH_BASE: u32 = 0x10000000;

fn combine(_sh: &Shell, root: &Path) -> Result<()> {
    eprintln!("→ Combining bootloader + application…");

    let bl_elf = bootloader_elf(root);
    let app_elf = application_elf(root);

    // Parse ELF files into flat binaries relative to flash base
    let bl_bin = elf_to_binary(&bl_elf, FLASH_BASE).with_context(|| "parsing bootloader ELF")?;
    let app_bin = elf_to_binary(&app_elf, FLASH_BASE).with_context(|| "parsing application ELF")?;

    // Application binary starts at flash offset 0x7000.
    // Verify the bootloader doesn't extend into the application region.
    if bl_bin.len() > APP_FLASH_OFFSET {
        bail!(
            "bootloader binary ({} bytes) exceeds application offset ({:#x})",
            bl_bin.len(),
            APP_FLASH_OFFSET
        );
    }

    // Build combined flat binary
    let combined_size = APP_FLASH_OFFSET + app_bin.len();
    let mut combined = vec![0xFFu8; combined_size];
    combined[..bl_bin.len()].copy_from_slice(&bl_bin);
    combined[APP_FLASH_OFFSET..].copy_from_slice(&app_bin);

    eprintln!("  Bootloader: {} bytes at offset 0x0", bl_bin.len());
    eprintln!(
        "  Application: {} bytes at offset {:#x}",
        app_bin.len(),
        APP_FLASH_OFFSET
    );
    eprintln!("  Combined: {} bytes total", combined_size);

    // Convert to UF2 and write
    let uf2_data = binary_to_uf2(&combined, FLASH_BASE);
    let uf2_path = combined_uf2(root);
    std::fs::write(&uf2_path, &uf2_data)
        .with_context(|| format!("writing {}", uf2_path.display()))?;

    eprintln!(
        "  ✓ {} ({} UF2 blocks, {:.1} KiB)",
        uf2_path.display(),
        uf2_data.len() / 512,
        uf2_data.len() as f64 / 1024.0
    );
    Ok(())
}

// ── Flash ─────────────────────────────────────────────────────────────────────

/// Wait for the RPI-RP2 mass-storage device to appear (Linux: /dev/disk/by-label/RPI-RP2).
/// Returns the mount point once the device is mounted.
#[cfg(target_os = "linux")]
fn wait_for_rpi_rp2(timeout_secs: u64) -> Result<PathBuf> {
    use std::time::{Duration, Instant};

    let label_path = Path::new("/dev/disk/by-label/RPI-RP2");
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);

    eprintln!("  Waiting for RPI-RP2 USB drive (hold BOOTSEL and connect)…");

    while Instant::now() < deadline {
        if label_path.exists() {
            // Try to find the mount point via /proc/mounts
            let mounts = std::fs::read_to_string("/proc/mounts").unwrap_or_default();
            let real = std::fs::canonicalize(label_path).unwrap_or_default();
            let real_str = real.to_string_lossy();
            for line in mounts.lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 && parts[0] == real_str.as_ref() {
                    return Ok(PathBuf::from(parts[1]));
                }
            }
            // Device exists but isn't mounted yet — try to mount it
            let tmp = tempfile_mountpoint()?;
            let sh = Shell::new()?;
            let real = real.to_string_lossy().to_string();
            let mount_pt = tmp.to_string_lossy().to_string();
            if cmd!(sh, "mount {real} {mount_pt}").run().is_ok() {
                return Ok(tmp);
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    bail!(
        "Timed out waiting for RPI-RP2 drive after {}s",
        timeout_secs
    )
}

#[cfg(target_os = "linux")]
fn tempfile_mountpoint() -> Result<PathBuf> {
    let dir = std::env::temp_dir().join("rpi-rp2-mount");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

#[cfg(target_os = "macos")]
fn wait_for_rpi_rp2(timeout_secs: u64) -> Result<PathBuf> {
    use std::time::{Duration, Instant};

    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    let volume = Path::new("/Volumes/RPI-RP2");
    eprintln!("  Waiting for RPI-RP2 volume (hold BOOTSEL and connect)…");

    while Instant::now() < deadline {
        if volume.exists() {
            return Ok(volume.to_path_buf());
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    bail!(
        "Timed out waiting for RPI-RP2 volume after {}s",
        timeout_secs
    )
}

#[cfg(windows)]
fn wait_for_rpi_rp2(_timeout_secs: u64) -> Result<PathBuf> {
    // On Windows, look for a drive labelled RPI-RP2
    bail!(
        "UF2 auto-copy not supported on Windows. Copy the UF2 file manually to the RPI-RP2 drive."
    )
}

fn flash_uf2(uf2_path: &Path) -> Result<()> {
    let mount = wait_for_rpi_rp2(60)?;
    let filename = uf2_path.file_name().context("UF2 path has no filename")?;
    let dest = mount.join(filename);
    eprintln!("  Copying {} → {}", uf2_path.display(), dest.display());
    std::fs::copy(uf2_path, &dest)?;
    eprintln!("  ✓ Done — device will reboot");
    Ok(())
}

/// Flash an ELF to the device via probe-rs (download only, no reset).
fn flash_only_probe(sh: &Shell, elf_path: &Path) -> Result<()> {
    cmd!(sh, "probe-rs download --chip RP2040 {elf_path}").run()?;
    Ok(())
}

/// Reset the chip via probe-rs.
fn reset_probe(sh: &Shell) -> Result<()> {
    cmd!(sh, "probe-rs reset --chip RP2040").run()?;
    Ok(())
}

/// Attach RTT to a running chip using the given ELF for memory-map info.
/// Runs until the user presses Ctrl+C.
fn attach_rtt_probe(sh: &Shell, app_elf: &Path) -> Result<()> {
    cmd!(sh, "probe-rs attach --chip RP2040 {app_elf}").run()?;
    Ok(())
}

/// Parse a UF2 file and return a flat raw binary containing only the blocks
/// whose target address falls within the ACTIVE partition (`>= ACTIVE_START`).
/// Gaps between blocks are filled with `0xFF`.
fn uf2_to_active_binary(uf2_data: &[u8]) -> Result<Vec<u8>> {
    const ACTIVE_START: u32 = 0x10007000;
    const MAGIC0: u32 = 0x0A324655;
    const MAGIC1: u32 = 0x9E5D5157;
    const BLOCK_SIZE: usize = 512;
    const PAYLOAD_OFFSET: usize = 32;
    const PAYLOAD_SIZE: usize = 256;

    let mut blocks: Vec<(u32, [u8; PAYLOAD_SIZE])> = Vec::new();

    for chunk in uf2_data.chunks(BLOCK_SIZE) {
        if chunk.len() < BLOCK_SIZE {
            break;
        }
        let m0 = u32::from_le_bytes(chunk[0..4].try_into().unwrap());
        let m1 = u32::from_le_bytes(chunk[4..8].try_into().unwrap());
        if m0 != MAGIC0 || m1 != MAGIC1 {
            continue;
        }
        let addr = u32::from_le_bytes(chunk[12..16].try_into().unwrap());
        if addr < ACTIVE_START {
            continue; // skip BOOT2 and bootloader blocks
        }
        let mut payload = [0u8; PAYLOAD_SIZE];
        payload.copy_from_slice(&chunk[PAYLOAD_OFFSET..PAYLOAD_OFFSET + PAYLOAD_SIZE]);
        blocks.push((addr, payload));
    }

    if blocks.is_empty() {
        bail!(
            "No ACTIVE region blocks found in UF2 (expected addresses >= {:#010x})",
            ACTIVE_START
        );
    }
    blocks.sort_by_key(|(addr, _)| *addr);

    let first_addr = blocks[0].0;
    let last_addr = blocks.last().unwrap().0;
    let bin_size = (last_addr - first_addr) as usize + PAYLOAD_SIZE;
    let mut bin = vec![0xFFu8; bin_size];
    for (addr, payload) in &blocks {
        let off = (addr - first_addr) as usize;
        bin[off..off + PAYLOAD_SIZE].copy_from_slice(payload);
    }

    eprintln!(
        "  UF2: {} ACTIVE blocks → {:.1} KiB raw binary (base {:#010x})",
        blocks.len(),
        bin_size as f64 / 1024.0,
        first_addr
    );
    Ok(bin)
}

fn flash_ota(uf2_path: &Path, ip: &str) -> Result<()> {
    // Read credentials from .pdu-credentials (format: "user:pass") or use admin:admin
    let creds =
        std::fs::read_to_string(".pdu-credentials").unwrap_or_else(|_| "admin:admin".to_string());
    let creds = creds.trim().to_string();

    let b64 = base64::engine::general_purpose::STANDARD.encode(creds.as_bytes());
    let auth = format!("Basic {}", b64);

    let url = format!("http://{}/api/update", ip);
    eprintln!("  OTA uploading {} to {}…", uf2_path.display(), url);

    let uf2_data =
        std::fs::read(uf2_path).with_context(|| format!("reading {}", uf2_path.display()))?;

    let data = uf2_to_active_binary(&uf2_data)
        .with_context(|| format!("parsing UF2 {}", uf2_path.display()))?;

    let result = ureq::post(&url)
        .set("Authorization", &auth)
        .set("Content-Type", "application/octet-stream")
        .send_bytes(&data);

    match result {
        Ok(response) => {
            let status = response.status();
            if status == 200 {
                eprintln!("  OTA upload complete — device is rebooting");
                Ok(())
            } else {
                let body = response.into_string().unwrap_or_default();
                bail!("OTA failed: HTTP {} — {}", status, body)
            }
        }
        Err(ureq::Error::Transport(t))
            if t.kind() == ureq::ErrorKind::ConnectionFailed
                || t.message()
                    .map(|m| m.contains("Connection reset") || m.contains("os error 104"))
                    .unwrap_or(false) =>
        {
            // Device reset the TCP connection immediately after accepting the
            // firmware write — this is expected when sys_reset() fires before
            // the HTTP response is fully flushed.
            eprintln!("  OTA upload complete — device reset connection (reboot triggered)");
            Ok(())
        }
        Err(e) => bail!("OTA request failed: {}", e),
    }
}

// ── Tool checks ───────────────────────────────────────────────────────────────

fn check_tool(name: &str, install_hint: &str) -> bool {
    if which::which(name).is_ok() {
        eprintln!("  ✓ {}", name);
        true
    } else {
        eprintln!("  ✗ {} — not found", name);
        eprintln!("    Install: {}", install_hint);
        false
    }
}

// ── Main ──────────────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    let cli = Cli::parse();
    let root = workspace_root();
    let sh = Shell::new()?;

    match cli.command {
        Command::Build {
            bootloader,
            application,
        } => {
            let both = !bootloader && !application;
            if both || bootloader {
                build_bootloader(&sh, &root)?;
            }
            if both || application {
                build_application(&sh, &root)?;
            }
        }

        Command::Combine => {
            combine(&sh, &root)?;
        }

        Command::Dist => {
            build_bootloader(&sh, &root)?;
            build_application(&sh, &root)?;
            combine(&sh, &root)?;
        }

        Command::Flash {
            bootloader,
            application,
            probe,
            debug,
            ota,
        } => {
            let flash_combined = !bootloader && !application;
            // --probe implies --debug (build with panic-probe + full RTT)
            let use_debug_build = debug || probe;

            if let Some(ip) = &ota {
                // OTA mode: always flashes the application UF2
                let uf2 = application_uf2(&root);
                if !uf2.exists() {
                    eprintln!("Application UF2 not found — building first…");
                    build_application(&sh, &root)?;
                }
                flash_ota(&uf2, ip)?;
            } else if probe {
                // probe-rs path: bootloader via download-only, application via run (RTT)
                // Always use the debug build for the application so RTT logging works.
                let get_app_elf = |root: &Path| {
                    if use_debug_build {
                        application_debug_elf(root)
                    } else {
                        application_elf(root)
                    }
                };
                let build_app = |sh: &Shell, root: &Path| {
                    if use_debug_build {
                        build_application_debug(sh, root)
                    } else {
                        build_application(sh, root)
                    }
                };

                // --probe always rebuilds (development workflow: latest code on device).
                if flash_combined {
                    // IMPORTANT: application must be downloaded BEFORE bootloader.
                    //
                    // The application ELF contains a BOOT2 section at 0x10000000.
                    // Flash sector 0 (0x10000000–0x10000FFF) is shared: BOOT2 occupies
                    // the first 256 bytes and the bootloader vector table starts at
                    // 0x10000100. probe-rs erases the entire sector to write BOOT2,
                    // destroying the bootloader vector table in the process.
                    //
                    // Downloading the bootloader second restores sector 0 correctly.
                    build_app(&sh, &root)?;
                    let app_elf = get_app_elf(&root);
                    eprintln!("→ Downloading application via probe-rs…");
                    flash_only_probe(&sh, &app_elf)?;

                    build_bootloader(&sh, &root)?;
                    let bl_elf = bootloader_elf(&root);
                    eprintln!("→ Downloading bootloader via probe-rs (restores sector 0)…");
                    flash_only_probe(&sh, &bl_elf)?;

                    eprintln!("→ Resetting chip…");
                    reset_probe(&sh)?;

                    eprintln!("→ Attaching RTT (Ctrl+C to exit)…");
                    attach_rtt_probe(&sh, &app_elf)?;
                } else {
                    if bootloader {
                        build_bootloader(&sh, &root)?;
                        let elf = bootloader_elf(&root);
                        eprintln!("→ Downloading bootloader via probe-rs…");
                        flash_only_probe(&sh, &elf)?;
                        eprintln!("→ Resetting chip…");
                        reset_probe(&sh)?;
                    }
                    if application {
                        // Downloading application erases sector 0 (BOOT2 + bootloader).
                        // Re-download the bootloader afterward to restore sector 0.
                        build_app(&sh, &root)?;
                        let app_elf = get_app_elf(&root);
                        eprintln!("→ Downloading application via probe-rs…");
                        flash_only_probe(&sh, &app_elf)?;

                        build_bootloader(&sh, &root)?;
                        let bl_elf = bootloader_elf(&root);
                        eprintln!("→ Re-downloading bootloader (restores sector 0)…");
                        flash_only_probe(&sh, &bl_elf)?;

                        eprintln!("→ Resetting chip…");
                        reset_probe(&sh)?;

                        eprintln!("→ Attaching RTT (Ctrl+C to exit)…");
                        attach_rtt_probe(&sh, &app_elf)?;
                    }
                }
            } else {
                // UF2 drag-and-drop
                if flash_combined {
                    let uf2 = combined_uf2(&root);
                    if !uf2.exists() {
                        build_bootloader(&sh, &root)?;
                        build_application(&sh, &root)?;
                        combine(&sh, &root)?;
                    }
                    flash_uf2(&uf2)?;
                } else {
                    if bootloader {
                        let uf2 = bootloader_uf2(&root);
                        if !uf2.exists() {
                            build_bootloader(&sh, &root)?;
                        }
                        flash_uf2(&uf2)?;
                    }
                    if application {
                        let uf2 = application_uf2(&root);
                        if !uf2.exists() {
                            build_application(&sh, &root)?;
                        }
                        flash_uf2(&uf2)?;
                    }
                }
            }
        }

        Command::CheckTools => {
            eprintln!("Checking required tools…");
            let mut ok = true;
            ok &= check_tool("elf2uf2-rs", "cargo install elf2uf2-rs");
            ok &= check_tool("flip-link", "cargo install flip-link");
            // probe-rs is optional
            check_tool("probe-rs", "cargo install probe-rs-tools");
            if ok {
                eprintln!("All required tools found!");
            } else {
                bail!("Some required tools are missing");
            }
        }

        Command::Clean => {
            eprintln!("→ Cleaning…");
            {
                let _dir = sh.push_dir(root.join("bootloader"));
                cmd!(sh, "cargo clean").run()?;
            }
            {
                let _dir = sh.push_dir(root.join("application"));
                cmd!(sh, "cargo clean").run()?;
            }
            let build = build_dir(&root);
            if build.exists() {
                std::fs::remove_dir_all(&build)?;
            }
            eprintln!("  ✓ Clean complete");
        }
    }

    Ok(())
}
