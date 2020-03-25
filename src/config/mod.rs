use serde::{Deserialize, Serialize};
use toml;
// File system modules
use std::error::Error;
use std::fs::File;
use std::io::prelude::*;
use std::path::Path;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Config {
    pub listen_address: String,
    pub midi_port: Option<usize>,
}

// Wrap read whole file process up and handle errors
pub fn slurp(file_path: String) -> Result<String, String> {
    let path = Path::new(&file_path);

    File::open(&path)
        .map_err(|error| {
            format!(
                "Failed opening path {}: {}",
                path.display(),
                error.description(),
            )
        })
        .and_then(|mut file| {
            let mut content = String::new();
            file.read_to_string(&mut content)
                .map_err(|error| {
                    format!(
                        "Failed reading from {}: {}",
                        path.display(),
                        error.description()
                    )
                })
                .and_then(|_| Ok(content))
        })
}

// Wrap up the toml to Config parsing
pub fn config_from_toml(config: String) -> Result<Config, String> {
    toml::from_str::<Config>(&config)
        .map_err(|error| format!("Failed parsing configuration: {}", error.description()))
}

// A simple default configuration generator
pub fn default_config() -> Config {
    Config {
        listen_address: "127.0.0.1:10009".to_string(),
        midi_port: Some(0),
    }
}

pub fn config_to_toml(config: Config) -> String {
    toml::to_string(&config).unwrap()
}

pub fn default_config_toml() -> String {
    config_to_toml(default_config())
}
