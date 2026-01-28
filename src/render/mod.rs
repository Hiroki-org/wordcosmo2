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

#[cfg(test)]
mod tests {
    use super::*;

    mod camera {
        use super::*;

        #[test]
        fn default_camera_at_origin() {
            let camera = Camera::default();
            assert_eq!(camera.pos, Vec2::ZERO);
            assert_eq!(camera.zoom, 1.0);
        }
    }

    mod framebuffer {
        use super::*;

        mod new {
            use super::*;

            #[test]
            fn creates_with_correct_dimensions() {
                let fb = FrameBuffer::new(80, 24);
                assert_eq!(fb.width(), 80);
                assert_eq!(fb.height(), 24);
            }

            #[test]
            fn zero_dimensions_creates_empty_buffer() {
                let fb = FrameBuffer::new(0, 0);
                assert_eq!(fb.width(), 0);
                assert_eq!(fb.height(), 0);
            }
        }

        mod resize {
            use super::*;

            #[test]
            fn changes_dimensions() {
                let mut fb = FrameBuffer::new(10, 10);
                fb.resize(20, 15);
                assert_eq!(fb.width(), 20);
                assert_eq!(fb.height(), 15);
            }

            #[test]
            fn clears_cells_on_resize() {
                let mut fb = FrameBuffer::new(10, 10);
                fb.resize(10, 10);
                let cell = fb.get(0, 0);
                assert_eq!(cell.ch, ' ');
            }
        }

        mod clear {
            use super::*;

            #[test]
            fn resets_all_cells_to_space() {
                let mut fb = FrameBuffer::new(10, 10);
                fb.clear();
                for y in 0..10 {
                    for x in 0..10 {
                        let cell = fb.get(x, y);
                        assert_eq!(cell.ch, ' ');
                        assert_eq!(cell.color, ColorId::White);
                    }
                }
            }
        }

        mod set {
            use super::*;

            #[test]
            fn sets_cell_with_higher_mass() {
                let mut fb = FrameBuffer::new(10, 10);
                fb.set(5, 5, 'A', 10.0, ColorId::Blue);
                let cell = fb.get(5, 5);
                assert_eq!(cell.ch, 'A');
                assert_eq!(cell.color, ColorId::Blue);
            }

            #[test]
            fn does_not_overwrite_with_lower_mass() {
                let mut fb = FrameBuffer::new(10, 10);
                fb.set(5, 5, 'A', 10.0, ColorId::Blue);
                fb.set(5, 5, 'B', 5.0, ColorId::Red);
                let cell = fb.get(5, 5);
                assert_eq!(cell.ch, 'A');
            }

            #[test]
            fn out_of_bounds_is_ignored() {
                let mut fb = FrameBuffer::new(10, 10);
                fb.set(100, 100, 'X', 10.0, ColorId::Blue);
                // Should not panic
            }
        }
    }

    mod word_color_fn {
        use super::*;

        fn make_snapshot(mass_visible: f32, mass_total: f32, mass_dust: f32, vel: Vec2) -> WordSnapshot {
            WordSnapshot {
                id: 1,
                text: [' '; TEXT_MAX_DRAW],
                text_len: 0,
                pos: Vec2::ZERO,
                radius: 1.0,
                mass_visible,
                mass_total,
                mass_dust,
                vel,
                trail: [Vec2::ZERO; TRAIL_LEN],
                trail_len: 0,
                trail_head: 0,
            }
        }

        #[test]
        fn high_dust_ratio_returns_gray() {
            let word = make_snapshot(3.0, 10.0, 7.0, Vec2::ZERO);
            assert_eq!(word_color(&word), ColorId::Gray);
        }

        #[test]
        fn high_speed_returns_cyan() {
            let word = make_snapshot(10.0, 10.0, 0.0, Vec2::new(15.0, 0.0));
            assert_eq!(word_color(&word), ColorId::Cyan);
        }

        #[test]
        fn high_mass_returns_yellow() {
            let word = make_snapshot(25.0, 25.0, 0.0, Vec2::ZERO);
            assert_eq!(word_color(&word), ColorId::Yellow);
        }

        #[test]
        fn medium_high_mass_returns_magenta() {
            let word = make_snapshot(15.0, 15.0, 0.0, Vec2::ZERO);
            assert_eq!(word_color(&word), ColorId::Magenta);
        }

        #[test]
        fn medium_mass_returns_blue() {
            let word = make_snapshot(8.0, 8.0, 0.0, Vec2::ZERO);
            assert_eq!(word_color(&word), ColorId::Blue);
        }

        #[test]
        fn low_mass_returns_white() {
            let word = make_snapshot(3.0, 3.0, 0.0, Vec2::ZERO);
            assert_eq!(word_color(&word), ColorId::White);
        }

        #[test]
        fn zero_mass_total_does_not_panic() {
            let word = make_snapshot(0.0, 0.0, 0.0, Vec2::ZERO);
            // Should not panic
            let _ = word_color(&word);
        }
    }

    mod draw_fn {
        use super::*;

        #[test]
        fn empty_snapshot_produces_empty_frame() {
            let snapshot: Vec<WordSnapshot> = Vec::new();
            let effects: Vec<EffectParticle> = Vec::new();
            let camera = Camera::default();
            let viewport = Viewport { width: 80, height: 24 };
            let mut frame = FrameBuffer::new(80, 24);
            
            draw(&snapshot, &effects, None, &camera, viewport, &mut frame);
            
            for y in 0..24 {
                for x in 0..80 {
                    let cell = frame.get(x, y);
                    assert_eq!(cell.ch, ' ');
                }
            }
        }

        #[test]
        fn word_at_center_is_visible() {
            let mut text = [' '; TEXT_MAX_DRAW];
            text[0] = 'H';
            text[1] = 'i';
            let snapshot = vec![WordSnapshot {
                id: 1,
                text,
                text_len: 2,
                pos: Vec2::ZERO,
                radius: 1.0,
                mass_visible: 10.0,
                mass_total: 10.0,
                mass_dust: 0.0,
                vel: Vec2::ZERO,
                trail: [Vec2::ZERO; TRAIL_LEN],
                trail_len: 0,
                trail_head: 0,
            }];
            let effects: Vec<EffectParticle> = Vec::new();
            let camera = Camera::default();
            let viewport = Viewport { width: 80, height: 24 };
            let mut frame = FrameBuffer::new(80, 24);
            
            draw(&snapshot, &effects, None, &camera, viewport, &mut frame);
            
            let center_x = 40;
            let center_y = 12;
            let cell = frame.get(center_x, center_y);
            assert_eq!(cell.ch, 'H');
        }

        #[test]
        fn focused_word_is_red() {
            let mut text = [' '; TEXT_MAX_DRAW];
            text[0] = 'X';
            let snapshot = vec![WordSnapshot {
                id: 1,
                text,
                text_len: 1,
                pos: Vec2::ZERO,
                radius: 1.0,
                mass_visible: 10.0,
                mass_total: 10.0,
                mass_dust: 0.0,
                vel: Vec2::ZERO,
                trail: [Vec2::ZERO; TRAIL_LEN],
                trail_len: 0,
                trail_head: 0,
            }];
            let effects: Vec<EffectParticle> = Vec::new();
            let camera = Camera::default();
            let viewport = Viewport { width: 80, height: 24 };
            let mut frame = FrameBuffer::new(80, 24);
            
            draw(&snapshot, &effects, Some(1), &camera, viewport, &mut frame);
            
            let cell = frame.get(40, 12);
            assert_eq!(cell.color, ColorId::Red);
        }

        #[test]
        fn effect_overrides_word() {
            let mut text = [' '; TEXT_MAX_DRAW];
            text[0] = 'W';
            let snapshot = vec![WordSnapshot {
                id: 1,
                text,
                text_len: 1,
                pos: Vec2::ZERO,
                radius: 1.0,
                mass_visible: 10.0,
                mass_total: 10.0,
                mass_dust: 0.0,
                vel: Vec2::ZERO,
                trail: [Vec2::ZERO; TRAIL_LEN],
                trail_len: 0,
                trail_head: 0,
            }];
            let effects = vec![EffectParticle {
                pos: Vec2::ZERO,
                vel: Vec2::ZERO,
                ttl: 1.0,
                glyph: '*',
                color: ColorId::Yellow,
            }];
            let camera = Camera::default();
            let viewport = Viewport { width: 80, height: 24 };
            let mut frame = FrameBuffer::new(80, 24);
            
            draw(&snapshot, &effects, None, &camera, viewport, &mut frame);
            
            let cell = frame.get(40, 12);
            assert_eq!(cell.ch, '*');
        }
    }
}
