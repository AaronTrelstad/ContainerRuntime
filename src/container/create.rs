use nix::sched::{clone, CloneFlags};
use nix::sys::wait::waitpid;
use nix::unistd::{execvp, Pid};
use std::ffi::CString;

use crate::cli::CreateArgs;
use crate::types::AnyError;

pub fn create(args: CreateArgs) -> Result<(), AnyError> {
    /*
    CLONE_NEWPID: This creates a new PID namespace, this isolates the child process so the PID is 1
    CLONE_NEWNS: Copies the parents filesystem mounts to the child, so the child can mount/unmount without affecting other processes
    CLONE_NEWUTS: Creates a new UTS (Unix Timesharing System) namespace for the child process, so it has its own hostname and domain name
    CLONE_NEWIPC: Isolates IPC objects, prevents access of shared memory, semaphores or message queues
    CLONE_NETNET: Isolates network interfaces, IP addresses, routing tables and socket listings

    TODO
    CLONE_NEWCGROUP
    CLONE_NEWUSER
    */
    let flags = CloneFlags::CLONE_NEWPID 
        | CloneFlags::CLONE_NEWNS
        | CloneFlags::CLONE_NEWUTS
        | CloneFlags::CLONE_NEWIPC
        | CloneFlags::CLONE_NETNET;

    let mut stack: Vec<u8> = vec![0u8; 1024 * 1024];

    let child_pid = unsafe {
        clone(
            Box::new(|| match run_child() {
                Ok(_) => 0,
                Err(e) => {
                    eprintln!("child error: {}", e);
                    -1
                }
            }),
            &mut stack,
            flags,
            None
        )?
    };

    println!("Container PID: {}", child_pid);
    waitpid(child, None)?;
    println!("Container exited");

    Ok(())
}
