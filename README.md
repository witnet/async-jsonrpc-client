# async-jsonrpc-client

Event-driven JSON-RPC client.

# Introduction

This is a fork of the JSON-RPC client implementation from [web3](https://github.com/tomusdrw/rust-web3).

Supported transports:

* HTTP
* WebSockets
* TCP

# Usage

First, add the dependency in `Cargo.toml`.
The required transports must be explicitely enabled as features:

```
async-jsonrpc-client = { version = "0.1.0", features = "ws" }
```

The available features are:

```
["http", "tcp", "ws", "tls"]
```

Where `"tls"` enables support for `https` and `wss`.

# Example

A WebSockets client which sends a request and waits for the response:

```rust
use serde_json::json;
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
```

# See also

* [Parity JSON-RPC server](https://github.com/paritytech/jsonrpc)
* [Web3](https://github.com/tomusdrw/rust-web3)

