use anyhow::{Context, Result};
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::Duration;

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

/// Spinner frames for loading animation (braille pattern - smooth and modern)
const SPINNER: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Delay before showing spinner (minimal - show almost immediately)
const SPINNER_DELAY_MS: u64 = 10;

/// Start a spinner animation in a background thread (shows after delay)
fn start_spinner(message: &str) -> (Arc<AtomicBool>, thread::JoinHandle<()>) {
    let stop = Arc::new(AtomicBool::new(false));
    let stop_clone = Arc::clone(&stop);
    let msg = message.to_string();

    let handle = thread::spawn(move || {
        // Wait before showing spinner (skip for fast loads)
        thread::sleep(Duration::from_millis(SPINNER_DELAY_MS));

        if stop_clone.load(Ordering::Relaxed) {
            return; // Already done, don't show anything
        }

        let mut i = 0;
        let mut stderr = std::io::stderr();
        let mut showed_spinner = false;

        while !stop_clone.load(Ordering::Relaxed) {
            showed_spinner = true;
            let frame = SPINNER[i % SPINNER.len()];
            // \x1b[2K clears line, \r returns to start
            let _ = write!(stderr, "\r\x1b[2K\x1b[36m{}\x1b[0m {}", frame, msg);
            let _ = stderr.flush();
            i += 1;
            thread::sleep(Duration::from_millis(80));
        }

        // Clear the spinner line only if we showed it
        if showed_spinner {
            let _ = write!(stderr, "\r\x1b[2K");
            let _ = stderr.flush();
        }
    });

    (stop, handle)
}

/// Initialize the embedding model (downloads on first use ~80MB)
fn init_model() -> Result<TextEmbedding> {
    let debug = DEBUG.load(Ordering::Relaxed);

    // Start spinner animation (shows after 300ms delay)
    let (stop, handle) = start_spinner("Loading semantic model...");

    // Use centralized cache directory instead of current working directory
    let cache_dir = Config::model_cache_dir()?;
    std::fs::create_dir_all(&cache_dir)
        .with_context(|| format!("Failed to create cache directory: {}", cache_dir.display()))?;

    let result = TextEmbedding::try_new(
        InitOptions::new(EmbeddingModel::MultilingualE5Small)
            .with_cache_dir(cache_dir)
            .with_show_download_progress(debug),
    )
    .context("Failed to initialize embedding model");

    // Stop spinner
    stop.store(true, Ordering::Relaxed);
    let _ = handle.join();

    result
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

