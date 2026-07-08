use thiserror::Error;

#[derive(Debug, Error)]
pub enum DeriverseErrorKind {
    /// code 223
    #[error("Arithmetic overflow")]
    ArithmeticOverflow,

    /// code 336
    #[error("Max number constant overflow")]
    MaxNumberOverflow,
}
