use std::fmt::Debug;
use std::fs;
use std::path::{PathBuf};

pub use config::{Config, ConfigError, FileFormat};
use home::home_dir;
use pem::Pem;

use crate::project;

#[derive(thiserror::Error, Debug)]
pub enum LoadError {
    #[error("Failed to load config: {0}")]
    ConfigError(#[from] ConfigError)
}

#[derive(thiserror::Error, Debug)]
pub enum WriteError {
    #[error("Failed to write config: {0}")]
    ConfigError(#[from] ConfigError)
}

#[derive(Clone)]
pub struct LoadedConfig {
    pub config: Config,
    pub redacted_config: Config,
    pub config_files_used: Vec<PathBuf>,
    pub config_files_declared: Vec<PathBuf>,
}

impl Debug for LoadedConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoadedConfig")
            .field("config", &self.redacted_config)
            .field("config_files_used", &self.config_files_used)
            .field("config_files_declared", &self.config_files_declared)
            .finish()
    }
}

/// Load configuration from files and environment variables used by openDuT.
///
/// This includes in following order:
/// * A default configuration, provided as a string
/// * A development configuration, read from the crate's directory
/// * A system configuration, read from `/etc/opendut/{name}.toml`
/// * A user configuration, read from `[XDG_CONFIG_HOME|~/.config]/opendut/{name}/config.toml`
/// * Environment variables prefixed with `OPENDUT_{NAME}_`
/// * The `overrides` passed as parameter.
///
pub fn load_config(name: &str, defaults: &str, defaults_format: FileFormat, overrides: Config, secret_redacted_overrides: Config) -> Result<LoadedConfig, LoadError> {

    let development_config = format!("opendut-{name}/{name}-development.toml");
    let system_config = format!("/etc/opendut/{name}.toml");
    let user_config = format!("opendut/{name}/config.toml");

    let builder = Config::builder()
        .add_source(config::File::from_str(defaults, defaults_format));

    let mut config_files = Vec::new();

    if project::is_running_in_development() {
        config_files.push(project::make_path_absolute(development_config).ok())
    }

    config_files.push(Some(PathBuf::from(system_config)));

    match std::env::var("XDG_CONFIG_HOME") {
        Ok(xdg_config_home) => {
            config_files.push(Some(PathBuf::from(xdg_config_home).join(user_config)));
        }
        Err(_) => {
            config_files.push(home_dir().map(|path| path.join(".config").join(user_config)));
        }
    }

    let (sources_used, sources_declared): (Vec<PathBuf>, Vec<PathBuf>) = config_files.into_iter()
        .fold((Vec::new(), Vec::new()), |(mut used, mut declared), path| {
            if let Some(path) = path {
                declared.push(Clone::clone(&path));
                if path.exists() && path.is_file() {
                    used.push(path);
                }
            }
            (used, declared)
        });

    let builder = sources_used.iter()
        .cloned()
        .fold(builder, |builder, path| {
            builder.add_source(config::File::from(path).required(false))
        });

    let builder = builder.add_source(
        config::Environment::with_prefix(&format!("OPENDUT_{}", name.to_uppercase()))
            .separator("_")
            .try_parsing(true)
    );

    let settings = builder.add_source(overrides);
    let secret_redacted_settings = settings.clone()
        .add_source(secret_redacted_overrides);     

    Ok(LoadedConfig {
        config: settings.build()?,
        redacted_config: secret_redacted_settings.build()?,
        config_files_used: sources_used,
        config_files_declared: sources_declared,
    })
}

#[derive(Clone)]
pub enum SetupType {
    System,
    User
}

/// Write configuration to one of the following paths:
///
/// * A system configuration, write to `/etc/opendut/{name}.toml`
/// * A user configuration, write to `[XDG_CONFIG_HOME|~/.config]/opendut/{name}/config.toml`
///
pub fn write_config(name: &str, settings_string: &str, user_type: SetupType) -> Result<(), WriteError> {
    
    let config = match user_type {
        SetupType::System => { format!("/etc/opendut/{name}.toml") }
        SetupType::User => {  format!("opendut/{name}/config.toml") }
    };

    let config_path = match std::env::var("XDG_CONFIG_HOME") {
        Ok(xdg_config_home) => {
            PathBuf::from(xdg_config_home).join(config)
        }
        Err(_) => {
            home_dir().map(|path| path.join(".config").join(config)).unwrap()
        }
    };

    let parent_dir = config_path
        .parent()
        .ok_or_else(|| format!("Expected configuration file '{}' to have a parent directory.", config_path.display())).unwrap();
    fs::create_dir_all(parent_dir).expect("Could not create directory");

    fs::write(config_path, settings_string).expect("Could not write file");
    Ok(())
}

/// Write certificate to one of the following paths:
///
/// * A system configuration, write to `/etc/opendut/{name}-ca.pem`
/// * A user configuration, write to `[XDG_DATA_HOME|~/.local/share]/opendut/{name}/ca.pem`
///
pub fn write_certificate(name: &str, ca: Pem, user_type: SetupType) -> Result<PathBuf, WriteError> {

    let certificate = match user_type {
        SetupType::System => { format!("/etc/opendut/{name}-ca.pem") }
        SetupType::User => {  format!("opendut/{name}/ca.pem") }
    };

    let certificate_path = match std::env::var("XDG_DATA_HOME") {
        Ok(xdg_data_home) => {
            PathBuf::from(xdg_data_home).join(certificate)
        }
        Err(_) => {
            home_dir().map(|path| path.join(".local/share").join(certificate)).unwrap()
        }
    };

    let cleo_ca_certificate_dir = certificate_path.parent().unwrap();
    fs::create_dir_all(cleo_ca_certificate_dir)
        .unwrap_or_else(|error| println!("Unable to create path {:?}: {}", certificate_path, error));

    fs::write(
        certificate_path.clone(),
        ca.to_string()
    ).unwrap_or_else(|error| println!(
        "Write CA certificate was not successful at location {:?}: {}", &certificate_path, error
    ));
    Ok(certificate_path)
}