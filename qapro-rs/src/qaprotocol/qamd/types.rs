use std::fmt;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OptionalNumeric<T> {
    Value(T),
    String(String),
    #[serde(rename = "null")]
    Null,
}

impl<T: Default> Default for OptionalNumeric<T> {
    fn default() -> Self {
        OptionalNumeric::Null
    }
}

impl<T: fmt::Display> fmt::Display for OptionalNumeric<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            OptionalNumeric::Value(v) => write!(f, "{}", v),
            OptionalNumeric::String(s) => write!(f, "{}", s),
            OptionalNumeric::Null => write!(f, "null"),
        }
    }
}

pub type OptionalF64 = OptionalNumeric<f64>;
pub type OptionalI64 = OptionalNumeric<i64>;
