use crate::cli::KillArgs;
use crate::types::AnyError;
use nix::unistd::close;
use std::fs;
use std::os::unix::io::RawFd;

pub fn kill(args: KillArgs) -> Result<(), AnyError> {
    let run_dir = format!("/tmp/containerruntime/{}", args.container_id);

    let kill_fd: RawFd = fs::read_to_string(format!("{}/kill_fd", run_dir))
        .map_err(|_| format!("container '{}' not found or not running", args.container_id))?
        .trim()
        .parse()
        .map_err(|_| "invalid kill_fd")?;

    close(kill_fd)?;
    fs::remove_file(format!("{}/kill_fd", run_dir))?;

    println!("Container '{}' stopped", args.container_id);
    Ok(())
}
