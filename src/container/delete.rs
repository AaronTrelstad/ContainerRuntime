use crate::cli::DeleteArgs;
use crate::container::create::cleanup_cgroup;
use crate::types::AnyError;
use std::fs;

pub fn delete(args: DeleteArgs) -> Result<(), AnyError> {
    let run_dir = format!("/tmp/containerruntime/{}", args.container_id);

    if std::path::Path::new(&format!("{}/kill_fd", run_dir)).exists() {
        return Err(format!(
            "container '{}' is still running",
            args.container_id
        )
        .into());
    }

    cleanup_cgroup(&args.container_id)?;

    if std::path::Path::new(&run_dir).exists() {
        fs::remove_dir_all(&run_dir)?;
    }

    println!("Container '{}' deleted", args.container_id);
    Ok(())
}
