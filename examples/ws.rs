#[macro_use]
extern crate serde_json;
extern crate async_jsonrpc_client;
use async_jsonrpc_client::futures::Future;
use async_jsonrpc_client::transports::ws::WebSocket;
use async_jsonrpc_client::Transport;

fn main() {
    // Start server (starts tokio reactor in current thread)
    let (_handle, ws) = WebSocket::new("ws://127.0.0.1:3030").unwrap();

    // Send request
    let res = ws.execute("say_hello", json!({ "name": "Tomasz" }));

    // Wait for response
    let x = res.wait();

    println!("{:?}", x);
}
