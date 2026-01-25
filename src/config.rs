pub const SIM_HZ: f32 = 60.0;
pub const RENDER_HZ: f32 = 30.0;
pub const DT: f32 = 1.0 / SIM_HZ;

pub const WORLD_HALF_WIDTH: f32 = 120.0;
pub const WORLD_HALF_HEIGHT: f32 = 60.0;

pub const CELL_SIZE: f32 = 6.0;

pub const INIT_WORDS: usize = 24;

pub const GRAVITY_G: f32 = 18.0;
pub const GRAVITY_SOFTENING: f32 = 2.0;
pub const GRAVITY_CUTOFF: f32 = 48.0;

pub const BOUNCE_DAMP: f32 = 0.9;

pub const MERGE_REL_SPEED_MAX: f32 = 6.0;
pub const SPLIT_REL_SPEED_MIN: f32 = 14.0;
pub const TIDAL_MASS_RATIO: f32 = 6.0;
pub const SPLIT_PARTS_MIN: u8 = 2;
pub const SPLIT_PARTS_MAX: u8 = 4;
pub const SPLIT_RADIAL_SPEED: f32 = 8.0;

pub const WEATHERING_RATE: f32 = 0.02;
pub const AUTOGENESIS_RATE: f32 = 0.08;

pub const MIN_VISIBLE_MASS: f32 = 0.2;

pub const K_VISIBLE_MIN: usize = 40;
pub const K_VISIBLE_MAX: usize = 400;

pub const WORD_RADIUS_BASE: f32 = 1.2;
pub const WORD_RADIUS_SCALE: f32 = 0.06;

pub const SUN_PULSE_RADIUS: f32 = 32.0;
pub const SUN_PULSE_STRENGTH: f32 = 14.0;

pub const EFFECT_CAPACITY: usize = 512;
pub const EFFECT_TTL: f32 = 0.6;
