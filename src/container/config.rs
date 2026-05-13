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

pub fn load_config(bundle_path: &str) -> Result<OciConfig, AnyError> {
    let config_path = Path::new(bundle_path).join("config.json");
    
    if !config_path.exists() {
        return Err(format!("config.json not found in bundle: {}", bundle_path).into());
    }
    
    let contents = std::fs::read_to_string(&config_path)
        .map_err(|e| format!("failed to read config.json: {}", e))?;
    
    let config: OciConfig = serde_json::from_str(&contents)
        .map_err(|e| format!("failed to parse config.json: {}", e))?;
    
    validate(&config)?;
    Ok(config)
}

fn validate(config: &OciConfig) -> Result<(), AnyError> {
    if !config.oci_version.starts_with("1.") {
        return Err(format!("unsupported OCI version: {}", config.oci_version).into());
    }
    if config.process.args.is_empty() {
        return Err("process.args must not be empty".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_config() {
        let json = r#"{
            "ociVersion": "1.0.2",
            "process": { "args": ["sh"], "env": [], "cwd": "/" },
            "root": { "path": "rootfs" }
        }"#;
        let config: OciConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.process.args[0], "sh");
        assert!(!config.root.readonly);
    }
}