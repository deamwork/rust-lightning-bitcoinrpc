extern crate futures;
extern crate hyper;
extern crate serde_json;
extern crate lightning;
extern crate lightning_net_tokio;
extern crate lightning_invoice;
extern crate rand;
extern crate secp256k1;
extern crate bitcoin;
extern crate tokio;
extern crate tokio_io;
extern crate tokio_fs;
extern crate tokio_codec;
extern crate bytes;
extern crate base64;
extern crate bitcoin_bech32;
extern crate bitcoin_hashes;

#[macro_use]
extern crate serde_derive;

mod rpc_client;
use rpc_client::*;

mod utils;

mod chain_monitor;
use chain_monitor::*;

mod event_handler;
use event_handler::*;

mod channel_monitor;
use channel_monitor::*;

mod command_handler;

use lightning_net_tokio::{Connection};

use futures::future;
use futures::future::Future;
use futures::Stream;

use secp256k1::key::PublicKey;
use secp256k1::Secp256k1;

use lightning::chain::keysinterface::{KeysInterface, KeysManager};
use lightning::ln::{peer_handler, router, channelmonitor};
use lightning::util::config;

use bitcoin::util::bip32;
use bitcoin::network::constants;

use std::{env, mem};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::vec::Vec;
use std::time::{Instant, Duration};
use std::fs;

mod lnbridge;
use lnbridge::log_printer::LogPrinter;

const FEE_PROPORTIONAL_MILLIONTHS: u32 = 10;
const ANNOUNCE_CHANNELS: bool = true;

#[allow(dead_code, unreachable_code)]
fn _check_usize_is_64() {
	// We assume 64-bit usizes here. If your platform has 32-bit usizes, wtf are you doing?
	unsafe { mem::transmute::<*const usize, [u8; 8]>(panic!()); }
}

fn main() {
	println!("USAGE: rust-lightning-jsonrpc user:pass@rpc_host:port storage_directory_path [port]");
	if env::args().len() < 3 { return; }

	let rpc_client = {
		let path = env::args().skip(1).next().unwrap();
		let path_parts: Vec<&str> = path.split('@').collect();
		if path_parts.len() != 2 {
			println!("Bad RPC URL provided");
			return;
		}
		Arc::new(RPCClient::new(path_parts[0], path_parts[1]))
	};

	let mut network = constants::Network::Bitcoin;
	let secp_ctx = Secp256k1::new();

	let fee_estimator = Arc::new(FeeEstimator::new());

	{
		println!("Checking validity of RPC URL to bitcoind...");
		let mut thread_rt = tokio::runtime::current_thread::Runtime::new().unwrap();
		thread_rt.block_on(rpc_client.make_rpc_call("getblockchaininfo", &[], false).and_then(|v| {
			assert!(v["verificationprogress"].as_f64().unwrap() > 0.99);
			assert_eq!(v["bip9_softforks"]["segwit"]["status"].as_str().unwrap(), "active");
			match v["chain"].as_str().unwrap() {
				"main" => network = constants::Network::Bitcoin,
				"test" => network = constants::Network::Testnet,
				"regtest" => network = constants::Network::Regtest,
				_ => panic!("Unknown network type"),
			}
			Ok(())
		})).unwrap();
		println!("Success! Starting up...");
	}

	if network == constants::Network::Bitcoin {
		panic!("LOL, you're insane");
	}

	let data_path = env::args().skip(2).next().unwrap();
	if !fs::metadata(&data_path).unwrap().is_dir() {
		println!("Need storage_directory_path to exist and be a directory (or symlink to one)");
		return;
	}
	let _ = fs::create_dir(data_path.clone() + "/monitors"); // If it already exists, ignore, hopefully perms are ok

	let port: u16 = match env::args().skip(3).next().map(|p| p.parse()) {
		Some(Ok(p)) => p,
		Some(Err(e)) => {
			println!("Error parsing port.");
			return;
		},
		None => 9735,
	};

	let logger = Arc::new(LogPrinter {});

  let our_node_seed = lnbridge::key::get_key_seed(data_path.clone());

	let keys = Arc::new(KeysManager::new(&our_node_seed, network, logger.clone()));
  let (import_key_1, import_key_2) = bip32::ExtendedPrivKey::new_master(network, &our_node_seed).map(|extpriv| {
		(extpriv.ckd_priv(&secp_ctx, bip32::ChildNumber::from_hardened_idx(1).unwrap()).unwrap().private_key.key,
		 extpriv.ckd_priv(&secp_ctx, bip32::ChildNumber::from_hardened_idx(2).unwrap()).unwrap().private_key.key)
	}).unwrap();

  // let (import_key_1, import_key_2) = lnbridge::key::extprivkey(network, &our_node_seed, &secp_ctx);
	let chain_monitor = Arc::new(ChainInterface::new(rpc_client.clone(), network, logger.clone()));

	let mut rt = tokio::runtime::Runtime::new().unwrap();
	rt.spawn(future::lazy(move || -> Result<(), ()> {
		tokio::spawn(rpc_client.make_rpc_call("importprivkey",
				&[&("\"".to_string() + &bitcoin::util::key::PrivateKey{ key: import_key_1, compressed: true, network}.to_wif() + "\""), "\"rust-lightning ChannelMonitor claim\"", "false"], false)
				.then(|_| Ok(())));
		tokio::spawn(rpc_client.make_rpc_call("importprivkey",
				&[&("\"".to_string() + &bitcoin::util::key::PrivateKey{ key: import_key_2, compressed: true, network}.to_wif() + "\""), "\"rust-lightning cooperative close\"", "false"], false)
				.then(|_| Ok(())));

		let monitors_loaded = ChannelMonitor::load_from_disk(&(data_path.clone() + "/monitors"));
		let monitor = Arc::new(ChannelMonitor {
			monitor: channelmonitor::SimpleManyChannelMonitor::new(chain_monitor.clone(), chain_monitor.clone(), logger.clone(), fee_estimator.clone()),
			file_prefix: data_path.clone() + "/monitors",
		});

		let mut config = config::UserConfig::new();
		config.channel_options.fee_proportional_millionths = FEE_PROPORTIONAL_MILLIONTHS;
		config.channel_options.announced_channel = ANNOUNCE_CHANNELS;

		let channel_manager = lnbridge::channel_manager::get_channel_manager(
      data_path.clone(),
      network.clone(),
      monitors_loaded,
      keys.clone(),
      fee_estimator.clone(),
      monitor.clone(),
      chain_monitor.clone(), // chain watcher
      chain_monitor.clone(), // chain broadcaster
      logger.clone(),
      config.clone(),
    );
		let router = Arc::new(router::Router::new(PublicKey::from_secret_key(&secp_ctx, &keys.get_node_secret()), chain_monitor.clone(), logger.clone()));

		let peer_manager = Arc::new(peer_handler::PeerManager::new(peer_handler::MessageHandler {
			chan_handler: channel_manager.clone(),
			route_handler: router.clone(),
		}, keys.get_node_secret(), logger.clone()));

		let payment_preimages = Arc::new(Mutex::new(HashMap::new()));
		let mut event_notify = EventHandler::setup(network, data_path, rpc_client.clone(), peer_manager.clone(), monitor.monitor.clone(), channel_manager.clone(), chain_monitor.clone(), payment_preimages.clone());

		let listener = tokio::net::TcpListener::bind(&format!("0.0.0.0:{}", port).parse().unwrap()).unwrap();

		let peer_manager_listener = peer_manager.clone();
		let event_listener = event_notify.clone();
		tokio::spawn(listener.incoming().for_each(move |sock| {
			println!("Got new inbound connection, waiting on them to start handshake...");
			Connection::setup_inbound(peer_manager_listener.clone(), event_listener.clone(), sock);
			Ok(())
		}).then(|_| { Ok(()) }));

		spawn_chain_monitor(fee_estimator, rpc_client, chain_monitor, event_notify.clone());

		tokio::spawn(tokio::timer::Interval::new(Instant::now(), Duration::new(1, 0)).for_each(move |_| {
			//TODO: Regularly poll chain_monitor.txn_to_broadcast and send them out
			Ok(())
		}).then(|_| { Ok(()) }));

    command_handler::run_command_board(
      network,
      router,
      event_notify,
      channel_manager,
      peer_manager,
      payment_preimages,
      secp_ctx,
      keys
    );

		Ok(())
	}));
	rt.shutdown_on_idle().wait().unwrap();
}
