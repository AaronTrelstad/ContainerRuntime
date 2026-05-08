use crate::cli::StateArgs;
use crate::types::AnyError;
use std::fs;

pub fn state(args: StateArgs) -> Result<(), AnyError> {
    let run_dir = format!("/tmp/containerruntime/{}", args.container_id);
    let pid_path = format!("{}/pid", run_dir);
    let kill_fd_path = format!("{}/kill_fd", run_dir);

    if !std::path::Path::new(&pid_path).exists() {
        return Err(format!("container '{}' not found", args.container_id).into());
    }

    let pid = fs::read_to_string(&pid_path)?;
    let status = if std::path::Path::new(&kill_fd_path).exists() {
        "running"
    } else {
        "stopped"
    };

    println!("id: {}", args.container_id);
    println!("pid: {}", pid.trim());
    println!("status: {}", status);

    Ok(())
}
