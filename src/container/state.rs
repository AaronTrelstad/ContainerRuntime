use std::fs;
use std::path::{Path, PathBuf};
use serde::{Serialize, Deserialize};
use crate::cli::StateArgs;
use crate::types::AnyError;

const RUNTIME_DIR: &str = "/run/container-runtime";

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub enum ContainerStatus {
    Created,
    Running,
    Stopped,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ContainerState {
    pub id: String,
    pub pid: u32,
    pub status: ContainerStatus,
    pub bundle: String,
}

impl ContainerState {
    pub fn save(&self) -> Result<(), AnyError> {

    }

    pub fn load(container_id: &str) -> Result<Self, AnyError> {

    }

    pub fn delete(container_id: &str) -> Result<(), AnyError> {

    }
}

fn container_dir(id: &str) -> PathBuf {
    Path::new(RUNTIME_DIR).join(id)
}
