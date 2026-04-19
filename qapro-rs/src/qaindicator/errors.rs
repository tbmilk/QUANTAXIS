use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum IndicatorError {
    #[error("invalid parameter")]
    InvalidParameter,
    #[error("data item is incomplete")]
    DataItemIncomplete,
    #[error("data item is invalid")]
    DataItemInvalid,
}

pub type Result<T> = std::result::Result<T, IndicatorError>;
