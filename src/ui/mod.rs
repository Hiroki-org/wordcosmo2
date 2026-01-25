use std::{error::Error, io, time::Duration};

use crossterm::{
    event::{self, Event as CrosstermEvent, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Terminal,
};

use crate::{
    config,
    core::World,
    render,
    types::{ColorId, EffectParticle, WordSnapshot},
};

pub fn run() -> Result<(), Box<dyn Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut world = World::new();
    let mut snapshot: Vec<WordSnapshot> = Vec::with_capacity(config::K_VISIBLE_MAX);
    let mut ui_state = UiState::new();
    let mut effects: Vec<EffectParticle> = Vec::with_capacity(config::EFFECT_CAPACITY);

    let mut accumulator = 0.0_f32;
    let mut last_tick = std::time::Instant::now();
    let mut last_render = std::time::Instant::now();
    let render_interval = Duration::from_secs_f32(1.0 / config::RENDER_HZ);
    let mut sim_counter = 0_u32;
    let mut render_counter = 0_u32;
    let mut last_fps_sample = std::time::Instant::now();
    let mut sim_fps = 0.0_f32;
    let mut render_fps = 0.0_f32;

    loop {
        let now = std::time::Instant::now();
        let dt = (now - last_tick).as_secs_f32();
        last_tick = now;
        accumulator += dt;

        while accumulator >= config::DT {
            world.tick(config::DT);
            accumulator -= config::DT;
            sim_counter += 1;
        }

        while event::poll(Duration::from_millis(0))? {
            if let CrosstermEvent::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => {
                        shutdown_terminal(&mut terminal)?;
                        return Ok(());
                    }
                    KeyCode::Up => {
                        ui_state.mass_total = (ui_state.mass_total + 1.0).min(100.0);
                    }
                    KeyCode::Down => {
                        ui_state.mass_total = (ui_state.mass_total - 1.0).max(1.0);
                    }
                    KeyCode::Backspace => {
                        ui_state.input.pop();
                    }
                    KeyCode::Enter => {
                        let text = ui_state.input.trim().to_string();
                        if !text.is_empty() {
                            if text.eq_ignore_ascii_case("sun") {
                                world.set_sun(ui_state.camera.pos);
                            } else {
                                world.add_word(text, ui_state.mass_total, ui_state.camera.pos);
                            }
                        }
                        ui_state.input.clear();
                    }
                    KeyCode::Char(ch) => {
                        if !ch.is_control() && ui_state.input.len() < 32 {
                            ui_state.input.push(ch);
                        }
                    }
                    _ => {}
                }
            }
        }

        if last_render.elapsed() >= render_interval {
            world.snapshot(&mut snapshot);
            world.effects_snapshot(&mut effects);
            let stats = world.stats();
            if last_fps_sample.elapsed() >= Duration::from_secs(1) {
                let secs = last_fps_sample.elapsed().as_secs_f32();
                sim_fps = sim_counter as f32 / secs;
                render_fps = render_counter as f32 / secs;
                sim_counter = 0;
                render_counter = 0;
                last_fps_sample = std::time::Instant::now();
            }
            terminal.draw(|frame| {
                let size = frame.size();
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(3),
                        Constraint::Min(3),
                        Constraint::Length(3),
                    ])
                    .split(size);

                let header = Paragraph::new(format!(
                    "visible: {} | dust: {} | total: {} | m_vis: {:.1} | m_total: {:.1} | gCand: {:.1} | cCand: {:.1} | sim fps: {:.1} | render fps: {:.1}",
                    stats.visible_count,
                    stats.dust_count,
                    stats.total_words,
                    stats.total_mass_visible,
                    stats.total_mass,
                    stats.gravity_candidates_avg,
                    stats.collision_candidates_avg,
                    sim_fps,
                    render_fps
                ))
                .block(Block::default().borders(Borders::ALL).title("wordcosmo2"));
                frame.render_widget(header, chunks[0]);

                ui_state.ensure_viewport(chunks[1].width, chunks[1].height);
                render::draw(
                    &snapshot,
                    &effects,
                    &ui_state.camera,
                    render::Viewport {
                        width: chunks[1].width,
                        height: chunks[1].height,
                    },
                    &mut ui_state.framebuf,
                );

                let framebuf = &ui_state.framebuf;
                let width = framebuf.width();
                let height = framebuf.height();
                {
                    let lines_store = &mut ui_state.lines;
                    for y in 0..height {
                        let line = &mut lines_store[y as usize];
                        line.clear();
                        line.reserve(width as usize);
                        for x in 0..width {
                            let cell = framebuf.get(x, y);
                            line.push(cell.ch);
                        }
                    }
                }
                let lines: Vec<Line> = ui_state
                    .lines
                    .iter()
                    .enumerate()
                    .map(|(y, line)| {
                        let mut spans: Vec<Span> = Vec::with_capacity(line.len());
                        for (x, ch) in line.chars().enumerate() {
                            let cell = framebuf.get(x as u16, y as u16);
                            let color = color_for(cell.color);
                            spans.push(Span::styled(ch.to_string(), Style::default().fg(color)));
                        }
                        Line::from(spans)
                    })
                    .collect();

                let viewport = Paragraph::new(lines)
                    .block(Block::default().borders(Borders::ALL).title("Viewport"));
                frame.render_widget(viewport, chunks[1]);

                let footer = Paragraph::new(format!(
                    "input: {} | mass_total: {:.1} | ↑↓: mass | Enter: spawn | SUN: create sun | q: quit",
                    ui_state.input, ui_state.mass_total
                ))
                    .block(Block::default().borders(Borders::ALL).title("Controls"));
                frame.render_widget(footer, chunks[2]);
            })?;

            last_render = std::time::Instant::now();
            render_counter += 1;
        }

        std::thread::sleep(Duration::from_millis(1));
    }
}

fn shutdown_terminal(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) -> Result<(), Box<dyn Error>> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

struct UiState {
    camera: render::Camera,
    framebuf: render::FrameBuffer,
    lines: Vec<String>,
    input: String,
    mass_total: f32,
}

impl UiState {
    fn new() -> Self {
        Self {
            camera: render::Camera::default(),
            framebuf: render::FrameBuffer::new(0, 0),
            lines: Vec::new(),
            input: String::new(),
            mass_total: 10.0,
        }
    }

    fn ensure_viewport(&mut self, width: u16, height: u16) {
        if self.framebuf.width() != width || self.framebuf.height() != height {
            self.framebuf.resize(width, height);
        }
        let desired = height as usize;
        if self.lines.len() != desired {
            self.lines.clear();
            self.lines.resize_with(desired, String::new);
        }
    }

    #[allow(dead_code)]
    fn line_mut(&mut self, idx: usize) -> &mut String {
        &mut self.lines[idx]
    }
}

fn color_for(color: ColorId) -> Color {
    match color {
        ColorId::White => Color::White,
        ColorId::Cyan => Color::Cyan,
        ColorId::Blue => Color::Blue,
        ColorId::Yellow => Color::Yellow,
        ColorId::Magenta => Color::Magenta,
        ColorId::Red => Color::Red,
        ColorId::Gray => Color::DarkGray,
        ColorId::Trail => Color::DarkGray,
        ColorId::Spark => Color::LightYellow,
    }
}
