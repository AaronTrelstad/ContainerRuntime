use nix::sys::signal::{Signal, kill as nix_kill};
use nix::unistd::Pid;
use std::fs;
use crate::cli::KillArgs;
use crate::types::AnyError;

pub fn kill(args: KillArgs) -> Result<(), AnyError> {
    let run_dir = format!("/tmp/containerruntime/{}", args.container_id);

    let pid_str = fs::read_to_string(format!("{}/pid", run_dir))
        .map_err(|_| format!("container '{}' not found", args.container_id))?;

    let pid: i32 = pid_str.trim().parse()
        .map_err(|_| "invalid pid")?;

    nix_kill(Pid::from_raw(pid), Signal::SIGTERM)?;

    println!("Container '{}' killed", args.container_id);
    Ok(())
}
