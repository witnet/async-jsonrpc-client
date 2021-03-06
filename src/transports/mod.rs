//! Supported Ethereum JSON-RPC transports.

use crate::Error;

/// RPC Result.
pub type Result<T> = ::std::result::Result<T, Error>;

extern crate tokio;

pub mod shared;

#[cfg(feature = "http")]
pub mod http;

#[cfg(feature = "tcp")]
pub mod tcp;
