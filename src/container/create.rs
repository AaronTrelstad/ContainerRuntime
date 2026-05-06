use nix::sched::{CloneFlags, clone};
use nix::sys::wait::waitpid;
use nix::unistd::{close, pipe, read, write};
use std::fs;
use std::os::fd::{BorrowedFd, IntoRawFd};
use std::os::unix::io::RawFd;

use crate::cli::CreateArgs;
use crate::types::AnyError;

const MEMORY_LIMIT_BYTES: u64 = 512 * 1024 * 1024;
const CPU_QUOTA_USEC: u64 = 50_000;
const CPU_PERIOD_USEC: u64 = 100_000;
const CGROUP_ROOT: &str = "/sys/fs/cgroup/containerruntime";

pub fn create(args: CreateArgs) -> Result<(), AnyError> {
    let (read_fd, write_fd) = {
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
            Box::new(move || match run_child(read_fd) {
                Ok(_) => 0,
                Err(e) => {
                    eprintln!("child error: {}", e);
                    -1
                }
            }),
            &mut stack,
            flags,
            None,
        )?
    };

    setup_cgroup(&args.container_id, child_pid.as_raw())?;

    unsafe { write(BorrowedFd::borrow_raw(write_fd), &[1u8])? };
    close(write_fd)?;

    println!(
        "Container '{}' created (PID {})",
        args.container_id, child_pid
    );
    match waitpid(child_pid, None) {
        Ok(_) => {}
        Err(nix::errno::Errno::ECHILD) => {}
        Err(e) => return Err(Box::new(e)),
    }
    println!("Container '{}' exited", args.container_id);

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

fn run_child(read_fd: RawFd) -> Result<(), AnyError> {
    let mut buf = [0u8; 1];
    unsafe { read(BorrowedFd::borrow_raw(read_fd), &mut buf)? };
    close(read_fd)?;

    println!("inside container (PID={})", std::process::id());
    Ok(())
}
