use std::fmt::{Display, Formatter};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticEmbedderState {
    Ready {
        provider: &'static str,
        model_id: String,
    },
    Degraded {
        provider: &'static str,
        reason: String,
    },
}

impl SemanticEmbedderState {
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticEmbedderError {
    reason: String,
}

impl SemanticEmbedderError {
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }

    pub fn reason(&self) -> &str {
        &self.reason
    }
}

impl Display for SemanticEmbedderError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.reason)
    }
}

impl std::error::Error for SemanticEmbedderError {}

pub type SemanticEmbedderResult<T> = std::result::Result<T, SemanticEmbedderError>;

pub trait SemanticEmbedder {
    fn state(&self) -> SemanticEmbedderState;
    fn embed_texts(&mut self, texts: &[String]) -> SemanticEmbedderResult<Vec<Vec<f32>>>;
}

#[derive(Debug, Clone)]
pub struct UnavailableSemanticEmbedder {
    provider: &'static str,
    reason: String,
}

impl UnavailableSemanticEmbedder {
    pub fn new(provider: &'static str, reason: impl Into<String>) -> Self {
        Self {
            provider,
            reason: reason.into(),
        }
    }

    pub fn fastembed_feature_disabled() -> Self {
        Self::new(
            "fastembed",
            "FastEmbed adapter unavailable: compile with roger-storage feature `semantic-fastembed`",
        )
    }
}

impl SemanticEmbedder for UnavailableSemanticEmbedder {
    fn state(&self) -> SemanticEmbedderState {
        SemanticEmbedderState::Degraded {
            provider: self.provider,
            reason: self.reason.clone(),
        }
    }

    fn embed_texts(&mut self, _texts: &[String]) -> SemanticEmbedderResult<Vec<Vec<f32>>> {
        Err(SemanticEmbedderError::new(self.reason.clone()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticEmbedderStatus {
    pub available: bool,
    pub backend: Option<String>,
    pub reason: Option<String>,
}

pub fn semantic_embedder_status() -> SemanticEmbedderStatus {
    #[cfg(feature = "semantic-fastembed")]
    {
        SemanticEmbedderStatus {
            available: true,
            backend: Some("fastembed".to_owned()),
            reason: None,
        }
    }

    #[cfg(not(feature = "semantic-fastembed"))]
    {
        SemanticEmbedderStatus {
            available: false,
            backend: None,
            reason: Some(
                "semantic-fastembed feature is disabled; compile roger-storage with `semantic-fastembed` to enable FastEmbed adapter".to_owned(),
            ),
        }
    }
}

/// True when the build was compiled with the `semantic-fastembed` feature, i.e.
/// the FastEmbed adapter is statically linked and *could* be installed. This is
/// distinct from "operational": a compiled build still needs `rr assets install`
/// to download and probe the real model before search can flip to hybrid.
pub const fn semantic_embedder_compiled() -> bool {
    cfg!(feature = "semantic-fastembed")
}

/// Result of constructing the real semantic embedder and embedding a probe
/// string. Returned by [`probe_semantic_embedder`] so callers (notably
/// `rr assets install`) can prove the model loads and runs inference before
/// recording it as operational.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticEmbedderProbe {
    pub backend: String,
    pub model_id: String,
    pub dimension: usize,
}

/// Construct the real FastEmbed embedder against `config` (triggering the
/// HuggingFace model download into `config.cache_dir` on first use), embed a
/// fixed probe string, and report the loaded model's identity + embedding
/// dimension. Fails closed with an honest reason when the feature is not
/// compiled or the model cannot be loaded/run.
pub fn probe_semantic_embedder(
    config: FastEmbedAdapterConfig,
) -> SemanticEmbedderResult<SemanticEmbedderProbe> {
    let model_id = config.model.model_id().to_owned();
    let mut adapter = SemanticEmbedderAdapter::try_fastembed(config)?;
    if !adapter.state().is_ready() {
        return Err(SemanticEmbedderError::new(
            "semantic embedder constructed but did not report a ready state",
        ));
    }
    let probe = adapter.embed_texts(&["roger semantic embedder install probe".to_owned()])?;
    let dimension = probe
        .first()
        .map(Vec::len)
        .filter(|len| *len > 0)
        .ok_or_else(|| {
            SemanticEmbedderError::new("semantic embedder returned an empty probe embedding")
        })?;
    Ok(SemanticEmbedderProbe {
        backend: "fastembed".to_owned(),
        model_id,
        dimension,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FastEmbedModel {
    AllMiniLML6V2,
}

impl FastEmbedModel {
    pub fn model_id(self) -> &'static str {
        match self {
            Self::AllMiniLML6V2 => "all-minilm-l6-v2",
        }
    }

    #[cfg(feature = "semantic-fastembed")]
    fn as_fastembed(self) -> fastembed::EmbeddingModel {
        match self {
            Self::AllMiniLML6V2 => fastembed::EmbeddingModel::AllMiniLML6V2,
        }
    }
}

impl Default for FastEmbedModel {
    fn default() -> Self {
        Self::AllMiniLML6V2
    }
}

#[derive(Debug, Clone)]
pub struct FastEmbedAdapterConfig {
    pub model: FastEmbedModel,
    pub cache_dir: PathBuf,
    pub show_download_progress: bool,
}

impl Default for FastEmbedAdapterConfig {
    fn default() -> Self {
        Self {
            model: FastEmbedModel::default(),
            cache_dir: std::env::temp_dir().join("roger-fastembed-cache"),
            show_download_progress: false,
        }
    }
}

pub enum SemanticEmbedderAdapter {
    Unavailable(UnavailableSemanticEmbedder),
    #[cfg(feature = "semantic-fastembed")]
    FastEmbed(FastEmbedSemanticEmbedder),
}

impl SemanticEmbedderAdapter {
    pub fn default_for_runtime() -> Self {
        #[cfg(feature = "semantic-fastembed")]
        {
            Self::Unavailable(UnavailableSemanticEmbedder::new(
                "fastembed",
                "FastEmbed adapter is not initialized for this runtime",
            ))
        }
        #[cfg(not(feature = "semantic-fastembed"))]
        {
            Self::Unavailable(UnavailableSemanticEmbedder::fastembed_feature_disabled())
        }
    }

    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self::Unavailable(UnavailableSemanticEmbedder::new("semantic", reason))
    }

    pub fn try_fastembed(config: FastEmbedAdapterConfig) -> SemanticEmbedderResult<Self> {
        #[cfg(feature = "semantic-fastembed")]
        {
            FastEmbedSemanticEmbedder::try_new(config).map(Self::FastEmbed)
        }
        #[cfg(not(feature = "semantic-fastembed"))]
        {
            let _ = config;
            Err(SemanticEmbedderError::new(
                "FastEmbed adapter unavailable: compile with roger-storage feature `semantic-fastembed`",
            ))
        }
    }
}

impl SemanticEmbedder for SemanticEmbedderAdapter {
    fn state(&self) -> SemanticEmbedderState {
        match self {
            Self::Unavailable(adapter) => adapter.state(),
            #[cfg(feature = "semantic-fastembed")]
            Self::FastEmbed(adapter) => adapter.state(),
        }
    }

    fn embed_texts(&mut self, texts: &[String]) -> SemanticEmbedderResult<Vec<Vec<f32>>> {
        match self {
            Self::Unavailable(adapter) => adapter.embed_texts(texts),
            #[cfg(feature = "semantic-fastembed")]
            Self::FastEmbed(adapter) => adapter.embed_texts(texts),
        }
    }
}

#[cfg(feature = "semantic-fastembed")]
pub struct FastEmbedSemanticEmbedder {
    model_id: String,
    model: fastembed::TextEmbedding,
}

#[cfg(feature = "semantic-fastembed")]
impl FastEmbedSemanticEmbedder {
    pub fn try_new(config: FastEmbedAdapterConfig) -> SemanticEmbedderResult<Self> {
        let options = fastembed::TextInitOptions::new(config.model.as_fastembed())
            .with_cache_dir(config.cache_dir)
            .with_show_download_progress(config.show_download_progress);

        let model = fastembed::TextEmbedding::try_new(options).map_err(|err| {
            SemanticEmbedderError::new(format!("FastEmbed initialization failed: {err}"))
        })?;

        Ok(Self {
            model_id: config.model.model_id().to_owned(),
            model,
        })
    }
}

#[cfg(feature = "semantic-fastembed")]
impl SemanticEmbedder for FastEmbedSemanticEmbedder {
    fn state(&self) -> SemanticEmbedderState {
        SemanticEmbedderState::Ready {
            provider: "fastembed",
            model_id: self.model_id.clone(),
        }
    }

    fn embed_texts(&mut self, texts: &[String]) -> SemanticEmbedderResult<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        self.model.embed(texts.to_vec(), None).map_err(|err| {
            SemanticEmbedderError::new(format!("FastEmbed embedding request failed: {err}"))
        })
    }
}
