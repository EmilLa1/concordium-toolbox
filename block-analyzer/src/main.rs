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
        long = "nodes",
        help = "GRPC interface of the node.",
        use_delimiter = true,
        default_value = "http://localhost:7000,http://localhost:7001,http://localhost:7002,http://localhost:7003,http://localhost:7004"
    )]
    endpoints: Vec<tonic::transport::Endpoint>,
    #[structopt(long = "block")]
    start_block: Option<types::hashes::BlockHash>,
    #[structopt(long = "out", help = "File to output the measurements to.")]
    out: Option<std::path::PathBuf>,
}

#[derive(SerdeSerialize)]
struct Row {
    #[serde(rename = "Node id")]
    node: String,
    #[serde(rename = "Block height")]
    block_height: AbsoluteBlockHeight,
    #[serde(rename = "Block hash")]
    block_hash: BlockHash,
    #[serde(rename = "Receive time")]
    receive_time: DateTime<Utc>,
    #[serde(rename = "Arrive time")]
    arrive_time: DateTime<Utc>,
    #[serde(rename = "Transaction count")]
    tx_count: u64,
    #[serde(rename = "Block execution time (millis)")]
    execution_time: i64,
    #[serde(rename = "Block slot")]
    block_slot: Slot,
    #[serde(rename = "Block slot time")]
    block_slot_time: DateTime<Utc>,
    #[serde(rename = "Block propagation time (millis)")]
    block_propagation_time: i64,
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    let app = {
        let app = App::clap().global_setting(AppSettings::ColoredHelp);
        let matches = app.get_matches();
        App::from_clap(&matches)
    };

    let mut node_uris = vec![];
    for e in &app.endpoints {
        let node_uri = e.uri().to_string();
        node_uris.push(node_uri);
    }

    let mut out = if let Some(ref out) = app.out {
        let out = csv::Writer::from_path(out).context("Could not create output file.")?;
        Some(out)
    } else {
        None
    };
    let mut csv_rows = vec![];

    let mut node_idx = 0;
    for endpoint in app.endpoints {
        let mut client = endpoints::Client::connect(endpoint, "rpcadmin".to_string()).await?;

        let version = client.version().await?;
        println!("Version: {}", version);
        let peers = client.peer_list(true).await?;
        println!("Peers: {:?}", peers);

        let ni = client.node_info().await?;
        println!("Node info: {:?}", ni);

        let consensus_info = client.get_consensus_status().await?;
        let gb = consensus_info.genesis_block;
        let mut cb = app.start_block.unwrap_or(consensus_info.best_block);
        while cb != gb {
            let bi = client.get_block_info(&cb).await?;
            if bi.transaction_count != 0 {
                let block_hash = bi.block_hash;
                println!("{}", node_uris[node_idx]);
                println!("{}", block_hash);
                let block_receive_time = bi.block_receive_time;
                let block_arrive_time = bi.block_arrive_time;

                let block_slot = bi.block_slot;
                let block_slot_time = bi.block_slot_time;

                println!("Block receive time: {}", block_receive_time);
                println!("Block arrive time: {}", block_arrive_time);
                let block_execution_time =
                    (block_arrive_time - block_receive_time).num_milliseconds();
                println!("Block execution time: {}", block_execution_time);
                println!("Block slot {}", block_slot);
                println!("Block slot time {}", block_slot_time);
                let block_propagation_time =
                    (block_receive_time - block_slot_time).num_milliseconds();
                println!("Block propagation time {}", block_propagation_time);
                println!("Consensus status {:?}", consensus_info);
                let transaction_count = bi.transaction_count;
                println!("Transactions in block: {}", transaction_count);

                csv_rows.push(Row {
                    node: node_uris[node_idx].as_str().to_string(),
                    block_hash,
                    block_height: bi.block_height,
                    receive_time: block_receive_time,
                    tx_count: transaction_count,
                    arrive_time: block_arrive_time,
                    execution_time: block_execution_time,
                    block_slot,
                    block_slot_time,
                    block_propagation_time,
                });
            }
            cb = bi.block_parent;
        }
        node_idx += 1;
    }

    csv_rows.reverse();
    for row in csv_rows {
        if let Some(ref mut writer) = out {
            writer.serialize(row)?;
        };
    }

    Ok(())
}
