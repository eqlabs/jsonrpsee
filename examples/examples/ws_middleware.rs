// Copyright 2019-2021 Parity Technologies (UK) Ltd.
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

//! This example sets a custom tower service middleware to the RPC implementation.

use hyper::http::HeaderValue;
use hyper::Method;
use std::iter::once;
use std::net::SocketAddr;
use std::time::Duration;
use tower_http::cors::CorsLayer;
use tower_http::sensitive_headers::SetSensitiveRequestHeadersLayer;
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};
use tower_http::LatencyUnit;

use jsonrpsee::core::client::ClientT;
use jsonrpsee::server::{RpcModule, ServerBuilder, ServerHandle};
use jsonrpsee::ws_client::WsClientBuilder;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	tracing_subscriber::FmtSubscriber::builder()
		.with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
		.try_init()
		.expect("setting default subscriber failed");

	let (addr, _handler) = run_server().await?;
	let url = format!("ws://{}", addr);

	let client = WsClientBuilder::default().build(&url).await?;
	let response: String = client.request("say_hello", None).await?;
	println!("[main]: response: {:?}", response);
	let _response: Result<String, _> = client.request("unknown_method", None).await;
	let _ = client.request::<String>("say_hello", None).await?;

	Ok(())
}

async fn run_server() -> anyhow::Result<(SocketAddr, ServerHandle)> {
	let cors = CorsLayer::new()
		// Allow `POST` when accessing the resource
		.allow_methods([Method::POST])
		// Allow requests from any origin
		.allow_origin(HeaderValue::from_str("http://example.com").unwrap())
		.allow_headers([hyper::header::CONTENT_TYPE]);

	// Custom tower service to handle the RPC requests
	let service_builder = tower::ServiceBuilder::new()
		// Add high level tracing/logging to all requests
		.layer(
			TraceLayer::new_for_http()
				.on_request(
					|request: &hyper::Request<hyper::Body>, _span: &tracing::Span| tracing::info!(request = ?request, "sending body chunk"),
				)
				.make_span_with(DefaultMakeSpan::new().include_headers(true))
				.on_response(DefaultOnResponse::new().include_headers(true).latency_unit(LatencyUnit::Micros)),
		)
		// Mark the `Authorization` request header as sensitive so it doesn't show in logs
		.layer(SetSensitiveRequestHeadersLayer::new(once(hyper::header::AUTHORIZATION)))
		.layer(cors)
		.timeout(Duration::from_secs(2));

	let server =
		ServerBuilder::new().set_middleware(service_builder).build("127.0.0.1:0".parse::<SocketAddr>()?).await?;

	let addr = server.local_addr()?;

	let mut module = RpcModule::new(());
	module.register_method("say_hello", |_, _| Ok("lo")).unwrap();

	let handler = server.start(module)?;

	Ok((addr, handler))
}