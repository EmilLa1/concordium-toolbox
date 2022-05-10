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
        long = "num_nodes",
        help = "The number of nodes to spawn",
        default_value = "5"
    )]
    num_nodes: usize,
    #[structopt(
        long = "well_connected",
        help = "If every node should be connected to eachother or they should be connected sequentially"
    )]
    well_connected: bool,
}

struct App<'a> {
    pub titles: Vec<&'a str>,
    pub index: usize,
}

impl<'a> App<'a> {
    fn new() -> App<'a> {
        App {
            titles: vec!["Node0", "Node1", "Node2", "Node3", "Node4"],
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

    // create app and run it
    let app = App::new();
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
    let mut stdout_receivers = vec![];
    let mut log_buffers = vec![];
    for i in 0..cfg.num_nodes {
        log_buffers.push(String::new());
        let mut fork = Command::new("./run-node-local.sh")
            .arg(i.to_string())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context(format!("Failed to launch node {:?}", i))?;

        let mut buf_reader = BufReader::new(fork.stderr.take().context("Could not take stdout")?);
        // create a channel for reading stdout of the forked process.
        let (sender, receiver) = tokio::sync::mpsc::channel(100);
        stdout_receivers.push(receiver);
        let reader = async move {
            loop {
                let mut buffered_line = String::new();
                for _ in 0..10 {
                    buf_reader.read_line(&mut buffered_line).unwrap();
                }
                sender.send(buffered_line).await.unwrap();
            }
        };
        tokio::spawn(reader);
    }

    // run until someone presses `q`.
    loop {
        // append to the logs
        for i in 0..cfg.num_nodes {
            if let Ok(log) = stdout_receivers.get_mut(i).unwrap().try_recv() {
                log_buffers.get_mut(i).unwrap().push_str(&log);
            };
        }
        // draw the ui
        terminal.draw(|f| ui(f, &app, &log_buffers).unwrap())?;
        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Char('q') => {
                    // todo: kill the underlying process both when pressing `q` and when sigint
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
