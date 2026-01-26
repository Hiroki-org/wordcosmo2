use crate::types::{
    ColorId, EffectParticle, Vec2, WordId, WordSnapshot, TEXT_MAX_DRAW, TRAIL_LEN,
};

#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub pos: Vec2,
    pub zoom: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            pos: Vec2::ZERO,
            zoom: 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Viewport {
    pub width: u16,
    pub height: u16,
}

#[derive(Clone, Copy, Debug)]
pub struct RenderCell {
    pub ch: char,
    pub mass: f32,
    pub color: ColorId,
}

#[derive(Debug)]
pub struct FrameBuffer {
    width: u16,
    height: u16,
    cells: Vec<RenderCell>,
}

impl FrameBuffer {
    pub fn new(width: u16, height: u16) -> Self {
        let mut buffer = Self {
            width,
            height,
            cells: Vec::new(),
        };
        buffer.resize(width, height);
        buffer
    }

    pub fn resize(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;
        let len = (width as usize).saturating_mul(height as usize);
        if self.cells.len() != len {
            self.cells.resize(
                len,
                RenderCell {
                    ch: ' ',
                    mass: f32::NEG_INFINITY,
                    color: ColorId::White,
                },
            );
        }
        self.clear();
    }

    pub fn clear(&mut self) {
        for cell in &mut self.cells {
            cell.ch = ' ';
            cell.mass = f32::NEG_INFINITY;
            cell.color = ColorId::White;
        }
    }

    pub fn width(&self) -> u16 {
        self.width
    }

    pub fn height(&self) -> u16 {
        self.height
    }

    pub fn get(&self, x: u16, y: u16) -> RenderCell {
        debug_assert!(x < self.width && y < self.height, "get() out of bounds");
        let idx = (y as usize) * (self.width as usize) + (x as usize);
        self.cells[idx]
    }

    fn set(&mut self, x: u16, y: u16, ch: char, mass: f32, color: ColorId) {
        if x >= self.width || y >= self.height {
            return;
        }
        let idx = (y as usize) * (self.width as usize) + (x as usize);
        let cell = &mut self.cells[idx];
        if mass >= cell.mass {
            cell.mass = mass;
            cell.ch = ch;
            cell.color = color;
        }
    }
}

pub fn draw(
    snapshot: &[WordSnapshot],
    effects: &[EffectParticle],
    focus_word_id: Option<WordId>,
    camera: &Camera,
    viewport: Viewport,
    frame: &mut FrameBuffer,
) {
    if frame.width() != viewport.width || frame.height() != viewport.height {
        frame.resize(viewport.width, viewport.height);
    } else {
        frame.clear();
    }

    let half_w = viewport.width as f32 / 2.0;
    let half_h = viewport.height as f32 / 2.0;

    for word in snapshot {
        draw_trail(word, camera, viewport, frame, half_w, half_h);
    }

    for word in snapshot {
        let sx = ((word.pos.x - camera.pos.x) * camera.zoom + half_w).round() as i32;
        let sy = ((word.pos.y - camera.pos.y) * camera.zoom + half_h).round() as i32;
        if sy < 0 || sy >= viewport.height as i32 {
            continue;
        }

        let color = if focus_word_id == Some(word.id) {
            ColorId::Red
        } else {
            word_color(word)
        };
        let mut text_len = word.text_len.min(TEXT_MAX_DRAW);
        if word.text_len > TEXT_MAX_DRAW && text_len > 0 && word.text[text_len - 1] == '-' {
            text_len -= 1;
        }
        for i in 0..text_len {
            let x = sx + i as i32;
            if x < 0 || x >= viewport.width as i32 {
                continue;
            }
            let ux = x as u16;
            let uy = sy as u16;
            let ch = word.text[i];
            frame.set(ux, uy, ch, word.mass_visible, color);
        }
    }

    for effect in effects {
        let sx = ((effect.pos.x - camera.pos.x) * camera.zoom + half_w).round() as i32;
        let sy = ((effect.pos.y - camera.pos.y) * camera.zoom + half_h).round() as i32;
        if sx >= 0 && sy >= 0 {
            let ux = sx as u16;
            let uy = sy as u16;
            if ux < viewport.width && uy < viewport.height {
                frame.set(ux, uy, effect.glyph, 1.0e9, effect.color);
            }
        }
    }
}

fn draw_trail(
    word: &WordSnapshot,
    camera: &Camera,
    viewport: Viewport,
    frame: &mut FrameBuffer,
    half_w: f32,
    half_h: f32,
) {
    if word.trail_len == 0 {
        return;
    }
    let max_len = word.trail_len.min(TRAIL_LEN);
    for i in 0..max_len {
        // リングバッファを最新から古い順にアクセス
        let idx = (word.trail_head + TRAIL_LEN - i) % TRAIL_LEN;
        let pos = word.trail[idx];
        let sx = ((pos.x - camera.pos.x) * camera.zoom + half_w).round() as i32;
        let sy = ((pos.y - camera.pos.y) * camera.zoom + half_h).round() as i32;
        if sx < 0 || sy < 0 || sx >= viewport.width as i32 || sy >= viewport.height as i32 {
            continue;
        }
        let age = i as f32 / max_len as f32;
        let ch = if age < 0.4 { '·' } else { '.' };
        let mass = word.mass_visible * (0.3 * (1.0 - age));
        frame.set(sx as u16, sy as u16, ch, mass, ColorId::Trail);
    }
}

fn word_color(word: &WordSnapshot) -> ColorId {
    let dust_ratio = if word.mass_total > 0.0 {
        (word.mass_dust / word.mass_total).min(1.0)
    } else {
        0.0
    };
    let speed = word.vel.length();
    if dust_ratio > 0.6 {
        ColorId::Gray
    } else if speed > 14.0 {
        ColorId::Cyan
    } else if word.mass_visible > 20.0 {
        ColorId::Yellow
    } else if word.mass_visible > 10.0 {
        ColorId::Magenta
    } else if word.mass_visible > 6.0 {
        ColorId::Blue
    } else {
        ColorId::White
    }
}
