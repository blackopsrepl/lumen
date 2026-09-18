//! A tiny T-Rex runner for Lumen, in the shape of the real thing.
//!
//! It is deliberately self-contained: the only things it borrows from Lumen are
//! the backend and the input stream. Run it through the service with
//! `lumen ensure trex --ratatui <path-to-this-binary>`, or run it standalone and
//! it will notice it has no service and exit.
//!
//! Controls: `space` / `↑` to jump, `q` / `Esc` to quit.

use lumen_ratatui::{Event, Session};
use ratatui::style::{Color, Modifier, Style};
use ratatui::Frame;
use std::time::Duration;

/// One physics tick, and the frame cadence.
const TICK: Duration = Duration::from_millis(90);
const GRAVITY: i32 = 1;
const JUMP_VELOCITY: i32 = 3;
/// Randomness without a dependency: a 64-bit LCG stepped once per tick.
const SEED: u64 = 0x2545_f491_4f6c_dd1d;

struct Obstacle {
    x: i32,
}

struct App {
    dino_x: i32,
    /// Height above the ground: 0 is standing.
    dino_y: i32,
    velocity: i32,
    obstacles: Vec<Obstacle>,
    rng: u64,
    /// Ticks until the next obstacle may spawn.
    spawn_in: u32,
    score: u32,
    high: u32,
    game_over: bool,
}

impl App {
    fn new() -> Self {
        Self {
            dino_x: 4,
            dino_y: 0,
            velocity: 0,
            obstacles: Vec::new(),
            rng: SEED,
            spawn_in: 6,
            score: 0,
            high: 0,
            game_over: false,
        }
    }

    fn next_rand(&mut self) -> u64 {
        self.rng = self
            .rng
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.rng >> 33
    }

    fn jump(&mut self) {
        if self.game_over {
            self.reset();
            return;
        }
        if self.dino_y == 0 {
            self.velocity = JUMP_VELOCITY;
        }
    }

    fn reset(&mut self) {
        self.high = self.high.max(self.score);
        *self = Self {
            high: self.high,
            ..Self::new()
        };
    }

    fn quit_on_key(&self, event: &Event) -> bool {
        matches!(
            event,
            Event::Key(key)
                if matches!(key.code, lumen_ratatui::protocol::KeyCode::Char('q'))
                    || matches!(key.code, lumen_ratatui::protocol::KeyCode::Esc)
        )
    }

    fn on_key(&mut self, event: &Event) {
        if let Event::Key(key) = event {
            use lumen_ratatui::protocol::KeyCode;
            match key.code {
                KeyCode::Char(' ') | KeyCode::Up => self.jump(),
                _ => {}
            }
        }
    }

    fn tick(&mut self) {
        if self.game_over {
            return;
        }
        self.score += 1;

        // Jump physics.
        if self.velocity != 0 || self.dino_y > 0 {
            self.dino_y += self.velocity;
            self.velocity -= GRAVITY;
            if self.dino_y <= 0 {
                self.dino_y = 0;
                self.velocity = 0;
            }
        }

        // Obstacles drift left; spawn with a randomised gap.
        for obstacle in &mut self.obstacles {
            obstacle.x -= 1;
        }
        self.obstacles.retain(|obstacle| obstacle.x > -2);
        self.spawn_in = self.spawn_in.saturating_sub(1);
        if self.spawn_in == 0 {
            self.obstacles.push(Obstacle { x: 100 });
            self.spawn_in = 5 + (self.next_rand() % 9) as u32;
        }

        // A collision only counts while the dino is on the ground.
        if self.dino_y == 0
            && self
                .obstacles
                .iter()
                .any(|obstacle| (self.dino_x..self.dino_x + 2).contains(&obstacle.x))
        {
            self.game_over = true;
            self.high = self.high.max(self.score);
        }
    }

    fn render(&self, frame: &mut Frame) {
        let area = frame.area();
        let buffer = frame.buffer_mut();
        let width = area.width as i32;
        let ground = area.height.saturating_sub(1) as i32;
        let dino_x = self.dino_x.clamp(0, width - 1);
        let dino_row = (ground - 1 - self.dino_y).max(0) as u16;

        buffer.set_string(
            0,
            ground as u16,
            "─".repeat(area.width as usize),
            Style::default().fg(Color::DarkGray),
        );

        for obstacle in &self.obstacles {
            if (0..width).contains(&obstacle.x) {
                buffer.set_string(
                    obstacle.x as u16,
                    (ground - 1).max(0) as u16,
                    "🌵",
                    Style::default().fg(Color::Green),
                );
            }
        }

        buffer.set_string(
            dino_x as u16,
            dino_row,
            "🦖",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        );

        buffer.set_string(
            2,
            0,
            format!(
                "score {}   high {}   {:<12}",
                self.score,
                self.high,
                if self.game_over { "GAME OVER" } else { "" }
            ),
            Style::default().fg(Color::Yellow),
        );

        if self.game_over {
            let hint = "space to run again · q to quit";
            let x = width.saturating_sub(hint.len() as i32) / 2;
            buffer.set_string(
                x.max(0) as u16,
                (area.height / 2).max(1) as u16,
                hint,
                Style::default()
                    .fg(Color::LightRed)
                    .add_modifier(Modifier::BOLD),
            );
        }
    }
}

fn main() -> std::io::Result<()> {
    // A standalone binary must not hang waiting for the service's Size frame.
    if std::env::var_os("LUMEN_COLS").is_none() {
        eprintln!("trex: no Lumen session detected; run it through the service");
        std::process::exit(1);
    }

    let mut session = Session::connect()?;
    let mut app = App::new();

    loop {
        session.draw(|frame| app.render(frame))?;

        match session.poll(TICK) {
            Some(Event::Quit) => break,
            Some(event) if app.quit_on_key(&event) => break,
            Some(event) => app.on_key(&event),
            None => {}
        }
        app.tick();
    }

    Ok(())
}
