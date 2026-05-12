use std::fs;
use std::path::Path;
use std::process::Command;

use nix::mount::{MntFlags, MsFlags, mount, umount2};
use nix::sys::stat::{Mode, SFlag, makedev, mknod};

use crate::types::AnyError;

const ROOTFS_DIRS: &[&str] = &[
    "bin",
    "sbin",
    "usr/bin",
    "usr/sbin",
    "lib",
    "lib64",
    "usr/lib",
    "usr/lib64",
    "proc",
    "sys",
    "dev",
    "tmp",
    "etc",
    "root",
    ".old_root",
];

const HOST_BINARIES: &[&str] = &["/bin/sh", "/bin/ls", "/bin/cat", "/bin/echo", "/bin/ps"];

pub fn prepare_rootfs(container_id: &str) -> Result<String, AnyError> {
    let rootfs = format!("/tmp/containerruntime/{}/rootfs", container_id);

    for dir in ROOTFS_DIRS {
        fs::create_dir_all(format!("{}/{}", rootfs, dir))?;
    }

    let lib64_link = format!("{}/lib64", rootfs);
    let lib_link = format!("{}/lib", rootfs);

    if Path::new(&lib64_link).is_dir() && !Path::new(&lib64_link).is_symlink() {
        if fs::read_dir(&lib64_link)?.next().is_none() {
            fs::remove_dir(&lib64_link)?;
            std::os::unix::fs::symlink("usr/lib64", &lib64_link)?;
        }
    }
    if Path::new(&lib_link).is_dir() && !Path::new(&lib_link).is_symlink() {
        if fs::read_dir(&lib_link)?.next().is_none() {
            fs::remove_dir(&lib_link)?;
            std::os::unix::fs::symlink("usr/lib", &lib_link)?;
        }
    }

    fs::write(
        format!("{}/etc/passwd", rootfs),
        "root:x:0:0:root:/root:/bin/sh\n",
    )?;
    fs::write(format!("{}/etc/hosts", rootfs), "127.0.0.1 localhost\n")?;
    fs::write(format!("{}/etc/hostname", rootfs), "container\n")?;

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

pub fn pivot_rootfs(rootfs: &str) -> Result<(), AnyError> {
    mount(
        None::<&str>,
        "/",
        None::<&str>,
        MsFlags::MS_PRIVATE | MsFlags::MS_REC,
        None::<&str>,
    )?;

    mount(
        Some(rootfs),
        rootfs,
        None::<&str>,
        MsFlags::MS_BIND | MsFlags::MS_REC,
        None::<&str>,
    )?;

    mount(
        Some("proc"),
        format!("{}/proc", rootfs).as_str(),
        Some("proc"),
        MsFlags::empty(),
        None::<&str>,
    )?;

    mount(
        Some("sysfs"),
        format!("{}/sys", rootfs).as_str(),
        Some("sysfs"),
        MsFlags::empty(),
        None::<&str>,
    )?;

    mount(
        Some("tmpfs"),
        format!("{}/dev", rootfs).as_str(),
        Some("tmpfs"),
        MsFlags::empty(),
        None::<&str>,
    )?;

    create_devices(rootfs)?;

    mount(
        Some("tmpfs"),
        format!("{}/tmp", rootfs).as_str(),
        Some("tmpfs"),
        MsFlags::empty(),
        None::<&str>,
    )?;

    let old_root = format!("{}/.old_root", rootfs);
    nix::unistd::pivot_root(rootfs, old_root.as_str())?;

    std::env::set_current_dir("/")?;

    umount2("/.old_root", MntFlags::MNT_DETACH)?;

    fs::remove_dir("/.old_root").ok();

    eprintln!("[fs] pivot_root complete, host filesystem detached");
    Ok(())
}

fn create_devices(rootfs: &str) -> Result<(), AnyError> {
    let dev_path = format!("{}/dev", rootfs);

    let devices = &[
        ("null", 1u64, 3u64),
        ("zero", 1, 5),
        ("random", 1, 8),
        ("urandom", 1, 9),
        ("tty", 5, 0),
    ];

    let mode = Mode::from_bits_truncate(0o666);

    for (name, major, minor) in devices {
        let path = format!("{}/{}", dev_path, name);
        mknod(
            Path::new(&path),
            SFlag::S_IFCHR,
            mode,
            makedev(*major, *minor),
        )
        .or_else(|e| {
            if e == nix::errno::Errno::EEXIST {
                Ok(())
            } else {
                Err(e)
            }
        })?;
    }

    eprintln!("[fs] /dev devices created");
    Ok(())
}

fn copy_file_into_rootfs(src: &str, rootfs: &str) -> Result<(), AnyError> {
    let dest = format!("{}{}", rootfs, src);
    if let Some(parent) = Path::new(&dest).parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(src, &dest).map_err(|e| format!("copy {} -> {}: {}", src, dest, e))?;
    Ok(())
}

fn copy_libs(binary: &str, rootfs: &str) -> Result<(), AnyError> {
    let output = match Command::new("ldd").arg(binary).output() {
        Ok(o) => o,
        Err(_) => return Ok(()),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);

    for line in stdout.lines() {
        let lib = match parse_ldd_line(line) {
            Some(p) if p.starts_with('/') => p,
            _ => continue,
        };

        if !Path::new(lib).exists() {
            continue;
        }

        let dest = format!("{}{}", rootfs, lib);

        if Path::new(&dest).exists() {
            continue;
        }

        if let Some(parent) = Path::new(&dest).parent() {
            fs::create_dir_all(parent)?;
        }

        if Path::new(lib).is_symlink() {
            let real = fs::canonicalize(lib)?;
            let link_target = fs::read_link(lib)?;

            let lib_dir = Path::new(&dest).parent().unwrap();
            let real_filename = real.file_name().ok_or("library has no filename")?;
            let real_dest = lib_dir.join(real_filename);

            if !real_dest.exists() {
                eprintln!("[fs] copying {} -> {}", real.display(), real_dest.display());
                fs::copy(&real, &real_dest).map_err(|e| {
                    format!(
                        "copy real lib {} -> {}: {}",
                        real.display(),
                        real_dest.display(),
                        e
                    )
                })?;
            }

            std::os::unix::fs::symlink(&link_target, &dest).ok();
        } else {
            fs::copy(lib, &dest)?;
        }
    }

    Ok(())
}

fn parse_ldd_line(line: &str) -> Option<&str> {
    let line = line.trim();
    if line.contains("=>") {
        let rhs = line.split("=>").nth(1)?.trim();
        let path = rhs.split_whitespace().next()?;
        if path == "not" {
            None // "not found"
        } else {
            Some(path)
        }
    } else {
        let path = line.split_whitespace().next()?;
        if path.starts_with('/') {
            Some(path)
        } else {
            None
        }
    }
}
