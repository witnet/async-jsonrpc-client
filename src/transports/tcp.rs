extern crate tokio_codec;
/// TCP Socket Transport
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::{atomic, Arc};

use self::tokio_codec::{Framed, LinesCodec};
use api::SubscriptionId;
use futures::sync::{mpsc, oneshot};
use futures::{Future, Sink, Stream};
use helpers;
use parking_lot::Mutex;
use transports::shared::{EventLoopHandle, Response};
use transports::tokio_core::net::TcpStream;
use transports::tokio_core::reactor;
use transports::Result;
use {BatchTransport, DuplexTransport, Error, ErrorKind, RequestId, Transport};

impl From<std::net::AddrParseError> for Error {
    fn from(err: std::net::AddrParseError) -> Self {
        ErrorKind::Transport(format!("{:?}", err)).into()
    }
}

type Pending = oneshot::Sender<Result<Vec<Result<rpc::Value>>>>;

type Subscription = mpsc::UnboundedSender<rpc::Value>;

/// A future representing pending WebSocket request, resolves to a response.
pub type WsTask<F> = Response<F, Vec<Result<rpc::Value>>>;

/// WebSocket transport
#[derive(Debug, Clone)]
pub struct TcpSocket {
    id: Arc<atomic::AtomicUsize>,
    addr: SocketAddr,
    pending: Arc<Mutex<BTreeMap<RequestId, Pending>>>,
    subscriptions: Arc<Mutex<BTreeMap<SubscriptionId, Subscription>>>,
    write_sender: mpsc::UnboundedSender<String>,
}

impl TcpSocket {
    /// Create new WebSocket transport with separate event loop.
    /// NOTE: Dropping event loop handle will stop the transport layer!
    pub fn new(url: &str) -> Result<(EventLoopHandle, Self)> {
        let url = url.to_owned();
        EventLoopHandle::spawn(move |handle| {
            Self::with_event_loop(&url, &handle).map_err(Into::into)
        })
    }

    /// Create new WebSocket transport within existing Event Loop.
    pub fn with_event_loop(addr: &str, handle: &reactor::Handle) -> Result<Self> {
        trace!("Connecting to: {:?}", addr);

        let addr: SocketAddr = addr.parse()?;
        let pending: Arc<Mutex<BTreeMap<RequestId, Pending>>> = Default::default();
        let subscriptions: Arc<Mutex<BTreeMap<SubscriptionId, Subscription>>> = Default::default();
        let (write_sender, write_receiver) = mpsc::unbounded();

        let ws_future =
            {
                let pending_ = pending.clone();
                let subscriptions_ = subscriptions.clone();

                TcpStream::connect(&addr, handle)
                    .from_err::<Error>()
                    .map(|stream| Framed::new(stream, LinesCodec::new()).split())
                    .and_then(move |(sink, stream)| {
                        let reader = stream.from_err::<Error>().for_each(move |message| {
						trace!("Message received: {}", message);

						if let Ok(notification) = helpers::to_notification_from_slice(message.as_bytes()) {
							if let rpc::Params::Map(params) = notification.params {
								let id = params.get("subscription");
								let result = params.get("result");

								if let (Some(&rpc::Value::String(ref id)), Some(result)) = (id, result) {
									let id: SubscriptionId = id.clone().into();
									if let Some(stream) = subscriptions_.lock().get(&id) {
										return stream
											.unbounded_send(result.clone())
											.map_err(|_| ErrorKind::Transport("Error sending notification".into()).into());
									} else {
										warn!("Got notification for unknown subscription (id: {:?})", id);
									}
								} else {
									error!("Got unsupported notification (id: {:?})", id);
								}
							}

							return Ok(());
						}

						let response = helpers::to_response_from_slice(message.as_bytes());
						let outputs = match response {
							Ok(rpc::Response::Single(output)) => vec![output],
							Ok(rpc::Response::Batch(outputs)) => outputs,
							_ => vec![],
						};

						let id = match outputs.get(0) {
							Some(&rpc::Output::Success(ref success)) => success.id.clone(),
							Some(&rpc::Output::Failure(ref failure)) => failure.id.clone(),
							None => rpc::Id::Num(0),
						};

						if let rpc::Id::Num(num) = id {
							if let Some(request) = pending_.lock().remove(&(num as usize)) {
								trace!("Responding to (id: {:?}) with {:?}", num, outputs);
								if let Err(err) = request.send(helpers::to_results_from_outputs(outputs)) {
									warn!("Sending a response to deallocated channel: {:?}", err);
								}
							} else {
								warn!("Got response for unknown request (id: {:?})", num);
							}
						} else {
							warn!("Got unsupported response (id: {:?})", id);
						}

						Ok(())
					});

                        let writer = sink
                            .sink_from_err()
                            .send_all(write_receiver.map_err(|()| {
                                Error::from(ErrorKind::Transport("No data available".into()))
                            }))
                            .map(|_| ());

                        reader.join(writer)
                    })
            };

        handle.spawn(ws_future.map(|_| ()).map_err(|err| {
            error!("WebSocketError: {:?}", err);
        }));

        Ok(Self {
            id: Arc::new(atomic::AtomicUsize::new(1)),
            addr,
            pending,
            subscriptions,
            write_sender,
        })
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    fn send_request<F, O>(&self, id: RequestId, request: rpc::Request, extract: F) -> WsTask<F>
    where
        F: Fn(Vec<Result<rpc::Value>>) -> O,
    {
        let request = helpers::to_string(&request);
        debug!("[{}] Calling: {}", id, request);
        let (tx, rx) = futures::oneshot();
        self.pending.lock().insert(id, tx);

        let result = self
            .write_sender
            .unbounded_send(request)
            .map_err(|_| ErrorKind::Transport("Error sending request".into()).into());

        Response::new(id, result, rx, extract)
    }
}

impl Transport for TcpSocket {
    type Out = WsTask<fn(Vec<Result<rpc::Value>>) -> Result<rpc::Value>>;

    fn prepare(&self, method: &str, params: rpc::Value) -> (RequestId, rpc::Call) {
        let id = self.id.fetch_add(1, atomic::Ordering::AcqRel);
        let request = helpers::build_request(id, method, params);

        (id, request)
    }

    fn send(&self, id: RequestId, request: rpc::Call) -> Self::Out {
        self.send_request(id, rpc::Request::Single(request), |response| match response
            .into_iter()
            .next()
        {
            Some(res) => res,
            None => Err(ErrorKind::InvalidResponse("Expected single, got batch.".into()).into()),
        })
    }
}

impl BatchTransport for TcpSocket {
    type Batch = WsTask<fn(Vec<Result<rpc::Value>>) -> Result<Vec<Result<rpc::Value>>>>;

    fn send_batch<T>(&self, requests: T) -> Self::Batch
    where
        T: IntoIterator<Item = (RequestId, rpc::Call)>,
    {
        let mut it = requests.into_iter();
        let (id, first) = it
            .next()
            .map(|x| (x.0, Some(x.1)))
            .unwrap_or_else(|| (0, None));
        let requests = first.into_iter().chain(it.map(|x| x.1)).collect();
        self.send_request(id, rpc::Request::Batch(requests), Ok)
    }
}

impl DuplexTransport for TcpSocket {
    type NotificationStream = Box<Stream<Item = rpc::Value, Error = Error> + Send + 'static>;

    fn subscribe(&self, id: &SubscriptionId) -> Self::NotificationStream {
        let (tx, rx) = mpsc::unbounded();
        if self.subscriptions.lock().insert(id.clone(), tx).is_some() {
            warn!("Replacing already-registered subscription with id {:?}", id)
        }
        Box::new(rx.map_err(|()| ErrorKind::Transport("No data available".into()).into()))
    }

    fn unsubscribe(&self, id: &SubscriptionId) {
        self.subscriptions.lock().remove(id);
    }
}
