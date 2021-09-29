#![allow(dead_code)]
#![allow(unused_imports)]
mod util;

use crate::util::{
    StatefulList,
};

use ryts::*;
use std::{
	error::Error,
    io,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event as CEvent, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use tui::{
	Terminal, 
	widgets::{Widget, Block, Borders, List, ListItem, ListState},
	backend::CrosstermBackend, layout::{Layout, Constraint, Direction,},
	style::{Color,Style,Modifier}
};

enum Event<I>{
    Input(I),
    Tick,
}

struct App<'a> {
    items: StatefulList<(&'a str, usize)>,
    events: Vec<(&'a str, &'a str)>,
}

#[tokio::main]
async fn main()-> Result<(), io::Error> {
	let stdout = io::stdout();
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
	let (tx, rx) = mpsc::channel();

    let tick_rate = Duration::from_millis(250);
    terminal.clear()?;
    thread::spawn(move || {
        let mut last_tick = Instant::now();
        loop {
            // poll for tick rate duration, if no events, sent tick event.
            let timeout = tick_rate
                .checked_sub(last_tick.elapsed())
                .unwrap_or_else(|| Duration::from_secs(0));
            if event::poll(timeout).unwrap() {
                if let CEvent::Key(key) = event::read().unwrap() {
                    tx.send(Event::Input(key)).unwrap();
                }
            }
            if last_tick.elapsed() >= tick_rate {
                tx.send(Event::Tick).unwrap();
                last_tick = Instant::now();
            }
        }
    });
    
  	terminal.draw(|f| {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints(
                [
                    Constraint::Percentage(10),
                    Constraint::Percentage(80),
                    Constraint::Percentage(10)
                ].as_ref()
            )
            .split(f.size());
		let items = [ListItem::new("Item 1"), ListItem::new("Item 2"), ListItem::new("Item 3")];
		let list =	List::new(items)
			.block(Block::default().title("List").borders(Borders::ALL))
			.style(Style::default().fg(Color::White))
			.highlight_style(Style::default().add_modifier(Modifier::ITALIC))
			.highlight_symbol(">>");
		f.render_widget(list, chunks[1]);
    }) 
}
