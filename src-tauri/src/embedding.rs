use anyhow::{Context, Result};
use fastembed::{EmbeddingModel, TextEmbedding, TextInitOptions};
use parking_lot::Mutex;
use std::{path::Path, sync::Arc};

#[derive(Clone)]
pub struct LocalEmbedder {
    model: Arc<Mutex<Option<TextEmbedding>>>,
    cache_dir: Arc<std::path::PathBuf>,
}

impl LocalEmbedder {
    pub fn new(cache_dir: &Path) -> Self {
        Self {
            model: Arc::new(Mutex::new(None)),
            cache_dir: Arc::new(cache_dir.to_path_buf()),
        }
    }

    pub fn embed_passages(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let inputs = texts
            .iter()
            .map(|text| format!("passage: {text}"))
            .collect::<Vec<_>>();
        self.embed(inputs)
    }

    pub fn embed_query(&self, query: &str) -> Result<Vec<f32>> {
        self.embed(vec![format!("query: {query}")])?
            .into_iter()
            .next()
            .context("embedding model returned no query vector")
    }

    fn embed(&self, inputs: Vec<String>) -> Result<Vec<Vec<f32>>> {
        let mut model_guard = self.model.lock();
        if model_guard.is_none() {
            std::fs::create_dir_all(self.cache_dir.as_path())?;
            let options = TextInitOptions::new(EmbeddingModel::MultilingualE5Small)
                .with_cache_dir(self.cache_dir.as_ref().clone())
                .with_show_download_progress(true);
            *model_guard = Some(
                TextEmbedding::try_new(options)
                    .context("failed to initialize the local multilingual embedding model")?,
            );
        }
        model_guard
            .as_mut()
            .expect("embedding model initialized")
            .embed(inputs, Some(32))
            .context("local embedding failed")
    }
}
