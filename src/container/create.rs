use nix::mount::{MntFlags, umount2};
use nix::sched::{CloneFlags, clone};
use nix::sys::signal::Signal;
use nix::sys::wait::{WaitStatus, waitpid};
use nix::unistd::{close, execv, pipe, read, write};
use std::ffi::CString;
use std::fs;
use std::os::fd::{BorrowedFd, IntoRawFd};
use std::os::unix::io::RawFd;

use crate::cli::{CreateArgs, StartArgs};
use crate::container::filesystem::{pivot_rootfs, prepare_rootfs};
use crate::container::seccomp::apply_seccomp_filter;
use crate::types::AnyError;

const DEFAULT_MEMORY_LIMIT: u64 = 512 * 1024 * 1024;
const DEFAULT_CPU_QUOTA: u64 = 50_000;
const DEFAULT_CPU_PERIOD: u64 = 100_000;
const DEFAULT_CGROUP_ROOT: &str = "/sys/fs/cgroup/containerruntime";

pub fn create(args: CreateArgs) -> Result<(), AnyError> {
    let config = crate::container::config::load_config(&args.bundle)?;

    let memory_limit = config.memory_limit.unwrap_or(DEFAULT_MEMORY_LIMIT);
    let cpu_quota   = config.cpu_quota.unwrap_or(DEFAULT_CPU_QUOTA);
    let cpu_period  = config.cpu_period.unwrap_or(DEFAULT_CPU_PERIOD);
    let cgroup_root = config.c_group_root.clone().unwrap_or_else(|| DEFAULT_CGROUP_ROOT.to_string());


    cleanup_cgroup(&args.container_id, &cgroup_root)?;

    let rootfs = prepare_rootfs(&args.container_id)?;

    let (sync_read_fd, sync_write_fd) = {
        let (r, w) = pipe()?;
        (r.into_raw_fd(), w.into_raw_fd())
    };

    let flags = CloneFlags::CLONE_NEWPID
        | CloneFlags::CLONE_NEWNS
        | CloneFlags::CLONE_NEWUTS
        | CloneFlags::CLONE_NEWIPC
        | CloneFlags::CLONE_NEWNET;

    let mut stack: Vec<u8> = vec![0u8; 1024 * 1024];

    let process_config = config.process.clone();
    let hostname = config.hostname.clone();

    let child_pid = unsafe {
        clone(
            Box::new(move || match run_child(sync_read_fd, rootfs.clone(), process_config.clone(), hostname.clone(),) {
                Ok(_) => 0,
                Err(e) => {
                    eprintln!("[child] error: {}", e);
                    -1
                }
            }),
            &mut stack,
            flags,
            Some(Signal::SIGCHLD as i32),
        )?
    };

    close(sync_read_fd)?;

    setup_cgroup(&args.container_id, child_pid.as_raw(), memory_limit, cpu_quota, cpu_period, &cgroup_root)?;
    unsafe { write(BorrowedFd::borrow_raw(sync_write_fd), &[1u8])? };
    close(sync_write_fd)?;

    let run_dir = format!("/tmp/containerruntime/{}", args.container_id);
    fs::create_dir_all(&run_dir)?;
    fs::write(format!("{}/pid", run_dir), child_pid.as_raw().to_string())?;
    fs::write(format!("{}/status", run_dir), "created")?;

    crate::container::start::start(StartArgs {
        container_id: args.container_id.clone(),
    })?;

    match waitpid(child_pid, None) {
        Ok(WaitStatus::Exited(pid, code)) => {
            eprintln!("child {} exited with code {}", pid, code);
        }
        Ok(WaitStatus::Signaled(pid, signal, _)) => {
            eprintln!("child {} killed by signal {:?}", pid, signal);
        }
        Ok(other) => {
            eprintln!("waitpid: {:?}", other);
        }
        Err(nix::errno::Errno::ECHILD) => {
            eprintln!("ECHILD");
        }
        Err(e) => return Err(Box::new(e)),
    }

    cleanup_cgroup(&args.container_id, &cgroup_root)?;
    fs::remove_dir_all(&run_dir)?;
    Ok(())
}

fn run_child(sync_read_fd: RawFd, rootfs: String, process: crate::container::config::Process, hostname: Option<String>) -> Result<(), AnyError> {
    let mut buf = [0u8; 1];
    unsafe { read(BorrowedFd::borrow_raw(sync_read_fd), &mut buf)? };
    close(sync_read_fd)?;
    pivot_rootfs(&rootfs)?;

    if let Some(name) = hostname {
        nix::unistd::sethostname(&name)?;
    }

    std::env::set_current_dir(&process.cwd).map_err(|e| format!("set cwd to {}: {}", process.cwd, e))?;

    apply_seccomp_filter()?;
    
    let argv: Vec<CString> = process.args.iter()
        .map(|s| CString::new(s.as_str()))
        .collect::<Result<_, _>>()?;
    let envp: Vec<CString> = process.env.iter()
        .map(|s| CString::new(s.as_str()))
        .collect::<Result<_, _>>()?;
        
    nix::unistd::execve(&argv[0], &argv, &envp)?;

    Ok(())
}

fn setup_cgroup(container_id: &str, pid: i32, memory_limit: u64, cpu_quota: u64, cpu_period: u64, cgroup_root: &str,) -> Result<(), AnyError> {
    let cgroup_path = format!("{}/{}", cgroup_root, container_id);

    fs::create_dir_all(&cgroup_path).map_err(|e| format!("create cgroup dir: {}", e))?;

    fs::write(
        format!("{}/cgroup.subtree_control", cgroup_root),
        "+cpu +memory",
    )
    .map_err(|e| format!("subtree_control: {}", e))?;

    fs::write(
        format!("{}/memory.max", cgroup_path),
        memory_limit.to_string(),
    )
    .map_err(|e| format!("memory.max: {}", e))?;

    fs::write(
        format!("{}/cpu.max", cgroup_path),
        format!("{} {}", cpu_quota, cpu_period),
    )
    .map_err(|e| format!("cpu.max: {}", e))?;

    fs::write(format!("{}/cgroup.procs", cgroup_path), pid.to_string())
        .map_err(|e| format!("cgroup.procs: {}", e))?;

    Ok(())
}

pub fn cleanup_cgroup(container_id: &str, cgroup_root: &str) -> Result<(), AnyError> {
    let rootfs = format!("/tmp/containerruntime/{}/rootfs", container_id);
    for dir in &["proc", "sys", "dev", "tmp"] {
        let path = format!("{}/{}", rootfs, dir);
        umount2(path.as_str(), MntFlags::MNT_DETACH).ok();
    }
    umount2(rootfs.as_str(), MntFlags::MNT_DETACH).ok();

    let cgroup_path = format!("{}/{}", cgroup_root, container_id);

    if !std::path::Path::new(&cgroup_path).exists() {
        fs::remove_dir(cgroup_root).ok();
        return Ok(());
    }

    let procs = fs::read_to_string(format!("{}/cgroup.procs", cgroup_path)).unwrap_or_default();

    for pid in procs.split_whitespace() {
        fs::write("/sys/fs/cgroup/cgroup.procs", pid)
            .map_err(|e| format!("failed to move pid {} to root: {}", pid, e))?;
    }

    for attempt in 1..=5 {
        match fs::remove_dir(&cgroup_path) {
            Ok(_) => {
                fs::remove_dir(cgroup_root).ok();
                return Ok(());
            }
            Err(e) if e.raw_os_error() == Some(16) => {
                std::thread::sleep(std::time::Duration::from_millis(200));
                if attempt == 5 {
                    return Err("cgroup still busy".into());
                }
            }
            Err(e) => return Err(format!("remove cgroup: {}", e).into()),
        }
    }

    Ok(())
}