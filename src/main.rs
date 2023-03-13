use futures::StreamExt;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
	dotenv::dotenv().ok();

	let websocket_infura_endpoint: String = std::env::var("INFURA_WSS_ENDPOINT")?;

	let web3 =
		web3::Web3::new(web3::transports::ws::WebSocket::new(&websocket_infura_endpoint).await?);
	let contract_address = web3::types::H160::from_slice(
		&hex::decode("5777d92f208679db4b9778590fa3cab3ac9e2168").unwrap()[..],
	);
	let contract = web3::contract::Contract::from_json(
		web3.eth(),
		contract_address,
		include_bytes!("contracts/uniswap_pool_abi.json"),
	)?;
	let swap_event = contract.abi().events_by_name("Swap")?.first().unwrap();
	let swap_event_signature = swap_event.signature();

	let mut block_stream = web3.eth_subscribe().subscribe_new_heads().await?;

	while let Some(Ok(block)) = block_stream.next().await {
		let swap_logs_in_block = web3
			.eth()
			.logs(
				web3::types::FilterBuilder::default()
					.block_hash(block.hash.unwrap())
					.address(vec![contract_address])
					.topics(Some(vec![swap_event_signature]), None, None, None)
					.build(),
			)
			.await?;

		for log in swap_logs_in_block {
			let parsed_log = swap_event
				.parse_log(web3::ethabi::RawLog { topics: log.topics, data: log.data.0 })?;
			println!("{:?}", parsed_log);
		}
	}

	Ok(())
}
