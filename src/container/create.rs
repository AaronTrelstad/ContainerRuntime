use nix::sched::{CloneFlags, clone};
use nix::sys::signal::Signal;
use nix::sys::wait::{WaitStatus, waitpid};
use nix::unistd::{close, pipe, read, write};
use std::fs;
use std::os::fd::{BorrowedFd, IntoRawFd};
use std::os::unix::io::RawFd;

use crate::cli::{CreateArgs, StartArgs};
use crate::types::AnyError;

const MEMORY_LIMIT_BYTES: u64 = 512 * 1024 * 1024;
const CPU_QUOTA_USEC: u64 = 50_000;
const CPU_PERIOD_USEC: u64 = 100_000;
const CGROUP_ROOT: &str = "/sys/fs/cgroup/containerruntime";

pub fn create(args: CreateArgs) -> Result<(), AnyError> {
    cleanup_cgroup(&args.container_id)?;

    let (sync_read_fd, sync_write_fd) = {
        let (r, w) = pipe()?;
        (r.into_raw_fd(), w.into_raw_fd())
    };

    let (kill_read_fd, kill_write_fd) = {
        let (r, w) = pipe()?;
        (r.into_raw_fd(), w.into_raw_fd())
    };

    let flags = CloneFlags::CLONE_NEWPID
        | CloneFlags::CLONE_NEWNS
        | CloneFlags::CLONE_NEWUTS
        | CloneFlags::CLONE_NEWIPC
        | CloneFlags::CLONE_NEWNET;

    let mut stack: Vec<u8> = vec![0u8; 1024 * 1024];

    let child_pid = unsafe {
        clone(
            Box::new(
                move || match run_child(sync_read_fd, kill_read_fd, kill_write_fd) {
                    Ok(_) => 0,
                    Err(e) => {
                        eprintln!("error: {}", e);
                        -1
                    }
                },
            ),
            &mut stack,
            flags,
            Some(Signal::SIGCHLD as i32),
        )?
    };

    close(sync_read_fd)?;
    close(kill_read_fd)?;

    setup_cgroup(&args.container_id, child_pid.as_raw())?;

    unsafe { write(BorrowedFd::borrow_raw(sync_write_fd), &[1u8])? };
    close(sync_write_fd)?;

    let run_dir = format!("/tmp/containerruntime/{}", args.container_id);
    fs::create_dir_all(&run_dir)?;
    fs::write(format!("{}/pid", run_dir), child_pid.as_raw().to_string())?;
    fs::write(format!("{}/kill_fd", run_dir), kill_write_fd.to_string())?;

    println!("Container '{}' created", args.container_id);

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

    close(kill_write_fd)?;
    println!("Container '{}' exited", args.container_id);
    cleanup_cgroup(&args.container_id)?;
    fs::remove_dir_all(&run_dir)?;
    Ok(())
}

fn run_child(
    sync_read_fd: RawFd,
    kill_read_fd: RawFd,
    kill_write_fd: RawFd,
) -> Result<(), AnyError> {
    let mut buf = [0u8; 1];
    eprintln!("waiting for sync signal");
    unsafe { read(BorrowedFd::borrow_raw(sync_read_fd), &mut buf)? };
    close(sync_read_fd)?;
    close(kill_write_fd)?;

    eprintln!("inside container, PID={}", std::process::id());

    eprintln!("blocking on kill pipe");
    let mut kill_buf = [0u8; 1];
    unsafe { read(BorrowedFd::borrow_raw(kill_read_fd), &mut kill_buf)? };
    close(kill_read_fd)?;

    eprintln!("shutting down");
    Ok(())
}

fn setup_cgroup(container_id: &str, pid: i32) -> Result<(), AnyError> {
    let cgroup_path = format!("{}/{}", CGROUP_ROOT, container_id);

    fs::create_dir_all(&cgroup_path).map_err(|e| format!("create cgroup dir: {}", e))?;

    fs::write(
        format!("{}/cgroup.subtree_control", CGROUP_ROOT),
        "+cpu +memory",
    )
    .map_err(|e| format!("subtree_control: {}", e))?;

    fs::write(
        format!("{}/memory.max", cgroup_path),
        MEMORY_LIMIT_BYTES.to_string(),
    )
    .map_err(|e| format!("memory.max: {}", e))?;

    fs::write(
        format!("{}/cpu.max", cgroup_path),
        format!("{} {}", CPU_QUOTA_USEC, CPU_PERIOD_USEC),
    )
    .map_err(|e| format!("cpu.max: {}", e))?;

    fs::write(format!("{}/cgroup.procs", cgroup_path), pid.to_string())
        .map_err(|e| format!("cgroup.procs: {}", e))?;

    println!(
        "cgroup ready: memory={}MB cpu={}% path={}",
        MEMORY_LIMIT_BYTES / (1024 * 1024),
        (CPU_QUOTA_USEC * 100) / CPU_PERIOD_USEC,
        cgroup_path
    );

    Ok(())
}

pub fn cleanup_cgroup(container_id: &str) -> Result<(), AnyError> {
    let cgroup_path = format!("{}/{}", CGROUP_ROOT, container_id);

    if !std::path::Path::new(&cgroup_path).exists() {
        return Ok(());
    }

    let procs = fs::read_to_string(format!("{}/cgroup.procs", cgroup_path)).unwrap_or_default();

    for pid in procs.split_whitespace() {
        fs::write("/sys/fs/cgroup/cgroup.procs", pid)
            .map_err(|e| format!("failed to move pid {} to root: {}", pid, e))?;
    }

    for attempt in 1..=5 {
        match fs::remove_dir(&cgroup_path) {
            Ok(_) => return Ok(()),
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
