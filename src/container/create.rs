use crate::cli::CreateArgs;
use crate::types::AnyError;
use nix::sched::{clone, CloneFlags};
use nix::unistd::{chroot, chdir};
use nix::mount::{mount, MsFlags};

pub fn create(args: CreateArgs) -> Result<(), AnyError> {
    todo!()
}
