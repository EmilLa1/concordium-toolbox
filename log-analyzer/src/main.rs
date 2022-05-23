use anyhow::Context;
use chrono::{DateTime, Utc};
use clap::arg_enum;
use clap::AppSettings;
use serde_derive::Serialize;
use std::{
    io::{BufReader, Read},
    path::PathBuf,
    str::FromStr,
};
use structopt::StructOpt;

arg_enum! {
    #[derive(Debug)]
    enum Metric {
        // get the block execution by subtracting the block receive time from
        // block arrive time. Note. the log must've been obtained via debug
        BlockExecution,
    }
}

#[derive(Serialize)]
struct Row {
    #[serde(rename = "Block height")]
    block_height: usize,
    #[serde(rename = "Execution time")]
    execution_time: i64,
}

#[derive(StructOpt)]
struct Config {
    #[structopt(long = "in", help = "Log file to inspect")]
    log_file: PathBuf,
    #[structopt(long = "cfg", help = "Metrics to inspect")]
    metrics: Vec<Metric>,
    #[structopt(long = "out", help = "File to output csv")]
    out: Option<PathBuf>,
}

fn main() -> anyhow::Result<()> {
    let cfg = {
        let cfg = Config::clap().global_setting(AppSettings::ColoredHelp);
        let matches = cfg.get_matches();
        Config::from_clap(&matches)
    };
    let fs = std::fs::File::open(cfg.log_file).context("cannot open log file")?;

    let mut out = if let Some(out) = cfg.out {
        let out = csv::Writer::from_path(out).context("cannot create output file.")?;
        Some(out)
    } else {
        None
    };

    let mut buf_reader = BufReader::new(fs);
    let mut buf = String::new();
    buf_reader
        .read_to_string(&mut buf)
        .context("cannot read log file")?;

    let mut block_execution_times: Vec<(DateTime<Utc>, Option<DateTime<Utc>>)> = vec![];
    let lines = buf.lines();

    let block_execution = cfg
        .metrics
        .iter()
        .any(|m| matches!(m, Metric::BlockExecution));

    let mut parsing = false;
    let mut block_height = 0;

    for line in lines {
        if block_execution {
            if !parsing && line.contains("Skov: Received block") {
                parsing = true;
                let receive_time = extract_timestamp(&line.to_string())?;
                println!("Block {} Received {}", block_height, receive_time);
                block_execution_times.push((receive_time, None));
            }
            if parsing && line.contains("arrived") {
                if let Some(last) = block_execution_times.last_mut() {
                    let arrive_time = extract_timestamp(&line.to_string())?;
                    println!("Block {} Arrived {}", block_height, arrive_time);
                    last.1 = Some(arrive_time);
                };
                parsing = false;
                block_height += 1;
            }
        }
    }

    let mut csv_rows = vec![];
    // write to csv if enabled
    for (height, be) in block_execution_times.iter().enumerate() {
        if let (receive, Some(arrive)) = be {
            let execution_time = *arrive - *receive;
            csv_rows.push(Row {
                block_height: height,
                execution_time: execution_time.num_milliseconds(),
            });
        }
    }

    for row in csv_rows {
        if let Some(ref mut writer) = out {
            writer.serialize(row)?;
        };
    }
    Ok(())
}

fn extract_timestamp(log_line: &str) -> anyhow::Result<DateTime<Utc>> {
    //"2022-05-22T10:45:55.229618571Z".len()
    let (ts_str, _) = log_line.split_at(30);
    DateTime::from_str(ts_str).context("cannot parse DateTime")
}
