use anyhow::{Context, Result};
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use crate::config::Config;

/// Vector dimension for AllMiniLML6V2 model
pub const EMBEDDING_DIM: usize = 384;

/// Global debug flag
static DEBUG: AtomicBool = AtomicBool::new(false);

/// Set debug mode
pub fn set_debug(enabled: bool) {
    DEBUG.store(enabled, Ordering::Relaxed);
}

/// Global embedding model instance (lazy-loaded, wrapped in Mutex for mutability)
static MODEL: OnceLock<Mutex<TextEmbedding>> = OnceLock::new();

/// Initialize the embedding model (downloads on first use ~80MB)
fn init_model() -> Result<TextEmbedding> {
    let debug = DEBUG.load(Ordering::Relaxed);

    // Use centralized cache directory instead of current working directory
    let cache_dir = Config::model_cache_dir()?;
    std::fs::create_dir_all(&cache_dir)
        .with_context(|| format!("Failed to create cache directory: {}", cache_dir.display()))?;

    TextEmbedding::try_new(
        InitOptions::new(EmbeddingModel::MultilingualE5Small)
            .with_cache_dir(cache_dir)
            .with_show_download_progress(debug),
    )
    .context("Failed to initialize embedding model")
}

/// Generate embedding for a single text
pub fn embed_text(text: &str) -> Result<Vec<f32>> {
    let model_mutex = MODEL.get_or_init(|| {
        Mutex::new(init_model().expect("Failed to initialize embedding model"))
    });

    let mut model = model_mutex
        .lock()
        .map_err(|_| anyhow::anyhow!("Failed to lock embedding model"))?;

    let embeddings = model
        .embed(vec![text], None)
        .context("Failed to generate embedding")?;

    embeddings
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("No embedding generated"))
}

/// Generate embeddings for multiple texts (batch processing)
pub fn embed_texts(texts: &[String]) -> Result<Vec<Vec<f32>>> {
    if texts.is_empty() {
        return Ok(vec![]);
    }

    let model_mutex = MODEL.get_or_init(|| {
        Mutex::new(init_model().expect("Failed to initialize embedding model"))
    });

    let mut model = model_mutex
        .lock()
        .map_err(|_| anyhow::anyhow!("Failed to lock embedding model"))?;

    model
        .embed(texts.to_vec(), None)
        .context("Failed to generate embeddings")
}

