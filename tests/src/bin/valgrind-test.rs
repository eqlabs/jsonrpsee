// Copyright 2022 Parity Technologies (UK) Ltd.
//
// Permission is hereby granted, free of charge, to any
// person obtaining a copy of this software and associated
// documentation files (the "Software"), to deal in the
// Software without restriction, including without
// limitation the rights to use, copy, modify, merge,
// publish, distribute, sublicense, and/or sell copies of
// the Software, and to permit persons to whom the Software
// is furnished to do so, subject to the following
// conditions:
//
// The above copyright notice and this permission notice
// shall be included in all copies or substantial portions
// of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
// ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
// TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
// PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
// SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
// CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
// OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
// IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
// DEALINGS IN THE SOFTWARE.

use std::net::SocketAddr;
use std::time::Duration;

use futures::future::{self, FutureExt};
use futures::stream::StreamExt;
use jsonrpsee::core::client::{ClientT, SubscriptionClientT};
use jsonrpsee::rpc_params;
use jsonrpsee::ws_client::WsClientBuilder;
use jsonrpsee::ws_server::{RpcModule, WsServerBuilder, WsServerHandle};
use tokio::task::block_in_place;
use tokio::time::interval;
use tokio_stream::wrappers::IntervalStream;

fn main() {
	let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();

	let (addr, handle) = rt.block_on(run_server());
	let url = format!("ws://{}", addr);
	let mut futs = Vec::new();

	for idx in 0..100 {
		let url = url.clone();
		futs.push(rt.spawn(async move {
			let client = WsClientBuilder::default().build(&url).await.unwrap();
			let _: String = client.request("say_hello", rpc_params![]).await.unwrap();
			let _: String = client.request("say_hello_async", rpc_params![]).await.unwrap();
			let _: String = client.request("blocking", rpc_params![]).await.unwrap();

			let mut sub = client.subscribe("hi", rpc_params![], "goodbye").await.unwrap();
			let _: usize = sub.next().await.unwrap().unwrap();

			if idx % 2 == 0 {
				drop(client);
				tokio::time::sleep(std::time::Duration::from_micros(100)).await;
				drop(sub);
			} else {
				sub.unsubscribe().await.unwrap();
				drop(client);
			}
		}));
	}

	for res in rt.block_on(future::join_all(futs)) {
		assert!(res.is_ok());
	}

	rt.block_on(handle.stop().unwrap());

	drop(rt);
}

async fn run_server() -> (SocketAddr, WsServerHandle) {
	let server = WsServerBuilder::default().build("127.0.0.1:0").await.unwrap();
	let mut module = RpcModule::new(());

	module.register_method("say_hello", |_, _| Ok("lo")).unwrap();
	module.register_async_method("say_hello_async", |_, _| async { Ok("lo_async") }).unwrap();
	module
		.register_blocking_method("blocking", |_, _| {
			std::thread::sleep(std::time::Duration::from_micros(100));
			Ok("blocking")
		})
		.unwrap();
	module
		.register_subscription("hi", "hi", "goodbye", |_, mut sink, _| {
			let interval = interval(Duration::from_millis(200));
			let stream = IntervalStream::new(interval).map(move |_| 1);

			tokio::spawn(async move {
				sink.pipe_from_stream(stream).await;
			});

			Ok(())
		})
		.unwrap();

	let addr = server.local_addr().unwrap();
	let handle = server.start(module).unwrap();
	(addr, handle)
}
