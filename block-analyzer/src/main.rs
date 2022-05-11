use anyhow::Context;
use chrono::{DateTime, Utc};
use clap::AppSettings;
use concordium_rust_sdk::{
    common::SerdeSerialize,
    endpoints,
    types::{self, hashes::BlockHash, AbsoluteBlockHeight, Slot},
};
use structopt::StructOpt;

#[derive(StructOpt)]
struct App {
    #[structopt(
        long = "node",
        help = "GRPC interface of the node.",
        default_value = "http://localhost:7000"
    )]
    endpoint: tonic::transport::Endpoint,
    #[structopt(long = "block")]
    start_block: Option<types::hashes::BlockHash>,
    #[structopt(long = "out", help = "File to output the measurements to.")]
    out: Option<std::path::PathBuf>,
}

#[derive(SerdeSerialize)]
struct Row {
    #[serde(rename = "Block height")]
    block_height: AbsoluteBlockHeight,
    #[serde(rename = "Block hash")]
    block_hash: BlockHash,
    #[serde(rename = "Receive time")]
    receive_time: DateTime<Utc>,
    #[serde(rename = "Arrive time")]
    arrive_time: DateTime<Utc>,
    #[serde(rename = "Block execution time")]
    execution_time: i64,
    #[serde(rename = "Block slot")]
    block_slot: Slot,
    #[serde(rename = "Block slot time")]
    block_slot_time: DateTime<Utc>,
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    let app = {
        let app = App::clap().global_setting(AppSettings::ColoredHelp);
        let matches = app.get_matches();
        App::from_clap(&matches)
    };

    let mut client = endpoints::Client::connect(app.endpoint, "rpcadmin".to_string()).await?;

    let version = client.version().await?;
    println!("Version: {}", version);
    let peers = client.peer_list(true).await?;
    println!("Peers: {:?}", peers);

    let ni = client.node_info().await?;
    println!("Node info: {:?}", ni);

    let mut out = if let Some(out) = app.out {
        let out = csv::Writer::from_path(out).context("Could not create output file.")?;
        Some(out)
    } else {
        None
    };

    let consensus_info = client.get_consensus_status().await?;
    let gb = consensus_info.genesis_block;
    let mut cb = app.start_block.unwrap_or(consensus_info.best_block);
    while cb != gb {
        let bi = client.get_block_info(&cb).await?;
        if bi.transaction_count != 0 {
            let block_hash = bi.block_hash;
            println!("{}", block_hash);
            let block_receive_time = bi.block_receive_time;
            let block_arrive_time = bi.block_arrive_time;

            let block_slot = bi.block_slot;
            let block_slot_time = bi.block_slot_time;

            println!("Block receive time: {:?}", block_receive_time);
            println!("Block arrive time: {:?}", block_arrive_time);
            let block_execution_time = block_arrive_time - block_receive_time;
            println!("Block execution time: {:?}", block_execution_time);
            println!("Block slot {:?}", block_slot);
            println!("Block slot time {:?}", block_slot_time);

            if let Some(ref mut writer) = out {
                writer.serialize(Row {
                    block_hash: block_hash,
                    block_height: bi.block_height,
                    receive_time: block_receive_time,
                    arrive_time: block_arrive_time,
                    execution_time: block_execution_time.num_milliseconds(),
                    block_slot: block_slot,
                    block_slot_time: block_slot_time,
                })?;
            };
        }
        cb = bi.block_parent;
    }
    Ok(())
}
