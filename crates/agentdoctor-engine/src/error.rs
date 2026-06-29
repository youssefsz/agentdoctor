use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("failed to read {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to inspect {path}: {source}")]
    Metadata {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to scan workspace {path}: {source}")]
    Walk {
        path: PathBuf,
        #[source]
        source: ignore::Error,
    },
    #[error("workspace root does not exist: {0}")]
    MissingRoot(PathBuf),
    #[error("workspace root is not a directory: {0}")]
    RootNotDirectory(PathBuf),
}
