use std::ops::{Add, AddAssign, Mul, Sub, SubAssign};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub const ZERO: Vec2 = Vec2 { x: 0.0, y: 0.0 };

    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub fn length_sq(self) -> f32 {
        self.x * self.x + self.y * self.y
    }

    pub fn length(self) -> f32 {
        self.length_sq().sqrt()
    }

    pub fn normalize(self) -> Vec2 {
        let len = self.length();
        if len > 0.0 {
            Vec2::new(self.x / len, self.y / len)
        } else {
            Vec2::ZERO
        }
    }

    pub fn dot(self, other: Vec2) -> f32 {
        self.x * other.x + self.y * other.y
    }
}

impl Add for Vec2 {
    type Output = Vec2;

    fn add(self, rhs: Vec2) -> Vec2 {
        Vec2::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl AddAssign for Vec2 {
    fn add_assign(&mut self, rhs: Vec2) {
        self.x += rhs.x;
        self.y += rhs.y;
    }
}

impl Sub for Vec2 {
    type Output = Vec2;

    fn sub(self, rhs: Vec2) -> Vec2 {
        Vec2::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl SubAssign for Vec2 {
    fn sub_assign(&mut self, rhs: Vec2) {
        self.x -= rhs.x;
        self.y -= rhs.y;
    }
}

impl Mul<f32> for Vec2 {
    type Output = Vec2;

    fn mul(self, rhs: f32) -> Vec2 {
        Vec2::new(self.x * rhs, self.y * rhs)
    }
}

impl Mul<Vec2> for f32 {
    type Output = Vec2;

    fn mul(self, rhs: Vec2) -> Vec2 {
        Vec2::new(self * rhs.x, self * rhs.y)
    }
}

pub type WordId = u64;

pub const TEXT_MAX_DRAW: usize = 12;
pub const TRAIL_LEN: usize = 10;

#[derive(Clone, Debug)]
pub struct Word {
    pub id: WordId,
    pub text: String,
    pub pos: Vec2,
    pub vel: Vec2,
    pub radius: f32,
    pub mass_total: f32,
    pub mass_visible: f32,
    pub mass_dust: f32,
    pub flags: WordFlags,
    pub trail: [Vec2; TRAIL_LEN],
    pub trail_head: usize,
    pub trail_len: usize,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct WordFlags {
    pub can_split: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorId {
    White,
    Cyan,
    Blue,
    Yellow,
    Magenta,
    Red,
    Gray,
    Trail,
    Spark,
}

#[derive(Clone, Copy, Debug)]
pub struct WordSnapshot {
    pub id: WordId,
    pub text: [char; TEXT_MAX_DRAW],
    pub text_len: usize,
    pub pos: Vec2,
    pub radius: f32,
    pub mass_visible: f32,
    pub mass_total: f32,
    pub mass_dust: f32,
    pub vel: Vec2,
    pub trail: [Vec2; TRAIL_LEN],
    pub trail_len: usize,
    pub trail_head: usize,
}

#[derive(Clone, Copy, Debug)]
pub struct EffectParticle {
    pub pos: Vec2,
    pub vel: Vec2,
    pub ttl: f32,
    pub glyph: char,
    pub color: ColorId,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct WorldStats {
    pub visible_count: usize,
    pub dust_count: usize,
    pub total_words: usize,
    pub total_mass_visible: f32,
    pub total_mass: f32,
    pub gravity_candidates_avg: f32,
    pub collision_candidates_avg: f32,
}
