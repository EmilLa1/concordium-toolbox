use anyhow::Context;
use clap::AppSettings;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::io::{self, BufRead, BufReader};
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
        long = "well-connected",
        help = "If every node should be connected to eachother or they should be connected sequentially"
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
        default_value = "30"
    )]
    housekeeping_interval: usize,
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
    let node_path = std::path::PathBuf::from("../deps/concordium-node/concordium-node/")
        .canonicalize()
        .context("invalid node path")?;
    for i in 0..cfg.num_nodes {
        log_buffers.push(String::new());

        // command for creating the baker folder
        let mut mkdir_cmd = Command::new("mkdir");
        mkdir_cmd.current_dir(&node_path);
        mkdir_cmd.arg("-p");
        mkdir_cmd.arg(format!("baker-{}", i));
        mkdir_cmd
            .status()
            .context("cannot create baker directory")?;

        //copy genesis.dat to baker directory cp $GENESIS_ROOT/genesis.dat
        let mut copy_cmd = Command::new("cp");
        copy_cmd.current_dir(&node_path);

        let genesis_dat = genesis_root
            .join("genesis.dat")
            .canonicalize()
            .context("cannot find genesis.dat")?;
        copy_cmd.arg(genesis_dat.to_str().unwrap());
        copy_cmd.arg(format!("baker-{}", i));
        copy_cmd.status().context("cannot copy genesis.dat")?;

        // command for running the node
        let cmd = &mut Command::new("cargo");
        cmd.env("RUST_BACKTRACE", "full");
        cmd.current_dir(&node_path);

        cmd.arg("run");
        cmd.arg("--release");
        cmd.arg("--quiet");
        cmd.arg("--");
        cmd.arg("--no-bootstrap 1");
        cmd.arg(format!("--id {:?}", i));
        cmd.arg(format!("--config-dir baker-{:?}", i));
        cmd.arg(format!("--data-dir baker-{:?}", i));

        let baker_credentials = genesis_root
            .join(format!("bakers/baker-{}-credentials.json", i))
            .canonicalize()
            .context("Invalid baker credentials")?;

        cmd.arg(format!(
            "--baker-credentials-file {}",
            baker_credentials.to_str().unwrap()
        ));
        cmd.arg(format!("--rpc-server-port {:?}", i + cfg.rpc_port_offset));
        cmd.arg(format!("--listen-port {:?}", i + cfg.peer_port_offset));
        cmd.arg("--listen-address 0.0.0.0");
        cmd.arg(format!("--haskell-rts-flags {:?}", cfg.rts_flags));
        cmd.arg(format!(
            "--housekeeping-interval {:?}",
            cfg.housekeeping_interval
        ));
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        // every node is connected to eachother
        if cfg.optimal_connected {
            cmd.arg(format!("--desired-nodes {:?}", cfg.num_nodes - 1));
            for n in 0..cfg.num_nodes {
                if i == n {
                    continue;
                }
                cmd.arg(format!(
                    "--connect-to 127.0.0.1:{:?}",
                    n + cfg.peer_port_offset
                ));
            }
        } else {
            // the nodes will be connected sequentially.
            // O - O - O - O - ...
            cmd.arg(format!("--desired-nodes {:?}", 1));
            cmd.arg(format!("--max-allowed-nodes {:?}", 1));
            let next_peer_port = if i == cfg.num_nodes {
                cfg.num_nodes - 1
            } else {
                i + 1
            };
            cmd.arg(format!("--connect-to 127.0.0.1:{:?}", next_peer_port));
        }

        let mut fork = cmd
            .spawn()
            .context(format!("Failed to launch node {:?}", i))?;

        let mut buf_reader = BufReader::new(fork.stderr.take().context("Could not take stdout")?);
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
                sender
                    .send(buffered_line)
                    .await
                    .context("mpsc sender failed")
                    .unwrap();
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
    Ok(Paragraph::new(line)
        .style(Style::default().bg(Color::White).fg(Color::Black))
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: true })
        .block(
            Block::default()
                .title(format!("Node {:?}", node_num))
                .borders(Borders::ALL),
        ))
}
