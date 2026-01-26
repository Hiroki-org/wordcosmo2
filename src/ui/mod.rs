use std::{cmp::Ordering, collections::HashMap, error::Error, io, mem, time::Duration};

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
    types::{ColorId, EffectParticle, Vec2, WordId, WordSnapshot},
};

pub fn run() -> Result<(), Box<dyn Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let result: Result<(), Box<dyn Error>> = (|| {
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

            let mut events_processed = 0;
            while events_processed < 100 && event::poll(Duration::from_millis(0))? {
                events_processed += 1;
                if let CrosstermEvent::Key(key) = event::read()? {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
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
                        KeyCode::Char('f') => {
                            let candidates = build_focus_candidates_from_world(&world);
                            ui_state.advance_focus(&candidates);
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
                let focus_candidates = build_focus_candidates_from_world(&world);
                ui_state.sync_focus(&focus_candidates);
                let focus_info = ui_state.update_camera_from_focus(&world, &focus_candidates);
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
                            Constraint::Length(5),
                            Constraint::Min(3),
                            Constraint::Length(3),
                        ])
                        .split(size);

                    let debug = stats.gravity_debug;
                    let debug_line = if debug.sample_index >= 0 {
                        format!(
                            "grav dbg: cand {} -> {} | |a| {:.3} | |dv| {:.3} | r_near {:.2} | cut:{} | m_near {:.2} | subvis:{}",
                            debug.candidates,
                            debug.candidates_after_cutoff,
                            debug.acc_mag,
                            debug.dv_mag,
                            debug.sample_r,
                            if debug.sample_cutoff_rejected { "yes" } else { "no" },
                            debug.sample_other_mass_visible,
                            if debug.sample_other_subvisible { "yes" } else { "no" }
                        )
                    } else {
                        "grav dbg: none".to_string()
                    };

                    let header = Paragraph::new(format!(
                        "visible: {} | dust: {} | total: {} | m_vis: {:.1} | m_total: {:.1} | gCand: {:.1} | cCand: {:.1} | sim fps: {:.1} | render fps: {:.1}\n{}\n{}",
                        stats.visible_count,
                        stats.dust_count,
                        stats.total_words,
                        stats.total_mass_visible,
                        stats.total_mass,
                        stats.gravity_candidates_avg,
                        stats.collision_candidates_avg,
                        sim_fps,
                        render_fps,
                        debug_line,
                        focus_info
                    ))
                    .block(Block::default().borders(Borders::ALL).title("wordcosmo2"));
                    frame.render_widget(header, chunks[0]);

                    ui_state.ensure_viewport(chunks[1].width, chunks[1].height);
                    render::draw(
                        &snapshot,
                        &effects,
                        ui_state.focus_word_id,
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
                    let lines: Vec<Line> = (0..height)
                        .map(|y| {
                            let mut spans: Vec<Span> = Vec::new();
                            if width == 0 {
                                return Line::from(spans);
                            }
                            let mut current_text = String::with_capacity(width as usize);
                            let mut current_color = framebuf.get(0, y).color;
                            for x in 0..width {
                                let cell = framebuf.get(x, y);
                                if cell.color == current_color {
                                    current_text.push(cell.ch);
                                } else {
                                    spans.push(Span::styled(
                                        mem::take(&mut current_text),
                                        Style::default().fg(color_for(current_color)),
                                    ));
                                    current_text.push(cell.ch);
                                    current_color = cell.color;
                                }
                            }
                            if !current_text.is_empty() {
                                spans.push(Span::styled(
                                    current_text,
                                    Style::default().fg(color_for(current_color)),
                                ));
                            }
                            Line::from(spans)
                        })
                        .collect();

                    let viewport = Paragraph::new(lines)
                        .block(Block::default().borders(Borders::ALL).title("Viewport"));
                    frame.render_widget(viewport, chunks[1]);

                    let footer = Paragraph::new(format!(
                        "input: {} | mass_total: {:.1} | ↑↓: mass | Enter: spawn | f: focus next | SUN: create sun | q: quit",
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
    })();

    shutdown_terminal(&mut terminal)?;
    result
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
    input: String,
    mass_total: f32,
    focus_component: Option<String>,
    focus_word_id: Option<WordId>,
    focus_index: usize,
    focus_total: usize,
}

impl UiState {
    fn new() -> Self {
        Self {
            camera: render::Camera::default(),
            framebuf: render::FrameBuffer::new(0, 0),
            input: String::new(),
            mass_total: 10.0,
            focus_component: None,
            focus_word_id: None,
            focus_index: 0,
            focus_total: 0,
        }
    }

    fn ensure_viewport(&mut self, width: u16, height: u16) {
        if self.framebuf.width() != width || self.framebuf.height() != height {
            self.framebuf.resize(width, height);
        }
    }

    fn advance_focus(&mut self, candidates: &[FocusCandidate]) {
        if candidates.is_empty() {
            self.focus_component = None;
            self.focus_word_id = None;
            self.focus_index = 0;
            self.focus_total = 0;
            return;
        }
        let next_index = self
            .focus_component
            .as_deref()
            .and_then(|key| candidates.iter().position(|c| c.component == key))
            .map(|idx| (idx + 1) % candidates.len())
            .unwrap_or(0);
        let next = &candidates[next_index];
        self.focus_component = Some(next.component.clone());
        self.focus_word_id = Some(next.word_id);
        self.focus_index = next_index + 1;
        self.focus_total = candidates.len();
    }

    fn sync_focus(&mut self, candidates: &[FocusCandidate]) {
        if candidates.is_empty() {
            self.focus_component = None;
            self.focus_word_id = None;
            self.focus_index = 0;
            self.focus_total = 0;
            return;
        }
        self.focus_total = candidates.len();
        if let Some(component) = self.focus_component.as_deref() {
            if let Some((idx, candidate)) = candidates
                .iter()
                .enumerate()
                .find(|(_, c)| c.component == component)
            {
                self.focus_index = idx + 1;
                self.focus_word_id = Some(candidate.word_id);
                return;
            }
        }
        self.focus_component = None;
        self.focus_word_id = None;
        self.focus_index = 0;
    }

    fn update_camera_from_focus(
        &mut self,
        world: &World,
        candidates: &[FocusCandidate],
    ) -> String {
        let Some(component) = self.focus_component.as_deref() else {
            return format!("focus: none");
        };
        let candidate = candidates.iter().find(|c| c.component == component);
        let Some(candidate) = candidate else {
            self.focus_component = None;
            self.focus_word_id = None;
            self.focus_index = 0;
            self.focus_total = 0;
            return format!("focus: none");
        };
        self.focus_word_id = Some(candidate.word_id);
        let Some(word) = world.words.iter().find(|w| w.id == candidate.word_id) else {
            self.focus_word_id = None;
            return format!("focus: none");
        };
        let target = word.pos;
        self.camera.pos = lerp_vec2(self.camera.pos, target, 0.2);
        let text = display_text(&word.text);
        format!(
            "focus: {}/{} | key={} | id={} | mass={:.2} | text={} ",
            self.focus_index,
            self.focus_total,
            component,
            word.id,
            word.mass_visible,
            text
        )
    }
}

fn lerp_vec2(a: Vec2, b: Vec2, alpha: f32) -> Vec2 {
    a + (b - a) * alpha
}

#[derive(Clone, Debug)]
struct FocusCandidate {
    component: String,
    word_id: WordId,
    mass_visible: f32,
}

fn build_focus_candidates_from_world(world: &World) -> Vec<FocusCandidate> {
    let mut map: HashMap<String, (WordId, f32)> = HashMap::new();
    for word in &world.words {
        if word.mass_visible < config::MIN_VISIBLE_MASS {
            continue;
        }
        let components = split_components(&word.text);
        for component in components {
            let entry = map.entry(component).or_insert((word.id, word.mass_visible));
            let (best_id, best_mass) = entry;
            if word.mass_visible > *best_mass
                || (word.mass_visible == *best_mass && word.id < *best_id)
            {
                *best_id = word.id;
                *best_mass = word.mass_visible;
            }
        }
    }

    let mut items: Vec<FocusCandidate> = map
        .into_iter()
        .map(|(component, (word_id, mass_visible))| FocusCandidate {
            component,
            word_id,
            mass_visible,
        })
        .collect();

    items.sort_by(|a, b| {
        b.mass_visible
            .partial_cmp(&a.mass_visible)
            .unwrap_or(Ordering::Equal)
            .then_with(|| a.word_id.cmp(&b.word_id))
            .then_with(|| a.component.cmp(&b.component))
    });

    items
}

fn split_components(text: &str) -> Vec<String> {
    text.split(config::WORD_JOIN_SEP)
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

fn display_text(text: &str) -> String {
    text.chars()
        .map(|ch| if ch == config::WORD_JOIN_SEP { '-' } else { ch })
        .collect()
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
