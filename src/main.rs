use std::{collections::VecDeque, fmt};

use futures::StreamExt;
use web3::{
	contract::Contract,
	ethabi::{Event, Log, RawLog},
	transports::WebSocket,
	types::{BlockId, BlockNumber, H160, H256, U256, U64},
	Web3,
};

const BLOCK_REORG_MAX_DEPTH: usize = 5;

#[derive(Debug, Clone)]
pub struct Block {
	pub number: U64,
	pub hash: H256,
	pub parsed_logs: Vec<ParsedLog>,
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

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
	dotenv::dotenv().ok();

	assert!(BLOCK_REORG_MAX_DEPTH > 0, "BLOCK_REORG_MAX_DEPTH should be set larger than 0");

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

	let mut queue = VecDeque::<Block>::new();

	if let Some(Ok(block)) = block_stream.next().await {
		let current_block_num = block.number.expect("Error getting the current block number");

		let block_numbers: Vec<U64> = (0..BLOCK_REORG_MAX_DEPTH - 1)
			.rev()
			.map(|x| current_block_num - U64::from(x))
			.collect();

		queue = fetch_block_queue(
			block_numbers,
			web3.clone(),
			contract_address,
			swap_event_signature,
			swap_event.clone(),
		)
		.await;
	}

	while let Some(Ok(block)) = block_stream.next().await {
		let current_block_num = block.number.expect("Error getting the current block number");

		let mut block_numbers = queue.iter().map(|block| block.number).collect::<Vec<U64>>();
		block_numbers.push(current_block_num);

		let new_queue = fetch_block_queue(
			block_numbers,
			web3.clone(),
			contract_address,
			swap_event_signature,
			swap_event.clone(),
		)
		.await;

		queue.push_back(new_queue[new_queue.len() - 1].clone());

		assert_eq!(
			new_queue.len(),
			BLOCK_REORG_MAX_DEPTH,
			"`new_queue` should have length 5 all the time"
		);
		assert_eq!(
			queue.len(),
			BLOCK_REORG_MAX_DEPTH,
			"`queue` should have length {} at this point.",
			BLOCK_REORG_MAX_DEPTH
		);

		let reorganizations = check_and_update_queue(&mut queue, &new_queue);

		let block = queue.pop_front().expect("fail in popping element from the queue");

		println!("block: {} reorgs: {}", block.number, reorganizations);
		if block.parsed_logs.len() > 0 {
			println!("{:#?}", block.parsed_logs);
		}

		assert_eq!(
			queue.len(),
			BLOCK_REORG_MAX_DEPTH - 1,
			"`queue` should have length {} at this point.",
			BLOCK_REORG_MAX_DEPTH - 1
		);
	}

	Ok(())
}

pub async fn fetch_block_queue(
	block_numbers: Vec<U64>,
	web3: Web3<WebSocket>,
	contract_address: H160,
	swap_event_signature: H256,
	swap_event: Event,
) -> VecDeque<Block> {
	let mut queue = VecDeque::<Block>::new();

	for block_i in block_numbers {
		let block = web3
			.eth()
			.block(BlockId::Number(BlockNumber::Number(block_i)))
			.await
			.unwrap()
			.unwrap();

		let swap_logs_in_block = web3
			.eth()
			.logs(
				web3::types::FilterBuilder::default()
					.block_hash(block.hash.unwrap())
					.address(vec![contract_address])
					.topics(Some(vec![swap_event_signature]), None, None, None)
					.build(),
			)
			.await
			.unwrap();

		let mut parsed_logs = vec![];
		for log in swap_logs_in_block {
			let log =
				swap_event.parse_log(RawLog { topics: log.topics, data: log.data.0 }).unwrap();

			parsed_logs.push(parse_log(log));
		}

		assert_eq!(
			block_i,
			block.number.expect("could not get block number"),
			"block_i should equal `number` field of block fetched"
		);

		let hash = block.hash.expect("could not get block number");
		let number = block_i;

		queue.push_back(Block { hash, number, parsed_logs });
	}
	queue
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

/// This function updates the queue using a new queue. In case there was a
/// depth-5 block reorganization, the function panics.
pub fn check_and_update_queue(queue: &mut VecDeque<Block>, new_queue: &VecDeque<Block>) -> u32 {
	if queue[0].hash != new_queue[0].hash {
		panic!("A {}-blocks reorganization ocurred", BLOCK_REORG_MAX_DEPTH);
	}

	let mut reorganizations = 0;
	for (i, q) in queue.iter_mut().enumerate().rev() {
		assert_eq!(
			q.number, new_queue[i].number,
			"block numbers should be the same for checking if there was block reorganization"
		);

		if q.hash == new_queue[i].hash {
			break
		}
		*q = new_queue[i].clone();

		reorganizations += 1;
	}
	reorganizations
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
	fn test_check_and_update_queue_ok() {
		let mut queue = VecDeque::<Block>::from(vec![
			Block { hash: H256::random(), number: U64::from(1u32), parsed_logs: vec![] },
			Block { hash: H256::random(), number: U64::from(2u32), parsed_logs: vec![] },
			Block { hash: H256::random(), number: U64::from(3u32), parsed_logs: vec![] },
			Block { hash: H256::random(), number: U64::from(4u32), parsed_logs: vec![] },
			Block { hash: H256::random(), number: U64::from(5u32), parsed_logs: vec![] },
		]);

		let new_queue = VecDeque::<Block>::from(vec![
			Block { hash: queue[0].hash, number: U64::from(1u32), parsed_logs: vec![] },
			Block { hash: H256::random(), number: U64::from(2u32), parsed_logs: vec![] },
			Block { hash: H256::random(), number: U64::from(3u32), parsed_logs: vec![] },
			Block { hash: H256::random(), number: U64::from(4u32), parsed_logs: vec![] },
			Block { hash: H256::random(), number: U64::from(5u32), parsed_logs: vec![] },
		]);

		let reorganizations = check_and_update_queue(&mut queue, &new_queue);
		assert_eq!(reorganizations, 4)
	}

	#[test]
	#[should_panic(expected = "A 5-blocks reorganization ocurred")]
	fn test_check_and_update_queue_block_reorganization_5() {
		let mut queue = VecDeque::<Block>::from(vec![
			Block { hash: H256::random(), number: U64::from(5u32), parsed_logs: vec![] },
			Block { hash: H256::random(), number: U64::from(4u32), parsed_logs: vec![] },
			Block { hash: H256::random(), number: U64::from(3u32), parsed_logs: vec![] },
			Block { hash: H256::random(), number: U64::from(2u32), parsed_logs: vec![] },
			Block { hash: H256::random(), number: U64::from(1u32), parsed_logs: vec![] },
		]);

		let new_queue = VecDeque::<Block>::from(vec![
			Block { hash: H256::random(), number: U64::from(5u32), parsed_logs: vec![] },
			Block { hash: H256::random(), number: U64::from(4u32), parsed_logs: vec![] },
			Block { hash: H256::random(), number: U64::from(3u32), parsed_logs: vec![] },
			Block { hash: H256::random(), number: U64::from(2u32), parsed_logs: vec![] },
			Block { hash: H256::random(), number: U64::from(1u32), parsed_logs: vec![] },
		]);

		let reorganizations = check_and_update_queue(&mut queue, &new_queue);
		assert_eq!(reorganizations, 4)
	}
}
