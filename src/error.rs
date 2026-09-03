use miette::{Diagnostic, SourceSpan};
use thiserror::Error;

#[derive(Error, Diagnostic, Debug)]
pub enum DataPipeError {
    #[error("Syntax error in expression")]
    #[diagnostic(
        code(datapipe::expr::syntax),
        help("Check the supported operators: ==, !=, >, <, >=, <=, &&, ||")
    )]
    SyntaxError {
        #[source_code]
        src: String,
        #[label("{msg}")]
        span: SourceSpan,
        msg: String,
    },

    #[error(transparent)]
    #[diagnostic(code(datapipe::io))]
    IoError(#[from] std::io::Error),
    
    #[error(transparent)]
    #[diagnostic(code(datapipe::json))]
    JsonError(#[from] serde_json::Error),
    
    #[error("General Error: {0}")]
    General(String),
}

impl From<anyhow::Error> for DataPipeError {
    fn from(err: anyhow::Error) -> Self {
        DataPipeError::General(err.to_string())
    }
}
