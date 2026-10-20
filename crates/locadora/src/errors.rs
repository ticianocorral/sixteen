use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("io falhou: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialização: {0}")]
    Serde(#[from] serde_json::Error),
}
