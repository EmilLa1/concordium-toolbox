use anyhow::Context;
use chrono::{DateTime, Utc};
use clap::AppSettings;
use serde_derive::Serialize;
use std::path::PathBuf;
use std::thread::sleep;
use std::time::Duration;
use structopt::StructOpt;
use sysinfo::{Pid, ProcessExt, System, SystemExt};

#[derive(Serialize)]
struct Row {
    #[serde(rename = "Time")]
    time: DateTime<Utc>,
    #[serde(rename = "Cpu usage (%)")]
    cpu_usage: f32,
    #[serde(rename = "Memory usage (kb)")]
    memory_usage: u64,
    #[serde(rename = "Disk read (kb)")]
    disk_read: u64,
    #[serde(rename = "Disk write (kb)")]
    disk_write: u64,
    #[serde(rename = "Disk read kb/s")]
    disk_read_per_sec: u64,
    #[serde(rename = "Disk write kb/s")]
    disk_write_per_sec: u64,
    #[serde(rename = "Disk read total (kb)")]
    disk_read_total: u64,
    #[serde(rename = "Disk write total (kb)")]
    disk_write_total: u64,
}

#[derive(StructOpt)]
struct Config {
    #[structopt(long = "pid", help = "Process to inspect")]
    pid: i32,
    #[structopt(
        long = "time",
        help = "Time to measure (minutes). Default is 5 minutes."
    )]
    time: Option<u64>,
    #[structopt(
        long = "interval",
        help = "Interval between retrieving metrics. Default is 3 seconds."
    )]
    interval: Option<u64>,
    #[structopt(long = "out", help = "File to output csv")]
    out: Option<PathBuf>,
}

fn main() -> anyhow::Result<()> {
    let mut system = System::new_all();

    let cfg = {
        let cfg = Config::clap().global_setting(AppSettings::ColoredHelp);
        let matches = cfg.get_matches();
        Config::from_clap(&matches)
    };
    let pid = Pid::from(cfg.pid);

    let mut out = if let Some(out) = cfg.out {
        let out = csv::Writer::from_path(out).context("cannot create output file.")?;
        Some(out)
    } else {
        None
    };

    let time: u64 = if let Some(time) = cfg.time {
        time * 60
    } else {
        300
    };
    let interval: u64 = if let Some(interval) = cfg.interval {
        interval
    } else {
        3
    };

    let iterations = time / interval;

    let mut csv_rows = vec![];
    for i in 1..iterations + 1 {
        system.refresh_process(pid);
        let proc = if let Some(proc) = system.process(pid) {
            proc
        } else {
            anyhow::bail!("Unknown pid");
        };

        let cpu_usage = proc.cpu_usage();
        let memory_usage = proc.memory();
        let disk_usage = proc.disk_usage();

        let disk_read = disk_usage.read_bytes;
        let disk_read_total = disk_usage.total_read_bytes;
        let disk_write = disk_usage.written_bytes;
        let disk_write_total = disk_usage.total_written_bytes;

        let disk_read_per_sec = disk_read / interval;
        let disk_write_per_sec = disk_write / interval;

        let time = chrono::offset::Utc::now();
        csv_rows.push(Row {
            time,
            cpu_usage,
            memory_usage,
            disk_read,
            disk_write,
            disk_read_per_sec,
            disk_write_per_sec,
            disk_read_total,
            disk_write_total,
        });
        println!(
            "{}/{} | Time {} | CPU {}% | Mem {} MB | Disk Read {} KB/s | Disk Write {} KB/s",
            i,
            iterations,
            time,
            cpu_usage,
            memory_usage / 1000,
            disk_read_per_sec,
            disk_write_per_sec
        );
        sleep(Duration::from_secs(interval));
    }

    for row in csv_rows {
        if let Some(ref mut writer) = out {
            writer.serialize(row).context("Unable to write csv row")?;
        }
    }

    Ok(())
}
