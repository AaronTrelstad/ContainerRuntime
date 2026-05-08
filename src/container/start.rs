use crate::cli::StartArgs;
use crate::types::AnyError;
use std::fs;

pub fn start(args: StartArgs) -> Result<(), AnyError> {
    let run_dir = format!("/tmp/containerruntime/{}", args.container_id);

    if !std::path::Path::new(&run_dir).exists() {
        return Err(format!("container '{}' not found", args.container_id).into());
    }

    fs::write(format!("{}/status", run_dir), "running")?;

    println!("Container '{}' started", args.container_id);

    Ok(())
}
