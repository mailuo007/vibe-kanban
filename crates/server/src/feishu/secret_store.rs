use std::path::PathBuf;

use anyhow::Result;
use async_trait::async_trait;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use tokio::{fs, sync::Mutex};
use uuid::Uuid;

#[async_trait]
pub trait FeishuSecretStore: Send + Sync {
    async fn put(&self, value: SecretString) -> Result<String>;
    async fn get(&self, reference: &str) -> Result<SecretString>;
}

#[derive(Debug, Clone)]
pub struct FileFeishuSecretStore {
    path: PathBuf,
    lock: std::sync::Arc<Mutex<()>>,
}

impl FileFeishuSecretStore {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            lock: std::sync::Arc::new(Mutex::new(())),
        }
    }

    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    async fn load_state(&self) -> Result<FeishuSecretFile> {
        match fs::read_to_string(&self.path).await {
            Ok(contents) => Ok(serde_json::from_str(&contents)?),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                Ok(FeishuSecretFile::default())
            }
            Err(err) => Err(err.into()),
        }
    }

    async fn save_state(&self, state: &FeishuSecretFile) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).await?;
        }

        let content = serde_json::to_string_pretty(state)?;
        fs::write(&self.path, format!("{content}\n")).await?;
        Ok(())
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct FeishuSecretFile {
    secrets: std::collections::HashMap<String, String>,
}

#[async_trait]
impl FeishuSecretStore for FileFeishuSecretStore {
    async fn put(&self, value: SecretString) -> Result<String> {
        let _guard = self.lock.lock().await;
        let mut state = self.load_state().await?;
        let reference = format!("feishu_secret_{}", Uuid::new_v4());
        state
            .secrets
            .insert(reference.clone(), value.expose_secret().to_owned());
        self.save_state(&state).await?;
        Ok(reference)
    }

    async fn get(&self, reference: &str) -> Result<SecretString> {
        let _guard = self.lock.lock().await;
        let state = self.load_state().await?;
        let value = state
            .secrets
            .get(reference)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Feishu secret reference not found: {reference}"))?;
        Ok(SecretString::new(value.into()))
    }
}
