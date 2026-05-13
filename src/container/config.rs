// config.rs
use serde::Deserialize;
use std::path::Path;
use crate::types::AnyError;

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct OciConfig {
    pub oci_version: String,
    pub process: Process,
    pub root: Root,
    pub hostname: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct Root {
    pub path: String,
    #[serde(default)]
    pub readonly: bool,
}

#[derive(Deserialize, Debug)]
pub struct Process {
    pub args: Vec<String>,
    #[serde(default)]
    pub env: Vec<String>,
    pub cwd: String,
}