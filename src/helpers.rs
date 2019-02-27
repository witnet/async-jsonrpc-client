//! Web3 helpers.

use std::marker::PhantomData;

use futures::{Async, Future, Poll};
use rpc;
use serde;
use serde_json;
use {Error, ErrorKind};

/// Value-decoder future.
/// Takes any type which is deserializable from rpc::Value and a future which yields that
/// type, and yields the deserialized value
#[derive(Debug)]
pub struct CallFuture<T, F> {
    inner: F,
    _marker: PhantomData<T>,
}

impl<T, F> CallFuture<T, F> {
    /// Create a new CallFuture wrapping the inner future.
    pub fn new(inner: F) -> Self {
        CallFuture {
            inner: inner,
            _marker: PhantomData,
        }
    }
}

impl<T: serde::de::DeserializeOwned, F> Future for CallFuture<T, F>
where
    F: Future<Item = rpc::Value, Error = Error>,
{
    type Item = T;
    type Error = Error;

    fn poll(&mut self) -> Poll<T, Error> {
        match self.inner.poll() {
            Ok(Async::Ready(x)) => serde_json::from_value(x)
                .map(Async::Ready)
                .map_err(Into::into),
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Err(e) => Err(e),
        }
    }
}

/// Serialize a type. Panics if the type is returns error during serialization.
pub fn serialize<T: serde::Serialize>(t: &T) -> rpc::Value {
    serde_json::to_value(t).expect("Types never fail to serialize.")
}

/// Serializes a request to string. Panics if the type returns error during serialization.
pub fn to_string<T: serde::Serialize>(request: &T) -> String {
    serde_json::to_string(&request).expect("String serialization never fails.")
}

/// Build a JSON-RPC request.
pub fn build_request(id: usize, method: &str, json_params: rpc::Value) -> rpc::Call {
    let params = match json_params {
        rpc::Value::Null => rpc::Params::None,
        rpc::Value::Bool(..) => (rpc::Params::Array(vec![json_params])),
        rpc::Value::Number(..) => (rpc::Params::Array(vec![json_params])),
        rpc::Value::String(..) => (rpc::Params::Array(vec![json_params])),
        rpc::Value::Array(a) => (rpc::Params::Array(a)),
        rpc::Value::Object(m) => (rpc::Params::Map(m)),
    };
    rpc::Call::MethodCall(rpc::MethodCall {
        jsonrpc: Some(rpc::Version::V2),
        method: method.into(),
        params: params,
        id: rpc::Id::Num(id as u64),
    })
}

/// Parse bytes slice into JSON-RPC response.
pub fn to_response_from_slice(response: &[u8]) -> Result<rpc::Response, Error> {
    serde_json::from_slice(response)
        .map_err(|e| ErrorKind::InvalidResponse(format!("{:?}", e)).into())
}

/// Parse bytes slice into JSON-RPC notification.
pub fn to_notification_from_slice(notification: &[u8]) -> Result<rpc::Notification, Error> {
    serde_json::from_slice(notification)
        .map_err(|e| ErrorKind::InvalidResponse(format!("{:?}", e)).into())
}

/// Parse a Vec of `rpc::Output` into `Result`.
pub fn to_results_from_outputs(
    outputs: Vec<rpc::Output>,
) -> Result<Vec<Result<rpc::Value, Error>>, Error> {
    Ok(outputs.into_iter().map(to_result_from_output).collect())
}

/// Parse `rpc::Output` into `Result`.
pub fn to_result_from_output(output: rpc::Output) -> Result<rpc::Value, Error> {
    match output {
        rpc::Output::Success(success) => Ok(success.result),
        rpc::Output::Failure(failure) => Err(ErrorKind::Rpc(failure.error).into()),
    }
}
