//! Ethereum JSON-RPC client (Web3).

pub extern crate jsonrpc_core as rpc;
extern crate parking_lot;
extern crate serde;

#[cfg_attr(test, macro_use)]
extern crate serde_json;

#[macro_use]
extern crate error_chain;
#[macro_use]
extern crate log;

/// Re-export of the `futures` crate.
#[macro_use]
pub extern crate futures;

mod api;
mod error;
mod helpers;

#[cfg(any(feature = "http", feature = "tcp", feature = "ws"))]
pub mod transports;

pub use crate::error::{Error, ErrorKind};

/// RPC result
pub type Result<T> = Box<dyn futures::Future<Item = T, Error = Error> + Send + 'static>;

/// Assigned RequestId
pub type RequestId = usize;

/// Transport implementation
pub trait Transport: ::std::fmt::Debug + Clone {
    /// The type of future this transport returns when a call is made.
    type Out: futures::Future<Item = rpc::Value, Error = Error>;

    /// Prepare serializable RPC call for given method with parameters.
    fn prepare(&self, method: &str, params: rpc::Value) -> (RequestId, rpc::Call);

    /// Execute prepared RPC call.
    fn send(&self, id: RequestId, request: rpc::Call) -> Self::Out;

    /// Execute remote method with given parameters.
    fn execute(&self, method: &str, params: rpc::Value) -> Self::Out {
        let (id, request) = self.prepare(method, params);
        self.send(id, request)
    }
}

/// A transport implementation supporting batch requests.
pub trait BatchTransport: Transport {
    /// The type of future this transport returns when a call is made.
    type Batch: futures::Future<Item = Vec<::std::result::Result<rpc::Value, Error>>, Error = Error>;

    /// Sends a batch of prepared RPC calls.
    fn send_batch<T>(&self, requests: T) -> Self::Batch
    where
        T: IntoIterator<Item = (RequestId, rpc::Call)>;

    // Execute remote method with given parameters.
    fn execute_batch<'a, T>(&'a self, x: T) -> Self::Batch
    where
        T: IntoIterator<Item = (&'a str, rpc::Value)>,
    {
        let requests = x
            .into_iter()
            .map(|(method, params)| self.prepare(method, params));
        self.send_batch(requests)
    }
}

/// A transport implementation supporting pub sub subscriptions.
pub trait DuplexTransport: Transport {
    /// The type of stream this transport returns
    type NotificationStream: futures::Stream<Item = rpc::Value, Error = Error>;

    /// Add a subscription to this transport
    fn subscribe(&self, id: &api::SubscriptionId) -> Self::NotificationStream;

    /// Remove a subscription from this transport
    fn unsubscribe(&self, id: &api::SubscriptionId);
}

impl<X, T> Transport for X
where
    T: Transport + ?Sized,
    X: ::std::ops::Deref<Target = T>,
    X: ::std::fmt::Debug,
    X: Clone,
{
    type Out = T::Out;

    fn prepare(&self, method: &str, params: rpc::Value) -> (RequestId, rpc::Call) {
        (**self).prepare(method, params)
    }

    fn send(&self, id: RequestId, request: rpc::Call) -> Self::Out {
        (**self).send(id, request)
    }
}

impl<X, T> BatchTransport for X
where
    T: BatchTransport + ?Sized,
    X: ::std::ops::Deref<Target = T>,
    X: ::std::fmt::Debug,
    X: Clone,
{
    type Batch = T::Batch;

    fn send_batch<I>(&self, requests: I) -> Self::Batch
    where
        I: IntoIterator<Item = (RequestId, rpc::Call)>,
    {
        (**self).send_batch(requests)
    }
}

impl<X, T> DuplexTransport for X
where
    T: DuplexTransport + ?Sized,
    X: ::std::ops::Deref<Target = T>,
    X: ::std::fmt::Debug,
    X: Clone,
{
    type NotificationStream = T::NotificationStream;

    fn subscribe(&self, id: &api::SubscriptionId) -> Self::NotificationStream {
        (**self).subscribe(id)
    }

    fn unsubscribe(&self, id: &api::SubscriptionId) {
        (**self).unsubscribe(id)
    }
}
