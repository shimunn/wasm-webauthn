use std::string::FromUtf8Error;

use coset::CoseError;
use thiserror::Error;
use wasm_bindgen::JsValue;
use web_sys::console;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    Serialize(#[from] serde_wasm_bindgen::Error),
    #[error("{0}")]
    Deserialize(#[from] ciborium::de::Error<std::io::Error>),
    #[error("failed to access context")]
    ContextUnavailable,
    #[error("{0}")]
    WebSys(String),
    #[error("{0}")]
    Cose(#[from] CoseError),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Utf8(#[from] FromUtf8Error),
}

impl From<JsValue> for Error {
    fn from(value: JsValue) -> Self {
        if let Some(message) = value.as_string() {
            Self::WebSys(message)
        } else {
            console::log_1(&value);
            Self::WebSys("Unknown".into())
        }
    }
}
