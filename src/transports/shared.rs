use futures::sync::oneshot;
use futures::{self, Future};
use std::sync::{self, atomic, Arc};
use std::{fmt, mem, thread};
use transports::tokio::reactor;
use transports::tokio::runtime;
use transports::Result;
use {Error, ErrorKind, RequestId};

/// Event Loop Handle.
/// NOTE: Event loop is stopped when handle is dropped!
#[derive(Debug)]
pub struct EventLoopHandle {
    runtime: runtime::Runtime,
}

impl EventLoopHandle {
    /// Creates a new `EventLoopHandle` and transport given the transport initializer.
    pub fn spawn<T, F>(func: F) -> Result<(Self, T)>
    where
        F: FnOnce(runtime::TaskExecutor) -> Result<T>,
        F: Send + 'static,
        T: Send + 'static,
    {
        let run = move || {
            let event_loop = tokio::runtime::Builder::new().core_threads(1).build().expect("failed to create runtime");
            let http = func(event_loop.executor())?;
            Ok((http, event_loop))
        };

        run().map(|(http, runtime)| {
                (
                    EventLoopHandle {
                        runtime,
                    },
                    http,
                )
            })
    }
}

type PendingResult<O> = oneshot::Receiver<Result<O>>;

enum RequestState<O> {
    Sending(Option<Result<()>>, PendingResult<O>),
    WaitingForResponse(PendingResult<O>),
    Done,
}

/// A future representing a response to a pending request.
pub struct Response<T, O> {
    id: RequestId,
    state: RequestState<O>,
    extract: T,
}

impl<T, O> Response<T, O> {
    /// Creates a new `Response`
    pub fn new(id: RequestId, result: Result<()>, rx: PendingResult<O>, extract: T) -> Self {
        Response {
            id,
            extract,
            state: RequestState::Sending(Some(result), rx),
        }
    }
}

impl<T, O, Out> Future for Response<T, O>
where
    T: Fn(O) -> Result<Out>,
    Out: fmt::Debug,
{
    type Item = Out;
    type Error = Error;

    fn poll(&mut self) -> futures::Poll<Self::Item, Self::Error> {
        loop {
            let extract = &self.extract;
            match self.state {
                RequestState::Sending(ref mut result, _) => {
                    trace!("[{}] Request pending.", self.id);
                    if let Some(Err(e)) = result.take() {
                        return Err(e);
                    }
                }
                RequestState::WaitingForResponse(ref mut rx) => {
                    trace!("[{}] Checking response.", self.id);
                    let result = try_ready!(rx.poll().map_err(|_| Error::from(ErrorKind::Io(
                        ::std::io::ErrorKind::TimedOut.into()
                    ))));
                    trace!("[{}] Extracting result.", self.id);
                    return result.and_then(|x| extract(x)).map(futures::Async::Ready);
                }
                RequestState::Done => {
                    return Err(ErrorKind::Unreachable.into());
                }
            }
            // Proceeed to the next state
            let state = mem::replace(&mut self.state, RequestState::Done);
            self.state = if let RequestState::Sending(_, rx) = state {
                RequestState::WaitingForResponse(rx)
            } else {
                state
            }
        }
    }
}
