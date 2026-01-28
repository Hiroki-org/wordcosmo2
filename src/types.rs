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

pub const TEXT_MAX_DRAW: usize = 120;
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
    pub gravity_debug: GravityDebugStats,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct GravityDebugStats {
    pub sample_index: i32,
    pub candidates: usize,
    pub candidates_after_cutoff: usize,
    pub acc_mag: f32,
    pub dv_mag: f32,
    pub sample_r: f32,
    pub sample_cutoff_rejected: bool,
    pub sample_other_mass_visible: f32,
    pub sample_other_subvisible: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    mod vec2_new {
        use super::*;

        #[test]
        fn creates_vector_with_given_coordinates() {
            let v = Vec2::new(3.0, 4.0);
            assert_eq!(v.x, 3.0);
            assert_eq!(v.y, 4.0);
        }

        #[test]
        fn zero_constant_is_origin() {
            assert_eq!(Vec2::ZERO.x, 0.0);
            assert_eq!(Vec2::ZERO.y, 0.0);
        }
    }

    mod vec2_length {
        use super::*;

        #[test]
        fn calculates_length_squared() {
            let v = Vec2::new(3.0, 4.0);
            assert_eq!(v.length_sq(), 25.0);
        }

        #[test]
        fn calculates_length() {
            let v = Vec2::new(3.0, 4.0);
            assert_eq!(v.length(), 5.0);
        }

        #[test]
        fn zero_vector_has_zero_length() {
            assert_eq!(Vec2::ZERO.length(), 0.0);
        }
    }

    mod vec2_normalize {
        use super::*;

        #[test]
        fn normalizes_non_zero_vector() {
            let v = Vec2::new(3.0, 4.0).normalize();
            let expected_x = 3.0 / 5.0;
            let expected_y = 4.0 / 5.0;
            assert!((v.x - expected_x).abs() < 1e-6);
            assert!((v.y - expected_y).abs() < 1e-6);
            assert!((v.length() - 1.0).abs() < 1e-6);
        }

        #[test]
        fn zero_vector_normalizes_to_zero() {
            let v = Vec2::ZERO.normalize();
            assert_eq!(v, Vec2::ZERO);
        }
    }

    mod vec2_dot {
        use super::*;

        #[test]
        fn calculates_dot_product() {
            let a = Vec2::new(2.0, 3.0);
            let b = Vec2::new(4.0, 5.0);
            assert_eq!(a.dot(b), 23.0); // 2*4 + 3*5 = 8 + 15 = 23
        }

        #[test]
        fn dot_product_with_zero_is_zero() {
            let a = Vec2::new(2.0, 3.0);
            assert_eq!(a.dot(Vec2::ZERO), 0.0);
        }

        #[test]
        fn perpendicular_vectors_have_zero_dot_product() {
            let a = Vec2::new(1.0, 0.0);
            let b = Vec2::new(0.0, 1.0);
            assert_eq!(a.dot(b), 0.0);
        }
    }

    mod vec2_add {
        use super::*;

        #[test]
        fn adds_two_vectors() {
            let a = Vec2::new(1.0, 2.0);
            let b = Vec2::new(3.0, 4.0);
            let c = a + b;
            assert_eq!(c.x, 4.0);
            assert_eq!(c.y, 6.0);
        }

        #[test]
        fn add_assign_modifies_in_place() {
            let mut a = Vec2::new(1.0, 2.0);
            a += Vec2::new(3.0, 4.0);
            assert_eq!(a.x, 4.0);
            assert_eq!(a.y, 6.0);
        }
    }

    mod vec2_sub {
        use super::*;

        #[test]
        fn subtracts_two_vectors() {
            let a = Vec2::new(5.0, 7.0);
            let b = Vec2::new(2.0, 3.0);
            let c = a - b;
            assert_eq!(c.x, 3.0);
            assert_eq!(c.y, 4.0);
        }

        #[test]
        fn sub_assign_modifies_in_place() {
            let mut a = Vec2::new(5.0, 7.0);
            a -= Vec2::new(2.0, 3.0);
            assert_eq!(a.x, 3.0);
            assert_eq!(a.y, 4.0);
        }
    }

    mod vec2_mul {
        use super::*;

        #[test]
        fn multiplies_vector_by_scalar() {
            let v = Vec2::new(2.0, 3.0);
            let result = v * 2.0;
            assert_eq!(result.x, 4.0);
            assert_eq!(result.y, 6.0);
        }

        #[test]
        fn multiplies_scalar_by_vector() {
            let v = Vec2::new(2.0, 3.0);
            let result = 2.0 * v;
            assert_eq!(result.x, 4.0);
            assert_eq!(result.y, 6.0);
        }

        #[test]
        fn multiply_by_zero_gives_zero() {
            let v = Vec2::new(2.0, 3.0);
            let result = v * 0.0;
            assert_eq!(result, Vec2::ZERO);
        }
    }
}
