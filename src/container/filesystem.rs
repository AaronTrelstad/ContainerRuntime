use std::fs;
use std::path::Path;
use std::process::Command;
#[cfg(target_os = "linux")]
use nix::mount::{MsFlags, MntFlags, mount, umount2};
use crate::types::AnyError;

const ROOTFS_DIRS: &[&str] = &[
    "bin", "sbin", "usr/bin", "usr/sbin",
    "lib", "lib64", "usr/lib", "usr/lib64",
    "proc", "sys", "dev", "tmp", "etc", "root", ".old_root",
];

// Binaries to copy from the EC2 host into the container
const HOST_BINARIES: &[&str] = &[
    "/bin/sh",
    "/bin/ls",
    "/bin/cat",
    "/bin/echo",
    "/bin/ps",
];

/// Called from the parent process before clone().
/// Creates the rootfs directory and populates it with binaries + libraries.
pub fn prepare_rootfs(container_id: &str) -> Result<String, AnyError> {
    let rootfs = format!("/tmp/containerruntime/{}/rootfs", container_id);

    // Create directory structure
    for dir in ROOTFS_DIRS {
        fs::create_dir_all(format!("{}/{}", rootfs, dir))?;
    }

    // Minimal /etc files so the shell feels at home
    fs::write(
        format!("{}/etc/passwd", rootfs),
        "root:x:0:0:root:/root:/bin/sh\n",
    )?;
    fs::write(
        format!("{}/etc/hosts", rootfs),
        "127.0.0.1 localhost\n",
    )?;
    fs::write(
        format!("{}/etc/hostname", rootfs),
        "container\n",
    )?;

    // Copy binaries and their shared libraries
    for bin in HOST_BINARIES {
        if !Path::new(bin).exists() {
            eprintln!("[fs] skipping {} (not found on host)", bin);
            continue;
        }
        copy_file_into_rootfs(bin, &rootfs)?;
        copy_libs(bin, &rootfs)?;
    }

    eprintln!("[fs] rootfs ready at {}", rootfs);
    Ok(rootfs)
}

#[cfg(target_os = "linux")]
/// Called from the child process after receiving the sync signal.
/// Bind-mounts rootfs, sets up proc/sys/dev, then pivot_roots into it.
pub fn pivot_rootfs(rootfs: &str) -> Result<(), AnyError> {
    // Bind mount rootfs onto itself — pivot_root requires the
    // new root to be a mount point distinct from the current root.
    mount(
        Some(rootfs),
        rootfs,
        None::<&str>,
        MsFlags::MS_BIND | MsFlags::MS_REC,
        None::<&str>,
        
    )?;

    // proc — needed for /proc/self, ps, etc.
    mount(
        Some("proc"),
        &format!("{}/proc", rootfs),
        Some("proc"),
        MsFlags::empty(),
        None::<&str>,
    )?;

    // sysfs
    mount(
        Some("sysfs"),
        &format!("{}/sys", rootfs),
        Some("sysfs"),
        MsFlags::empty(),
        None::<&str>,
    )?;

    // devtmpfs — gives the container /dev/null, /dev/zero, etc.
    mount(
        Some("devtmpfs"),
        &format!("{}/dev", rootfs),
        Some("devtmpfs"),
        MsFlags::empty(),
        None::<&str>,
    )?;

    // tmpfs for /tmp
    mount(
        Some("tmpfs"),
        &format!("{}/tmp", rootfs),
        Some("tmpfs"),
        MsFlags::empty(),
        None::<&str>,
    )?;

    // pivot_root: swap the container's / to rootfs, stash old / in .old_root
    let old_root = format!("{}/.old_root", rootfs);
    nix::unistd::pivot_root(rootfs, &old_root)?;

    // Move into the new root
    std::env::set_current_dir("/")?;

    // Detach the old host root — MNT_DETACH means it vanishes once
    // all existing references to it inside this namespace are gone.
    umount2("/.old_root", MntFlags::MNT_DETACH)?;

    // Remove the now-empty mountpoint
    fs::remove_dir("/.old_root").ok();

    eprintln!("[fs] pivot_root complete, host filesystem detached");
    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub fn pivot_rootfs(_rootfs: &str) -> Result<(), AnyError> {
    Err("pivot_rootfs is only supported on Linux".into())
}

// ── helpers ──────────────────────────────────────────────────────────────────

/// Copy a single file from its absolute host path into the same path under rootfs.
fn copy_file_into_rootfs(src: &str, rootfs: &str) -> Result<(), AnyError> {
    let dest = format!("{}{}", rootfs, src);
    if let Some(parent) = Path::new(&dest).parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(src, &dest)
        .map_err(|e| format!("copy {} → {}: {}", src, dest, e))?;
    Ok(())
}

/// Use `ldd` to find shared libraries for a binary and copy them into rootfs.
fn copy_libs(binary: &str, rootfs: &str) -> Result<(), AnyError> {
    let output = match Command::new("ldd").arg(binary).output() {
        Ok(o) => o,
        // Static binary — no libraries needed
        Err(_) => return Ok(()),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);

    for line in stdout.lines() {
        let lib_path = parse_ldd_line(line);

        let lib = match lib_path {
            Some(p) if p.starts_with('/') => p,
            _ => continue,
        };

        if !Path::new(lib).exists() {
            continue;
        }

        let dest = format!("{}{}", rootfs, lib);

        if Path::new(&dest).exists() {
            continue; // already copied
        }

        if let Some(parent) = Path::new(&dest).parent() {
            fs::create_dir_all(parent)?;
        }

        if Path::new(lib).is_symlink() {
            // Copy the real file first, then recreate the symlink
            let real = fs::canonicalize(lib)?;
            let real_dest = format!("{}{}", rootfs, real.display());

            if let Some(parent) = Path::new(&real_dest).parent() {
                fs::create_dir_all(parent)?;
            }
            if !Path::new(&real_dest).exists() {
                fs::copy(&real, &real_dest)?;
            }

            let link_target = fs::read_link(lib)?;
            std::os::unix::fs::symlink(&link_target, &dest).ok();
        } else {
            fs::copy(lib, &dest)?;
        }
    }

    Ok(())
}

/// Parse one line of `ldd` output and return the library path if present.
///
/// ldd lines come in two shapes:
///   libfoo.so => /lib/x86_64-linux-gnu/libfoo.so (0x...)
///   /lib64/ld-linux-x86-64.so.2 (0x...)
fn parse_ldd_line(line: &str) -> Option<&str> {
    let line = line.trim();
    if line.contains("=>") {
        // Take the right-hand side, first token
        let rhs = line.split("=>").nth(1)?.trim();
        let path = rhs.split_whitespace().next()?;
        if path == "not" {
            None // "not found"
        } else {
            Some(path)
        }
    } else {
        // Lines like "/lib64/ld-linux-x86-64.so.2 (0x...)"
        let path = line.split_whitespace().next()?;
        if path.starts_with('/') { Some(path) } else { None }
    }
}