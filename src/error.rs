use std::io;
use std::sync::Arc;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    // UTILITIES
    #[error("{wrapped}")]
    Wrapped {
        #[from]
        wrapped: WrappedError,
    },

    // INDIVIDUAL ERRORS
    #[error("error logging in: {message}")]
    Login { message: String },

    #[error("reading config: {source}")]
    Figment {
        #[from]
        source: figment::Error,
    },

    #[error("invalid file extension: {ext}")]
    InvalidFileExtension { ext: String },

    #[error("i/o: {source}")]
    IO {
        #[from]
        source: io::Error,
    },

    #[error("nothing supplied to stdin")]
    NoStdin,

    #[error("found no tasks")]
    NoTasks,

    #[error("not found: {what}")]
    NotFound { what: String },

    #[error("'{arg}' not supported for '{command}'")]
    NotSupported { arg: String, command: String },

    #[error("failed to parse")]
    Parse,

    #[error("failed to parse time: {source}")]
    ParseTime {
        #[from]
        source: chrono::ParseError,
    },

    #[error("oauth2: {source}")]
    OAuth2 {
        #[from]
        source: yup_oauth2::Error,
    },

    #[error("failed to parse JSON: {source}")]
    SerdeJSON {
        #[from]
        source: serde_json::Error,
    },

    #[error("google tasks api: {source}")]
    TasksAPI {
        #[from]
        source: google_tasks1::Error,
    },

    #[error("setting tree-sitter language: {source}")]
    TreeSitterLanguage {
        #[from]
        source: tree_sitter::LanguageError,
    },

    #[error("creating tree-sitter query: {source}")]
    TreeSitterQuery {
        #[from]
        source: tree_sitter::QueryError,
    },

    #[error("parsing utf8: {source}")]
    Utf8 {
        #[from]
        source: std::str::Utf8Error,
    },
}

#[derive(Debug, Error)]
#[error("while {context}: {what}")]
pub struct WrappedError {
    context: Arc<str>,
    what: Box<Error>,
}

pub trait WrapError {
    type OkT;

    fn during(self, context: &str) -> Result<Self::OkT, WrappedError>;

    fn during_f<F: FnOnce() -> Arc<str>>(self, context_f: F) -> Result<Self::OkT, WrappedError>;
}

impl<T, E> WrapError for Result<T, E>
where
    E: Into<Error>,
{
    type OkT = T;

    fn during(self, context: &str) -> Result<Self::OkT, WrappedError> {
        self.map_err(|err| WrappedError {
            context: context.into(),
            what: Box::new(err.into()),
        })
    }

    // TODO: Improve to be actually useful
    fn during_f<F: FnOnce() -> Arc<str>>(self, context_f: F) -> Result<Self::OkT, WrappedError> {
        self.map_err(|err| WrappedError {
            context: context_f(),
            what: Box::new(err.into()),
        })
    }
}

pub fn handle_load_error(path: &camino::Utf8Path, err: io::Error) -> Error {
    if err.kind() == io::ErrorKind::NotFound {
        Error::NotFound {
            what: path.to_string(),
        }
    } else {
        err.into()
    }
}
