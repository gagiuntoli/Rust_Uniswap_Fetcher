use std::collections::VecDeque;

use futures::StreamExt;
use web3::{
	ethabi::Log,
	types::{BlockId, BlockNumber, H256, U256, U64},
};

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

	let mut queue = VecDeque::<QueueElement>::new();

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

		let current_block_num = block.number.expect("Error getting the current block number");
		let current_block_hash = block.hash.expect("Error getting the current block hash");

		println!("Current block   = {:?} hash = {:?}", current_block_num, current_block_hash);

		let block_b5 = web3
			.eth()
			.block(BlockId::Number(BlockNumber::Number(current_block_num - U64::from(5))))
			.await
			.unwrap()
			.unwrap();

		let block_number_b5 =
			block_b5.number.expect("Error getting the block number = current - 5");

		let block_hash_b5 =
			block_b5.hash.expect("Error getting the hash of block number = current - 5");

		assert_eq!(block_number_b5, current_block_num - U64::from(5));

		println!("Block minus - 5 = {:?} hash = {:?}", block_number_b5, block_hash_b5);

		let mut logs = vec![];
		for log in swap_logs_in_block {
			let log = swap_event
				.parse_log(web3::ethabi::RawLog { topics: log.topics, data: log.data.0 })?;

			logs.push(parse_log(log));
		}

		let elem = match logs.len() {
			0 => QueueElement { block_hash: current_block_hash, parsed_log: None },
			_ => QueueElement { block_hash: current_block_hash, parsed_log: Some(logs) },
		};

		let candidate = push_to_queue(&mut queue, elem, block_hash_b5);
		if let Some(printable) = candidate {
			println!("Logs from block {:?}", block_number_b5);
			println!("{:#?}", printable);
		}
	}

	Ok(())
}

pub fn u256_is_negative(amount: U256) -> bool {
	amount.bit(255)
}

pub fn u256_to_f64(amount: U256, decimals: u8) -> f64 {
	let mut amount = amount;

	if u256_is_negative(amount) {
		amount = U256::MAX - amount;
	}

	let (integer_part, decimal_part) = amount.div_mod(U256::from(10u64.pow(decimals as u32)));

	integer_part.as_u128() as f64 +
		decimal_part.as_u128() as f64 / (10u64.pow(decimals as u32)) as f64
}

#[derive(Debug, Clone)]
pub struct ParsedLog {
	pub sender: String,
	pub receiver: String,
	pub direction: String,
	pub amount_usdc: f64,
	pub amount_dai: f64,
}

pub fn parse_log(log: Log) -> ParsedLog {
	let sender = log.params[0].value.clone().into_address().unwrap().to_string();
	let receiver = log.params[1].value.clone().into_address().unwrap().to_string();
	let amount_dai = log.params[2].value.clone().into_int().unwrap();
	let amount_usdc = log.params[3].value.clone().into_int().unwrap();

	let is_amount_dai_negative = amount_dai.bit(255);
	let is_amount_usdc_negative = amount_usdc.bit(255);

	// one should be false and the other true
	assert!(is_amount_dai_negative ^ is_amount_usdc_negative);

	// let direction = if is_amount0_negative { "DAI -> USDC" } else { "USDC -> DAI" };
	let direction =
		if is_amount_usdc_negative { "DAI -> USDC".to_string() } else { "USDC -> DAI".to_string() };

	let amount_dai = u256_to_f64(amount_dai, 18);
	let amount_usdc = u256_to_f64(amount_usdc, 6);

	ParsedLog { sender, receiver, direction, amount_usdc, amount_dai }
}

pub struct QueueElement {
	pub block_hash: H256,
	pub parsed_log: Option<Vec<ParsedLog>>,
}

pub fn push_to_queue(
	log_queue: &mut VecDeque<QueueElement>,
	new_elem: QueueElement,
	new_block_hash_b5: H256, // new fetched hash from block - 5
) -> Option<Vec<ParsedLog>> {
	const MAX_LEN: usize = 5;

	log_queue.push_back(new_elem);

	if log_queue.len() == MAX_LEN + 1 {
		match log_queue.pop_front() {
			Some(QueueElement { block_hash, parsed_log }) =>
				if block_hash == new_block_hash_b5 {
					return parsed_log
				} else {
					panic!("Block reorganization ocurred")
				},
			_ => panic!("Never achievable point"),
		}
	} else if log_queue.len() > MAX_LEN + 1 {
		panic!("Queue length of 5 elements overpassed")
	}
	None
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

	#[test]
	fn test_push_to_queue_saturates_in_5_elems() {
		let mut queue = VecDeque::<QueueElement>::new();

		let block_hashes =
			vec![H256::random(), H256::random(), H256::random(), H256::random(), H256::random()];

		for i in 0..5 {
			assert_eq!(queue.len(), i);

			// Here the queue.len() < 5 so any old b - 5 hash submitted doesn't affect the result
			let next = push_to_queue(
				&mut queue,
				QueueElement { block_hash: block_hashes[i], parsed_log: None },
				H256::random(),
			);

			match next {
				None => (),
				_ => assert!(false),
			}

			assert_eq!(queue.len(), i + 1);
		}

		for i in 0..5 {
			assert_eq!(queue.len(), 5);

			let next = push_to_queue(
				&mut queue,
				QueueElement { block_hash: H256::random(), parsed_log: None },
				block_hashes[i],
			);

			match next {
				None => (),
				_ => assert!(false),
			}

			assert_eq!(queue.len(), 5);
		}
	}

	#[test]
	#[should_panic(expected = "Block reorganization ocurred")]
	fn test_false_block_hash_b5() {
		let mut queue = VecDeque::<QueueElement>::new();

		let block_hashes =
			vec![H256::random(), H256::random(), H256::random(), H256::random(), H256::random()];

		let new_block_hash_b5 = H256::random();

		for i in 0..5 {
			assert_eq!(queue.len(), i);

			let next = push_to_queue(
				&mut queue,
				QueueElement { block_hash: block_hashes[i], parsed_log: None },
				new_block_hash_b5,
			);

			match next {
				None => (),
				_ => assert!(false),
			}

			assert_eq!(queue.len(), i + 1);
		}

		assert_eq!(queue.len(), 5);

		let _next = push_to_queue(
			&mut queue,
			QueueElement { block_hash: H256::random(), parsed_log: None },
			H256::random(),
		);
	}

	#[test]
	#[should_panic(expected = "Queue length of 5 elements overpassed")]
	fn test_panics_if_queue_is_larger_than_5() {
		let mut queue = VecDeque::<QueueElement>::new();
		for _i in 0..=5 {
			queue.push_back(QueueElement { block_hash: H256::random(), parsed_log: None })
		}

		assert_eq!(queue.len(), 6);

		let _next = push_to_queue(
			&mut queue,
			QueueElement { block_hash: H256::random(), parsed_log: None },
			H256::random(),
		);
	}
}
