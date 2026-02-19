use anyhow::{anyhow, Result};
use async_trait::async_trait;
use log::{debug, error, trace};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;

use super::embedding;
use crate::state::ModelState;

#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    async fn embed_passages(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>>;
    async fn embed_query(&self, query: &str) -> Result<Vec<f32>>;
    async fn get_dimension(&self) -> Result<usize>;
    fn provider_id(&self) -> String;
}

pub struct LocalProvider {
    pub model_state: Arc<Mutex<ModelState>>,
}

#[async_trait]
impl EmbeddingProvider for LocalProvider {
    async fn embed_passages(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
        let mut guard = self.model_state.lock().await;
        let model = guard
            .model
            .as_mut()
            .ok_or_else(|| anyhow!("Model not loaded"))?;
        embedding::embed_passages(model, texts)
    }

    async fn embed_query(&self, query: &str) -> Result<Vec<f32>> {
        let mut guard = self.model_state.lock().await;
        let model = guard
            .model
            .as_mut()
            .ok_or_else(|| anyhow!("Model not loaded"))?;
        embedding::embed_query(model, query)
    }

    async fn get_dimension(&self) -> Result<usize> {
        let mut guard = self.model_state.lock().await;
        if let Some(dim) = guard.cached_dim {
            return Ok(dim);
        }
        let model = guard
            .model
            .as_mut()
            .ok_or_else(|| anyhow!("Model not loaded"))?;
        let dim = embedding::get_model_dimension(model)?;
        guard.cached_dim = Some(dim);
        Ok(dim)
    }

    fn provider_id(&self) -> String {
        let guard = self.model_state.blocking_lock();
        format!(
            "local:{}",
            guard
                .model
                .as_ref()
                .map(|_| "loaded")
                .unwrap_or("pending")
        )
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RemoteProviderConfig {
    pub endpoint: String,
    pub api_key: Option<String>,
    pub model: String,
    pub dimensions: usize,
}

pub struct RemoteProvider {
    config: RemoteProviderConfig,
    client: reqwest::Client,
}

impl RemoteProvider {
    pub fn new(config: RemoteProviderConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }
}

#[derive(Serialize)]
struct EmbeddingRequest {
    model: String,
    input: Vec<String>,
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
}

#[derive(Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
}

#[async_trait]
impl EmbeddingProvider for RemoteProvider {
    async fn embed_passages(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(vec![]);
        }

        let mut all_embeddings = Vec::with_capacity(texts.len());
        for chunk in texts.chunks(64) {
            debug!("Remote embedding: {} texts to {}", chunk.len(), self.config.endpoint);
            let request = EmbeddingRequest {
                model: self.config.model.clone(),
                input: chunk.to_vec(),
            };

            let mut req = self.client.post(&self.config.endpoint).json(&request);

            if let Some(ref key) = self.config.api_key {
                if !key.is_empty() {
                    req = req.bearer_auth(key);
                }
            }

            let response = req.send().await.map_err(|e| {
                error!("Remote embedding request failed: {}", e);
                anyhow!("Remote embedding request failed: {}", e)
            })?;

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                error!("Remote embedding API returned {}: {}", status, body);
                return Err(anyhow!(
                    "Remote embedding API returned {}: {}",
                    status,
                    body
                ));
            }

            let resp: EmbeddingResponse = response.json().await.map_err(|e| {
                anyhow!("Failed to parse embedding response: {}", e)
            })?;

            for data in resp.data {
                all_embeddings.push(data.embedding);
            }
        }

        trace!("Remote embedding: got {} embeddings total", all_embeddings.len());
        Ok(all_embeddings)
    }

    async fn embed_query(&self, query: &str) -> Result<Vec<f32>> {
        let results = self.embed_passages(vec![query.to_string()]).await?;
        results
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("Empty embedding result from remote provider"))
    }

    async fn get_dimension(&self) -> Result<usize> {
        Ok(self.config.dimensions)
    }

    fn provider_id(&self) -> String {
        format!("remote:{}:{}", self.config.endpoint, self.config.model)
    }
}
