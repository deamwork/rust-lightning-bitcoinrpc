#![feature(async_await)]
use futures::future;
use futures::future::Future;
use futures::prelude::*;
use futures::channel::mpsc;
use futures_timer::Interval;
use futures::executor::{ ThreadPool, LocalPool };
use futures::task::{ LocalSpawn };

use std::time::{Duration};
use std::thread;
use std::error::Error;
use std::pin::Pin;
use std::sync::Arc;

// use ln_manager::executor::Larva;
use ln_manager::ln_bridge::rpc_client::{ RPCClient };

use hyper::{ Client, Uri };

#[macro_use] 
extern crate failure;
extern crate serde_derive;
extern crate serde;
extern crate serde_json;

use serde::{Serialize, Deserialize};
pub type UnboundedSender = mpsc::UnboundedSender<Pin<Box<dyn Future<Output = Result<Vec<User>, failure::Error>> + Send>>>;

#[derive(Deserialize, Debug)]
pub struct User {
    id: i32,
    name: String,
}

#[derive(Clone)]
pub struct Probe {
    sender: UnboundedSender,
    thread_pool: ThreadPool,
}


pub trait Larva: Clone + Sized + Send + Sync + 'static {
    fn spawn_task(
        &self,
        task: impl Future<Output = Result<Vec<User>, failure::Error>> + Send + 'static,
    ) -> Result<(), futures::task::SpawnError>;
}

impl Probe {
    pub fn new(sender: UnboundedSender) -> Self {
        Probe {
            sender: sender,
            thread_pool: ThreadPool::new().unwrap(),
        }
    }
}

impl Larva for Probe {
    fn spawn_task(
        &self,
        task: impl Future<Output = Result<Vec<User>, failure::Error>> + Send + 'static,
    ) -> Result<(), futures::task::SpawnError> {
        if let Err(err) = self.sender.unbounded_send(Box::pin(task)) {
            println!("{}", err);
            Err(futures::task::SpawnError::shutdown())
        } else {
            Ok(())
        }
    }
}

// let rpc_client = Arc::new(RPCClient::new(String::from("admin2:123@127.0.0.1:19011")));
// let r = runtime::spawn(async move {
// }).await;

// Interval::new(Duration::from_secs(1))
//     .for_each(|()|{
//         // rpc_client.clone().make_rpc_call("getblockchaininfo", &[], false);
//         future::ready(println!("run task"))
//     }).await;
    
// let r = rpc_client.make_rpc_call("getblockchaininfo", &[], false).await;
// println!("{}", &v.unwrap()); 

async fn h_get_json(i: usize) -> Result<Vec<User>, failure::Error> {
    // Interval::new(Duration::from_secs(1))
    //     .for_each(|()|{
    //         // rpc_client.clone().make_rpc_call("getblockchaininfo", &[], false);
    //         future::ready(println!("run task"))
    //     }).await;
    // let users = vec![ User { id: 1, name: String::from("Frank") }];
    let h_client = Arc::new(Client::new());
    let url: Uri = "http://jsonplaceholder.typicode.com/users".parse().unwrap();
    let res = h_client.get(url).await?;
    // // asynchronously concatenate chunks of the body
    let body = res.into_body().try_concat().await?;
    // // try to parse as json with serde_json
    let users: Vec<User> = serde_json::from_slice(&body)?;
    println!("{}", i);
    // println!("{:#?}", users);
    Ok::<Vec<User>, failure::Error>(users)
}

fn main() -> Result<(), failure::Error> {
    let (rt_tx, mut rt_rx) = mpsc::unbounded::<Pin<Box<dyn Future<Output = Result<Vec<User>, failure::Error>> + Send>>>();
    let exec = Probe::new(rt_tx); 
    
    let _ = exec.clone().spawn_task(async { h_get_json(0).await });
    // let _ = exec.clone().spawn_task(async { h_get_json(1).await });
    // let _ = exec.clone().spawn_task(async { h_get_json(2).await });
    // let _ = exec.clone().spawn_task(async { h_get_json(3).await });
    // let _ = exec.clone().spawn_task(h_get_json(1));
    // let _ = exec.clone().spawn_task(h_get_json(2));
    let _ = exec.clone().spawn_task(async { h_get_json(0).await });

    // let mut pool = LocalPool::new();
    let mut tokio_rt = tokio::runtime::Runtime::new().unwrap();

    loop {
        match rt_rx.try_next() {
            Ok(task) => {
                
                // let r = tokio_rt.block_on(async { task.unwrap().await });
                // let r = tokio_rt.block_on(task.unwrap());
                // let r = tokio_rt.spawn(async { task.unwrap().await; });
                tokio_rt.spawn(
                    task.unwrap().map(|_|{()})
                );

                // let r = runtime::raw::enter(runtime::native::Native, async { task.unwrap().await });
                // let r = runtime::raw::enter(runtime_tokio::Tokio, async { task.unwrap().await });
                // let r = pool.run_until(async { task.unwrap().await });
            }
            _ => { }
        }
    }

    Ok(())
}

// #[runtime::main]
// #[runtime::main(runtime_tokio::Tokio)]
// #[tokio::main]
// async fn main() -> Result<(), failure::Error> {
//     let users = h_get_json().await?;
//     println!("{:#?}", users);
//     Ok(())
// }
