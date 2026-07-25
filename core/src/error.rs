use std::path::PathBuf;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io error on {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("store error: {0}")]
    Store(String),

    #[error("watcher error: {0}")]
    Watcher(#[from] notify::Error),

    #[error("{0}")]
    Invalid(String),
}

impl Error {
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Error::Io {
            path: path.into(),
            source,
        }
    }
}

macro_rules! store_err {
    ($t:ty) => {
        impl From<$t> for Error {
            fn from(e: $t) -> Self {
                Error::Store(e.to_string())
            }
        }
    };
}

store_err!(redb::Error);
store_err!(redb::DatabaseError);
store_err!(redb::TransactionError);
store_err!(redb::TableError);
store_err!(redb::StorageError);
store_err!(redb::CommitError);
store_err!(serde_json::Error);
