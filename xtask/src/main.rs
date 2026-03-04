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
        /// Use probe-rs instead of UF2 drag-and-drop
        #[arg(long)]
        probe: bool,
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

    let src = root
        .join("bootloader")
        .join("target/thumbv6m-none-eabi/release/pdu-rp-bootloader");
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

    let src = root
        .join("application")
        .join("target/thumbv6m-none-eabi/release/pdu-rp-application");
    let dst = application_elf(root);
    std::fs::copy(&src, &dst)
        .with_context(|| format!("copy {} → {}", src.display(), dst.display()))?;

    let uf2 = application_uf2(root);
    cmd!(sh, "elf2uf2-rs {dst} {uf2}").run()?;
    eprintln!("  ✓ {}", uf2.display());
    Ok(())
}

// ── Combine ───────────────────────────────────────────────────────────────────

fn combine(sh: &Shell, root: &Path) -> Result<()> {
    eprintln!("→ Combining bootloader + application…");
    let bl_elf = bootloader_elf(root);
    let app_elf = application_elf(root);
    let bl_bin = root.join("build/bootloader.bin");
    let app_bin = root.join("build/application.bin");
    let combined_bin = root.join("build/combined.bin");
    let combined = combined_uf2(root);
    let script = root.join("scripts/combine_binaries.py");

    cmd!(
        sh,
        "python3 {script} {bl_elf} {app_elf} {bl_bin} {app_bin} {combined_bin} {combined}"
    )
    .run()?;
    eprintln!("  ✓ {}", combined.display());
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

fn flash_probe(sh: &Shell, elf_path: &Path) -> Result<()> {
    cmd!(sh, "probe-rs run --chip RP2040 {elf_path}").run()?;
    Ok(())
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

    let data =
        std::fs::read(uf2_path).with_context(|| format!("reading {}", uf2_path.display()))?;

    let response = ureq::post(&url)
        .set("Authorization", &auth)
        .set("Content-Type", "application/octet-stream")
        .send_bytes(&data)?;

    if response.status() == 200 {
        eprintln!("  ✓ Firmware uploaded — device will reboot");
        Ok(())
    } else {
        bail!("OTA failed: HTTP {}", response.status())
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
            ota,
        } => {
            let flash_combined = !bootloader && !application;

            if let Some(ip) = &ota {
                // OTA mode: always flashes the application UF2
                let uf2 = application_uf2(&root);
                if !uf2.exists() {
                    eprintln!("Application UF2 not found — building first…");
                    build_application(&sh, &root)?;
                }
                flash_ota(&uf2, ip)?;
            } else if probe {
                if flash_combined || bootloader {
                    let elf = bootloader_elf(&root);
                    if !elf.exists() {
                        build_bootloader(&sh, &root)?;
                    }
                    flash_probe(&sh, &elf)?;
                }
                if flash_combined || application {
                    let elf = application_elf(&root);
                    if !elf.exists() {
                        build_application(&sh, &root)?;
                    }
                    flash_probe(&sh, &elf)?;
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
            ok &= check_tool("python3", "install Python 3 from https://python.org");
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
