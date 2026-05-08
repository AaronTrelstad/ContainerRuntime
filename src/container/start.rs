use nix::unistd::{close, read};
use std::fs;
use std::os::fd::BorrowedFd;
use std::os::unix::io::RawFd;
use crate::cli::StartArgs;
use crate::types::AnyError;

pub fn start(args: StartArgs) -> Result<(), AnyError> {
    let run_dir = format!("/tmp/containerruntime/{}", args.container_id);

    if !std::path::Path::new(&run_dir).exists() {
        return Err(format!("container '{}' not found", args.container_id).into());
    }

    let kill_fd: RawFd = fs::read_to_string(format!("{}/kill_fd", run_dir))
        .map_err(|_| format!("container '{}' is not in created state", args.container_id))?
        .trim()
        .parse()
        .map_err(|_| "invalid kill_fd")?;

    println!("Container '{}' started", args.container_id);

    let mut buf = [0u8; 1];
    unsafe { read(BorrowedFd::borrow_raw(kill_fd), &mut buf)? };
    close(kill_fd)?;

    println!("Container '{}' shutting down...", args.container_id);
    Ok(())
}
