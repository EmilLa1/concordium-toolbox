use anyhow::Context;
use clap::AppSettings;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::process::Command;
use std::process::Stdio;
use structopt::StructOpt;
use tui::{
    backend::{Backend, CrosstermBackend},
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, Paragraph, Tabs, Wrap},
    Frame, Terminal,
};

#[derive(StructOpt)]
struct Config {
    #[structopt(
        long = "num-nodes",
        help = "The number of nodes to spawn",
        default_value = "5"
    )]
    num_nodes: usize,
    #[structopt(
        long = "optimal-connected",
        help = "default the peers in the network will be connected in line. If optimal-connected is chosen then every peer will have connection to all other peers."
    )]
    optimal_connected: bool,
    #[structopt(
        long = "genesis-root",
        help = "Path to genesis_data",
        default_value = "../deps/concordium-node/scripts/genesis/genesis_data/"
    )]
    genesis_root: String,
    #[structopt(
        long = "rpc-port-offset",
        help = "gRPC port offset. The nodes will be spawned this port and incrementing the port number for each",
        default_value = "7000"
    )]
    rpc_port_offset: usize,
    #[structopt(
        long = "p2p-port-offset",
        help = "P2p port offset. The nodes will be spawned this port and incrementing the port number for each",
        default_value = "8000"
    )]
    peer_port_offset: usize,
    #[structopt(long = "rts-flags", help = "RTS flags", default_value = "-N2")]
    rts_flags: String,
    #[structopt(
        long = "housekeeping-interval",
        help = "Interval in seconds where the node cleans up its connections etc.",
        default_value = "300"
    )]
    housekeeping_interval: usize,
    #[structopt(
        long = "continue-state",
        help = "If this is set then the nodes will use existing data directories."
    )]
    continue_state: bool,
    #[structopt(long = "no-emit-logs", help = "If true no log files will be emitted.")]
    no_emit_logs: bool,
}

struct App<'a> {
    pub titles: Vec<&'a str>,
    pub index: usize,
}

impl<'a> App<'a> {
    fn new(titles: &'a [std::string::String]) -> App<'a> {
        App {
            titles: titles.iter().map(AsRef::as_ref).collect(),
            index: 0,
        }
    }

    pub fn next(&mut self) {
        self.index = (self.index + 1) % self.titles.len();
    }

    pub fn previous(&mut self) {
        if self.index > 0 {
            self.index -= 1;
        } else {
            self.index = self.titles.len() - 1;
        }
    }
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    let cfg = {
        let cfg = Config::clap().global_setting(AppSettings::ColoredHelp);
        let matches = cfg.get_matches();
        Config::from_clap(&matches)
    };

    // setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut titles: Vec<String> = vec![];
    for i in 0..cfg.num_nodes {
        titles.push(format!("Node {:?}", i));
    }

    // create app and run it
    let app = App::new(&titles);
    let res = run_app(&mut terminal, app, &cfg);

    // restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{:?}", err)
    }

    Ok(())
}

fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    mut app: App,
    cfg: &Config,
) -> anyhow::Result<()> {
    // start the nodes.
    let mut forks = vec![];
    let mut stdout_receivers = vec![];
    let mut log_buffers = vec![];
    let genesis_root = std::path::PathBuf::from(&cfg.genesis_root)
        .canonicalize()
        .context("invalid genesis path")?;
    let path_to_node = "../deps/concordium-node/concordium-node/Cargo.toml";
    let node_path = std::path::PathBuf::from(path_to_node)
        .canonicalize()
        .context("invalid node path")?;

    for i in 0..cfg.num_nodes {
        log_buffers.push(String::new());

        if !cfg.continue_state {
            let _ = std::fs::remove_dir_all(node_path.join(format!("peer-{}", i)))
                .context("cannot remove old peer directory.");
        }

        std::fs::create_dir_all(format!("peer-{}", i)).context("Cannot create peer directory")?;

        //copy genesis.dat to peer directory.
        let genesis_dat = genesis_root
            .join("genesis.dat")
            .canonicalize()
            .context("cannot find genesis.dat")?;
        std::fs::copy(genesis_dat, format!("peer-{}/genesis.dat", i))
            .context("Cannot copy genesis dat to peer directory")?;

        // command for running the node
        let cmd = &mut Command::new("cargo");
        cmd.env("RUST_BACKTRACE", "full");
        cmd.env("CONCORDIUM_NODE_RUNTIME_HASKELL_RTS_FLAGS", &cfg.rts_flags);
        cmd.env("CONCORDIUM_NODE_CONNECTION_NO_BOOTSTRAP_DNS", "1");
        cmd.env("CONCORDIUM_NODE_ID", format!("{:016x}", i as u64).as_str());
        cmd.env(
            "CONCORDIUM_NODE_CONFIG_DIR",
            format!("peer-{:?}", i).as_str(),
        );
        cmd.env("CONCORDIUM_NODE_DATA_DIR", format!("peer-{:?}", i).as_str());
        cmd.env(
            "CONCORDIUM_NODE_RPC_SERVER_PORT",
            format!("{}", i + cfg.rpc_port_offset).as_str(),
        );
        cmd.env(
            "CONCORDIUM_NODE_LISTEN_PORT",
            format!("{}", i + cfg.peer_port_offset).as_str(),
        );
        cmd.env("CONCORDIUM_NODE_LISTEN_ADDRESS", "0.0.0.0");
        cmd.env(
            "CONCORDIUM_NODE_CONNECTION_HOUSEKEEPING_INTERVAL",
            format!("{}", cfg.housekeeping_interval).as_str(),
        );
        cmd.env(
            "CONCORDIUM_NODE_MAX_NORMAL_KEEP_ALIVE",
            format!("{}", cfg.housekeeping_interval * 3).as_str(),
        );

        cmd.arg("run");
        cmd.args(["--manifest-path", path_to_node]);
        cmd.arg("--release");
        cmd.arg("--quiet");
        cmd.arg("--");
        if !cfg.no_emit_logs {
            //            cmd.args(["-d", "1"]);
        }
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        if !cfg.optimal_connected {
            // the nodes will be connected sequentially
            // we submit transactions at the start of the queue.
            // O - O - O - O - B

            // assign the last node to be baker
            if i == cfg.num_nodes - 1 {
                let baker_credentials = genesis_root
                    .join("bakers/baker-0-credentials.json")
                    .canonicalize()
                    .context("Invalid baker credentials")?;
                cmd.env(
                    "CONCORDIUM_NODE_BAKER_CREDENTIALS_FILE",
                    baker_credentials.to_str().unwrap().to_string().as_str(),
                );
            }

            // if the node is last in line we don't connect to the one behind us.
            let next_peer_port = cfg.peer_port_offset + i + 1;

            // we're the first peer in line so we only connect to the peer in front of us.
            if i < cfg.num_nodes - 1 {
                cmd.env(
                    "CONCORDIUM_NODE_CONNECTION_CONNECT_TO",
                    format!("127.0.0.1:{}", next_peer_port),
                );
            }

            // if the node is either at the start or at the end it should only be connected one other peer
            if i == 0 || i == cfg.num_nodes - 1 {
                cmd.env(
                    "CONCORDIUM_NODE_CONNECTION_DESIRED_NODES",
                    format!("{}", 1).as_str(),
                );
                cmd.env(
                    "CONCORDIUM_NODE_CONNECTION_MAX_ALLOWED_NODES",
                    format!("{}", 1).as_str(),
                );
            } else {
                // else the peer will be connected to the peer at 'each side' of it.
                cmd.env(
                    "CONCORDIUM_NODE_CONNECTION_DESIRED_NODES",
                    format!("{}", 2).as_str(),
                );
                cmd.env(
                    "CONCORDIUM_NODE_CONNECTION_MAX_ALLOWED_NODES",
                    format!("{}", 2).as_str(),
                );
            }
        } else {
            // assign first 5 nodes to be bakers
            if i < 5 {
                let baker_credentials = genesis_root
                    .join(format!("bakers/baker-{}-credentials.json", i))
                    .canonicalize()
                    .context("Invalid baker credentials")?;
                cmd.env(
                    "CONCORDIUM_NODE_BAKER_CREDENTIALS_FILE",
                    baker_credentials.to_str().unwrap().to_string().as_str(),
                );
            }

            for n in 0..cfg.num_nodes {
                if i == n {
                    continue;
                }
                cmd.args([
                    "--connect-to",
                    format!("127.0.0.1:{}", cfg.peer_port_offset + n).as_str(),
                ]);
            }
        }

        let mut fork = cmd
            .spawn()
            .context(format!("Failed to launch node {:?}", i))?;

        let mut fh = if !cfg.no_emit_logs {
            Some(
                std::fs::File::create(format!("peer-{}.log", i))
                    .context(format!("cannot create log file for peer {}", i))?,
            )
        } else {
            None
        };

        let mut buf_reader = BufReader::new(fork.stderr.take().context("Could not take stderr")?);
        forks.push(fork);
        // create a channel for reading stdout of the forked process.
        let (sender, receiver) = tokio::sync::mpsc::channel(100);
        stdout_receivers.push(receiver);
        let reader = async move {
            loop {
                let mut buffered_line = String::new();
                for _ in 0..10 {
                    buf_reader.read_line(&mut buffered_line).unwrap();
                }
                if !buffered_line.is_empty() {
                    // write to log file if enabled
                    match fh {
                        Some(ref mut fh) => fh.write(buffered_line.clone().as_bytes()),
                        None => Ok(0),
                    }
                    .context("Failed to write log")
                    .unwrap();
                    // send to ui
                    sender
                        .send(buffered_line)
                        .await
                        .context("mpsc sender failed")
                        .unwrap();
                }
            }
        };
        tokio::spawn(reader);
    }

    // run until someone presses `q`.
    loop {
        // append to the logs
        for i in 0..cfg.num_nodes {
            if let Ok(log) = stdout_receivers
                .get_mut(i)
                .context("could not get mpsc reader")
                .unwrap()
                .try_recv()
            {
                log_buffers.get_mut(i).unwrap().push_str(&log);
            };
        }
        // draw the ui
        terminal.draw(|f| ui(f, &app, &log_buffers).unwrap())?;
        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Char('q') => {
                    for mut receiver in stdout_receivers {
                        receiver.close();
                    }
                    for mut f in forks {
                        f.kill()?;
                    }
                    return Ok(());
                }
                KeyCode::Right => app.next(),
                KeyCode::Left => app.previous(),
                _ => {}
            }
        }
    }
}

fn ui<B: Backend>(f: &mut Frame<B>, app: &App, logs: &[String]) -> anyhow::Result<()> {
    let size = f.size();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(5)
        .constraints([Constraint::Length(3), Constraint::Min(0)].as_ref())
        .split(size);

    let block = Block::default().style(Style::default().bg(Color::White).fg(Color::Black));
    f.render_widget(block, size);
    let titles = app
        .titles
        .iter()
        .map(|t| {
            let (first, rest) = t.split_at(1);
            Spans::from(vec![
                Span::styled(first, Style::default().fg(Color::Yellow)),
                Span::styled(rest, Style::default().fg(Color::Green)),
            ])
        })
        .collect();
    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::ALL).title("Tabs"))
        .select(app.index)
        .style(Style::default().fg(Color::Cyan))
        .highlight_style(
            Style::default()
                .add_modifier(Modifier::BOLD)
                .bg(Color::Black),
        );
    f.render_widget(tabs, chunks[0]);

    let inner = match app.index {
        0 => view_log(logs.get(0).unwrap().to_string(), 0)?,
        1 => view_log(logs.get(1).unwrap().to_string(), 1)?,
        2 => view_log(logs.get(2).unwrap().to_string(), 2)?,
        3 => view_log(logs.get(3).unwrap().to_string(), 3)?,
        4 => view_log(logs.get(4).unwrap().to_string(), 4)?,
        _ => unreachable!(),
    };
    f.render_widget(inner, chunks[1]);
    Ok(())
}

fn view_log(line: String, node_num: u32) -> anyhow::Result<Paragraph<'static>> {
    let no_lines = line.as_bytes().iter().filter(|&&c| c == b'\n').count();
    let to_show = if no_lines > 35 {
        let mut lines: Vec<_> = line.lines().collect();
        lines.drain(0..no_lines - 34);
        lines.join("\n")
    } else {
        line
    };

    Ok(Paragraph::new(to_show)
        .style(Style::default().bg(Color::White).fg(Color::Black))
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: true })
        .block(
            Block::default()
                .title(format!("Node {:?}", node_num))
                .borders(Borders::ALL),
        ))
}
