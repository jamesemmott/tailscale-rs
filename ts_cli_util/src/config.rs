use std::{
    fmt::{Debug, Formatter},
    path::Path,
};

/// Serializable tailscale config.
#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Config {
    /// The key state for this node.
    pub key_state: ts_keys::NodeState,

    /// The control server to use.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub control_server_url: Option<url::Url>,

    /// Override for this node's hostname.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub hostname: Option<String>,
}

impl Debug for Config {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("key_state", &self.key_state)
            .field(
                "control_server_url",
                &self.control_server_url.as_ref().map(|x| x.as_str()),
            )
            .field("hostname", &self.hostname)
            .finish()
    }
}

impl Config {
    /// Load the config from the given path. If it's not present, save a default config
    /// into that path (creating parent directories if necessary) and load that.
    #[tracing::instrument(skip_all, fields(path = %path.display()), ret, err)]
    pub async fn load_or_init(path: &Path) -> std::io::Result<Self> {
        let cfg = loop {
            let config = match tokio::fs::read_to_string(path).await {
                Ok(s) => s,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    tracing::info!("config not found, saving default");

                    Config::default().save(path).await?;
                    continue;
                }
                Err(e) => return Err(e),
            };

            let Ok(ret) = serde_json::from_str(&config) else {
                tracing::warn!("failed reading config, saving default over current contents");
                Config::default().save(path).await?;
                continue;
            };

            break ret;
        };

        Ok(cfg)
    }

    /// Save this config to the given `path`, creating parent directories if necessary.
    pub async fn save(&self, path: &Path) -> std::io::Result<()> {
        let s = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;

        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        tokio::fs::write(path, s).await?;
        Ok(())
    }

    /// Convert this config to a control config.
    pub fn control_config(&self) -> ts_control::Config {
        ts_control::Config {
            server_url: self
                .control_server_url
                .clone()
                .unwrap_or_else(|| ts_control::DEFAULT_CONTROL_SERVER.clone()),

            hostname: self
                .hostname
                .clone()
                .or_else(|| gethostname::gethostname().into_string().ok()),

            ..Default::default()
        }
    }
}
