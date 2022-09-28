//! This example sets a custom tower service middleware which picks a variant
//! of rpc methods depending on the uri path.
//!
//! It works with both `WebSocket` and `HTTP` which is done in the example.

use jsonrpsee::rpc_params;
use std::net::SocketAddr;

use jsonrpsee::core::client::ClientT;
use jsonrpsee::http_client::HttpClientBuilder;
use jsonrpsee::server::{logger::Logger, RpcModule, ServerBuilder, TowerService};
use jsonrpsee::ws_client::WsClientBuilder;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	let addr = run_server().await?;

	// HTTP.
	{
		let client = HttpClientBuilder::default().build(format!("http://{}/v1", addr))?;
		let response: String = client.request("say_hello", rpc_params![]).await?;
		println!("[main]: http response: {:?}", response);
	}
	{
		let client = HttpClientBuilder::default().build(format!("http://{}/v2", addr))?;
		let response: String = client.request("say_hello", rpc_params![]).await?;
		println!("[main]: http response: {:?}", response);
	}
	{
		let client = HttpClientBuilder::default().build(format!("http://{}", addr))?;
		let response = client.request::<String, _>("say_hello", rpc_params![]).await.expect_err("404");
		println!("[main]: http response: {:}", response);
	}

	// WebSocket.
	{
		let client = WsClientBuilder::default().build(format!("ws://{}/v1", addr)).await?;
		let response: String = client.request("say_hello", rpc_params![]).await?;
		println!("[main]: ws response: {:?}", response);
	}
	{
		let client = WsClientBuilder::default().build(format!("ws://{}/v2", addr)).await?;
		let response: String = client.request("say_hello", rpc_params![]).await?;
		println!("[main]: ws response: {:?}", response);
	}
	{
		let error = WsClientBuilder::default().build(format!("ws://{}", addr)).await.expect_err("404");
		println!("[main]: ws response: {:}", error);
	}

	Ok(())
}

/// Wraps the ultimate core service of the jsonrpsee server in order to access its RPC method picker.
struct MethodRouter<L: Logger>(TowerService<L>);

impl<L> tower::Service<hyper::Request<hyper::Body>> for MethodRouter<L>
where
	L: Logger,
{
	type Response = <TowerService<L> as hyper::service::Service<hyper::Request<hyper::Body>>>::Response;
	type Error = <TowerService<L> as hyper::service::Service<hyper::Request<hyper::Body>>>::Error;
	type Future = <TowerService<L> as hyper::service::Service<hyper::Request<hyper::Body>>>::Future;

	fn poll_ready(&mut self, _cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
		std::task::Poll::Ready(Ok(()))
	}

	fn call(&mut self, req: hyper::Request<hyper::Body>) -> Self::Future {
		let idx = match req.uri().path() {
			"/v1" => 0,
			"/v2" => 1,
			_ => return Box::pin(std::future::ready(Ok(jsonrpsee::server::http_response::not_found()))),
		};

		self.0.inner.methods.pick(|all_methods| &all_methods[idx]);
		self.0.call(req)
	}
}

struct MethodRouterLayer;

impl<L: Logger> tower::Layer<TowerService<L>> for MethodRouterLayer {
	type Service = MethodRouter<L>;

	fn layer(&self, inner: TowerService<L>) -> Self::Service {
		MethodRouter(inner)
	}
}

async fn run_server() -> anyhow::Result<SocketAddr> {
	let service_builder = tower::ServiceBuilder::new().layer(MethodRouterLayer);

	let server =
		ServerBuilder::new().set_middleware(service_builder).build("127.0.0.1:0".parse::<SocketAddr>()?).await?;

	let addr = server.local_addr()?;

	let mut module_v1 = RpcModule::new(());
	module_v1.register_method("say_hello", |_, _| Ok("lo v1")).unwrap();
	let mut module_v2 = RpcModule::new(());
	module_v2.register_method("say_hello", |_, _| Ok("lo v2")).unwrap();

	// Serve different apis on different paths
	let handle = server.start_with_methods_variants([module_v1.into(), module_v2.into()])?;

	// In this example we don't care about doing shutdown so let's it run forever.
	// You may use the `ServerHandle` to shut it down or manage it yourself.
	tokio::spawn(handle.stopped());

	Ok(addr)
}
