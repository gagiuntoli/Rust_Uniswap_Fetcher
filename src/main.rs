use futures::StreamExt;
use web3::{ethabi::Log, types::U256};

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
			let log = swap_event
				.parse_log(web3::ethabi::RawLog { topics: log.topics, data: log.data.0 })?;

			println!("{:#?}", parse_log(log));
		}
	}

	Ok(())
}

fn u256_is_negative(amount: U256) -> bool {
	amount.bit(255)
}

fn u256_to_f64(amount: U256, decimals: u8) -> f64 {
	let mut amount = amount;

	if u256_is_negative(amount) {
		amount = U256::MAX - amount;
	}

	let (integer_part, decimal_part) = amount.div_mod(U256::from(10u64.pow(decimals as u32)));

	integer_part.as_u128() as f64 +
		decimal_part.as_u128() as f64 / (10u64.pow(decimals as u32)) as f64
}

#[derive(Debug)]
pub struct ParsedLog {
	pub sender: String,
	pub receiver: String,
	pub direction: String,
	pub amount_usdc: f64,
	pub amount_dai: f64,
}

fn parse_log(log: Log) -> ParsedLog {
	let sender = log.params[0].value.clone().into_address().unwrap().to_string();
	let receiver = log.params[1].value.clone().into_address().unwrap().to_string();
	let amount0 = log.params[2].value.clone().into_int().unwrap();
	let amount1 = log.params[3].value.clone().into_int().unwrap();

	let is_amount0_negative = amount0.bit(255);
	let is_amount1_negative = amount1.bit(255);

	// one should be false and the other true
	assert!(is_amount0_negative ^ is_amount1_negative);

	// let direction = if is_amount0_negative { "DAI -> USDC" } else { "USDC -> DAI" };
	let direction =
		if is_amount0_negative { "DAI -> USDC".to_string() } else { "USDC -> DAI".to_string() };

	let amount_dai = u256_to_f64(amount0, 18);
	let amount_usdc = u256_to_f64(amount1, 6);

	ParsedLog { sender, receiver, direction, amount_usdc, amount_dai }
}

#[cfg(test)]
mod tests {
	// Note this useful idiom: importing names from outer (for mod tests) scope.
	use super::*;

	#[test]
	fn test_hex_decode() {
		let m = U256::from(1000000000000u128);
		assert_eq!(u256_to_f64(m, 6), 1000000f64);

		let m = U256::from(1000000000001u128);
		assert_eq!(u256_to_f64(m, 6), 1000000.000001f64);
	}
}
