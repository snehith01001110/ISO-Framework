use std::path::{Path, PathBuf};

use iso_code::{DefaultAdapter, EcosystemAdapter, ShellCommandAdapter};

/// Adapter configuration as read from `.iso-code.toml` or `config.toml`.
///
/// TOML examples:
/// ```toml
/// [adapter]
/// type = "shell-command"
/// post_create = "npm install"
/// pre_delete  = "npm run cleanup"
/// timeout_ms  = 60000
/// ```
/// ```toml
/// [adapter]
/// type = "default"
/// files_to_copy = [".env", ".env.local"]
/// ```
#[derive(serde::Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum AdapterConfig {
    ShellCommand {
        post_create: Option<String>,
        pre_delete: Option<String>,
        post_delete: Option<String>,
        timeout_ms: Option<u64>,
    },
    Default {
        #[serde(default)]
        files_to_copy: Vec<String>,
    },
}

#[derive(serde::Deserialize, Default)]
pub struct CliConfig {
    pub adapter: Option<AdapterConfig>,
}

/// Load CLI config from the first file found in this order:
/// 1. `<repo_root>/.iso-code.toml`  (project-local, highest priority)
/// 2. `$HOME/.config/iso-code/config.toml`  (user-level)
pub fn load_config(repo_root: &Path) -> CliConfig {
    let local = repo_root.join(".iso-code.toml");
    if local.exists() {
        if let Some(cfg) = read_toml(&local) {
            return cfg;
        }
    }

    if let Some(home) = home_dir() {
        let user = home.join(".config").join("iso-code").join("config.toml");
        if user.exists() {
            if let Some(cfg) = read_toml(&user) {
                return cfg;
            }
        }
    }

    CliConfig::default()
}

/// Instantiate the adapter described by `cfg`.
pub fn build_adapter(cfg: &AdapterConfig) -> Box<dyn EcosystemAdapter> {
    match cfg {
        AdapterConfig::ShellCommand {
            post_create,
            pre_delete,
            post_delete,
            timeout_ms,
        } => {
            let mut a = ShellCommandAdapter::new();
            if let Some(cmd) = post_create {
                a = a.with_post_create(cmd.as_str());
            }
            if let Some(cmd) = pre_delete {
                a = a.with_pre_delete(cmd.as_str());
            }
            if let Some(cmd) = post_delete {
                a = a.with_post_delete(cmd.as_str());
            }
            if let Some(ms) = timeout_ms {
                a = a.with_timeout_ms(*ms);
            }
            Box::new(a)
        }
        AdapterConfig::Default { files_to_copy } => {
            let files = files_to_copy.iter().map(PathBuf::from).collect();
            Box::new(DefaultAdapter::new(files))
        }
    }
}

fn read_toml(path: &Path) -> Option<CliConfig> {
    let content = std::fs::read_to_string(path).ok()?;
    toml::from_str(&content).ok()
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}
