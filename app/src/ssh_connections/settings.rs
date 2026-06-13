use serde::{Deserialize, Serialize};
use warp_core::settings::macros::define_settings_group;
use warp_core::settings::{SupportedPlatforms, SyncToCloud};

/// A saved SSH connection preset.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, settings_value::SettingsValue)]
#[derive(schemars::JsonSchema)]
pub struct SshConnection {
    pub id: String,
    pub label: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub identity_file: Option<String>,
}

impl Default for SshConnection {
    fn default() -> Self {
        Self {
            id: String::new(),
            label: String::new(),
            host: String::new(),
            port: 22,
            user: String::new(),
            identity_file: None,
        }
    }
}

impl SshConnection {
    /// Builds the SSH command string from this connection.
    pub fn to_command_string(&self) -> String {
        let mut cmd = String::from("ssh");
        if self.port != 22 {
            cmd.push_str(&format!(" -p {}", self.port));
        }
        if let Some(identity) = &self.identity_file {
            cmd.push_str(&format!(" -i '{}'", identity));
        }
        if !self.user.is_empty() {
            cmd.push_str(&format!(" {}@{}", self.user, self.host));
        } else {
            cmd.push_str(&format!(" {}", self.host));
        }
        cmd
    }
}

define_settings_group!(SshConnectionSettings, settings: [
    connections: Connections {
        type: Vec<SshConnection>,
        default: Vec::new(),
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Never,
        private: true,
        toml_path: "ssh_connections.connections",
        description: "Saved SSH connections for quick access.",
    }
]);
