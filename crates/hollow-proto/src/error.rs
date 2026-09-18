use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProtoError {
    #[error("buffer too short: need {need} bytes, have {have}")]
    Short { need: usize, have: usize },
    #[error("malformed protobuf: {0}")]
    Wire(&'static str),
    #[error("rsa: {0}")]
    Rsa(String),
    #[error("data file: {0}")]
    Data(String),
    #[error("fixture: {0}")]
    Fixture(String),
    #[error("unknown region {0:?}")]
    UnknownRegion(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, ProtoError>;
