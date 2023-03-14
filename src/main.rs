use std::{collections::VecDeque, fmt};

use futures::StreamExt;
use web3::{
	contract::Contract,
	ethabi::{Log, RawLog},
	types::{BlockId, BlockNumber, H160, H256, U256, U64},
};

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
	dotenv::dotenv().ok();

	let websocket_infura_endpoint: String = std::env::var("INFURA_WSS_ENDPOINT")?;

	let web3 =
		web3::Web3::new(web3::transports::ws::WebSocket::new(&websocket_infura_endpoint).await?);
	let contract_address =
		H160::from_slice(&hex::decode("5777d92f208679db4b9778590fa3cab3ac9e2168").unwrap()[..]);
	let contract = Contract::from_json(
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

		println!("Current block   = {:?} hash = {:?}", current_block_num, current_block_hash);
		println!("Block minus - 5 = {:?} hash = {:?}", block_number_b5, block_hash_b5);

		let mut logs = vec![];
		for log in swap_logs_in_block {
			let log = swap_event.parse_log(RawLog { topics: log.topics, data: log.data.0 })?;

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
		// We compute the 2's complement
		let mut bytes = [0u8; 32];
		amount.to_big_endian(&mut bytes);

		for b in bytes.iter_mut() {
			*b = !(*b);
		}

		amount = U256::from_big_endian(&bytes);
		amount += U256::one();
	}

	let (integer_part, decimal_part) = amount.div_mod(U256::from(10u64.pow(decimals as u32)));

	integer_part.as_u128() as f64 +
		decimal_part.as_u128() as f64 / (10u64.pow(decimals as u32)) as f64
}

#[derive(Clone)]
pub struct ParsedLog {
	pub sender: String,
	pub receiver: String,
	pub direction: String,
	pub amount_usdc: f64,
	pub amount_dai: f64,
}

impl fmt::Debug for ParsedLog {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "Parsed Log: {{\n")?;
		write!(f, " sender: {}\n", self.sender)?;
		write!(f, " receiver: {}\n", self.receiver)?;
		write!(f, " direction: {}\n", self.direction)?;
		write!(f, " amount_usdc: {:.2}\n", self.amount_usdc)?;
		write!(f, " amount_dai: {:.2}\n", self.amount_dai)?;
		write!(f, "}}")
	}
}

fn address_to_string(address: H160) -> String {
	let mut a = String::from("0x");
	a.push_str(hex::encode(&address).as_str());
	a
}

pub fn parse_log(log: Log) -> ParsedLog {
	let sender = address_to_string(log.params[0].value.clone().into_address().unwrap());
	let receiver = address_to_string(log.params[1].value.clone().into_address().unwrap());

	let amount_dai = log.params[2].value.clone().into_int().unwrap();
	let amount_usdc = log.params[3].value.clone().into_int().unwrap();

	// check the sign of each amount looking at the last bit (true = negative, false = positive)
	let is_amount_dai_negative = amount_dai.bit(255);
	let is_amount_usdc_negative = amount_usdc.bit(255);

	// one should be false and the other true
	assert!(is_amount_dai_negative ^ is_amount_usdc_negative);

	// the negative one is the swap's output
	let direction =
		if is_amount_usdc_negative { "DAI -> USDC".to_string() } else { "USDC -> DAI".to_string() };

	// format the amount according to the decimals of each token
	let amount_dai = u256_to_f64(amount_dai, 18);
	let amount_usdc = u256_to_f64(amount_usdc, 6);

	ParsedLog { sender, receiver, direction, amount_usdc, amount_dai }
}

pub struct QueueElement {
	pub block_hash: H256,
	pub parsed_log: Option<Vec<ParsedLog>>,
}

/// Pushes a new element (parsed logs) to the queue of events and, if the queue
/// has already 5 elements, pushes the ones that was pushed first. The function
/// panics if the hash of the element being popped from the queue has a changed
/// hash (the new hash of that block number = current block number - 5 should be
/// given).
///
/// # Arguments
///
/// * `queue` - Queue that contains at most 5 elements on it
///
/// * `new_elem` - New QueueElement to be pushed to the queue, this should have
/// it corresponding hash when the event was emitted
///
/// * `new_block_hash_b5` - Hash of current block number - 5 fetch at the moment
/// a new element is inserted in the queue.
pub fn push_to_queue(
	queue: &mut VecDeque<QueueElement>,
	new_elem: QueueElement,
	new_block_hash_b5: H256, // new fetched hash from block - 5
) -> Option<Vec<ParsedLog>> {
	const MAX_LEN: usize = 5;

	queue.push_back(new_elem);

	if queue.len() == MAX_LEN + 1 {
		match queue.pop_front() {
			Some(QueueElement { block_hash, parsed_log }) =>
				if block_hash == new_block_hash_b5 {
					return parsed_log
				} else {
					panic!("Block reorganization ocurred")
				},
			_ => panic!("Never achievable point"),
		}
	} else if queue.len() > MAX_LEN + 1 {
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
	fn test_push_to_queue() {
		// we test that an empty queue is properly filled and saturates in 5 elements
		// after the 5th element new elements and correct new hashes of
		// current block number - 5 are inserted
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
	fn test_panics_if_block_reorganization_occurs() {
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
