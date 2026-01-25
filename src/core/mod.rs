use std::collections::HashMap;

use rand::{rngs::StdRng, Rng, SeedableRng};

use crate::{
    config,
    spatial::SpatialHash,
    types::{
        ColorId, EffectParticle, Vec2, Word, WordFlags, WordId, WordSnapshot, WorldStats,
        TEXT_MAX_DRAW, TRAIL_LEN,
    },
};

#[derive(Clone, Debug)]
pub enum Event {
    Merge { a: WordId, b: WordId },
    Split { id: WordId, parts: u8 },
    Weather { id: WordId, amount: f32 },
    Recondense { id: WordId, amount: f32 },
    SunPulse { center: Vec2, strength: f32 },
}

#[derive(Clone, Copy, Debug)]
pub struct Sun {
    pub center: Vec2,
    pub radius: f32,
    pub strength: f32,
}

pub struct World {
    pub words: Vec<Word>,
    pub events: Vec<Event>,
    pub spatial: SpatialHash,
    pub sun: Option<Sun>,
    pub effects: Vec<EffectParticle>,
    pub dust_pool: HashMap<String, f32>,
    rng: StdRng,
    next_id: WordId,
    neighbors: Vec<usize>,
    acc: Vec<Vec2>,
    positions: Vec<Vec2>,
    grav_candidates: usize,
    collision_candidates: usize,
    last_grav_candidates: usize,
    last_collision_candidates: usize,
    effect_cursor: usize,
    text_index: HashMap<String, WordId>,
}

impl World {
    pub fn new() -> Self {
        let mut world = Self {
            words: Vec::new(),
            events: Vec::new(),
            spatial: SpatialHash::new(config::CELL_SIZE),
            sun: None,
            effects: Vec::with_capacity(config::EFFECT_CAPACITY),
            dust_pool: HashMap::new(),
            rng: StdRng::from_entropy(),
            next_id: 1,
            neighbors: Vec::new(),
            acc: Vec::new(),
            positions: Vec::new(),
            grav_candidates: 0,
            collision_candidates: 0,
            last_grav_candidates: 0,
            last_collision_candidates: 0,
            effect_cursor: 0,
            text_index: HashMap::new(),
        };
        world.spawn_initial_words();
        world.rebuild_text_index();
        world
    }

    pub fn tick(&mut self, dt: f32) {
        self.grav_candidates = 0;
        self.collision_candidates = 0;
        self.rebuild_spatial_index();
        self.apply_gravity_nearby(dt);
        self.integrate(dt);
        self.resolve_collisions();
        self.emit_events();
        self.apply_events();
        self.weathering_step(dt);
        self.autogenesis_step(dt);
        self.update_effects(dt);
        self.last_grav_candidates = self.grav_candidates;
        self.last_collision_candidates = self.collision_candidates;
    }

    pub fn snapshot(&self, out: &mut Vec<WordSnapshot>) {
        out.clear();
        for word in &self.words {
            if word.mass_visible >= config::MIN_VISIBLE_MASS {
                let mut text = [' '; TEXT_MAX_DRAW];
                let mut len = 0;
                for (idx, ch) in word.text.chars().take(TEXT_MAX_DRAW).enumerate() {
                    text[idx] = ch;
                    len = idx + 1;
                }
                out.push(WordSnapshot {
                    id: word.id,
                    text,
                    text_len: len,
                    pos: word.pos,
                    radius: word.radius,
                    mass_visible: word.mass_visible,
                    mass_total: word.mass_total,
                    mass_dust: word.mass_dust,
                    vel: word.vel,
                    trail: word.trail,
                    trail_len: word.trail_len,
                    trail_head: word.trail_head,
                });
            }
        }
    }

    pub fn effects_snapshot(&self, out: &mut Vec<EffectParticle>) {
        out.clear();
        out.extend(self.effects.iter().copied());
    }

    pub fn stats(&self) -> WorldStats {
        let mut stats = WorldStats::default();
        for word in &self.words {
            stats.total_mass += word.mass_total;
            stats.total_mass_visible += word.mass_visible;
            if word.mass_visible >= config::MIN_VISIBLE_MASS {
                stats.visible_count += 1;
            }
        }
        stats.dust_count = self.dust_pool.values().filter(|v| **v > 0.0).count();
        stats.total_words = self.words.len();
        if !self.words.is_empty() {
            stats.gravity_candidates_avg =
                self.last_grav_candidates as f32 / self.words.len() as f32;
            stats.collision_candidates_avg =
                self.last_collision_candidates as f32 / self.words.len() as f32;
        }
        stats
    }

    pub fn add_word(&mut self, text: String, mass_total: f32, pos: Vec2) {
        let visible_count = self
            .words
            .iter()
            .filter(|w| w.mass_visible >= config::MIN_VISIBLE_MASS)
            .count();
        let mut mass_visible = mass_total;
        let mut mass_dust = 0.0;
        if visible_count >= config::K_VISIBLE_MAX {
            mass_visible = mass_total * 0.25;
            mass_dust = mass_total - mass_visible;
        }

        let speed = self.rng.gen_range(4.0..10.0);
        let angle = self.rng.gen_range(0.0..std::f32::consts::TAU);
        let vel = Vec2::new(angle.cos() * speed, angle.sin() * speed);
        self.spawn_or_absorb(SpawnRequest {
            text,
            pos,
            vel,
            mass_visible,
            mass_dust,
        });
    }

    pub fn set_sun(&mut self, center: Vec2) {
        self.sun = Some(Sun {
            center,
            radius: config::SUN_PULSE_RADIUS,
            strength: config::SUN_PULSE_STRENGTH,
        });
        self.spawn_effect_ring(center, 10, '*', ColorId::Cyan);
    }

    fn spawn_initial_words(&mut self) {
        let word_list = [
            ("卒論", 18.0),
            ("研究", 14.0),
            ("進学", 12.0),
            ("就活", 10.0),
            ("発表", 9.0),
            ("実験", 11.0),
            ("締切", 16.0),
            ("指導", 8.0),
            ("授業", 6.0),
            ("生活", 5.0),
            ("不安", 13.0),
            ("期待", 7.0),
        ];

        for _ in 0..config::INIT_WORDS {
            let (text, mass_total) = word_list[self.rng.gen_range(0..word_list.len())];
            let text = text.to_string();
            let pos = Vec2::new(
                self.rng
                    .gen_range(-config::WORLD_HALF_WIDTH..config::WORLD_HALF_WIDTH),
                self.rng
                    .gen_range(-config::WORLD_HALF_HEIGHT..config::WORLD_HALF_HEIGHT),
            );
            let vel = Vec2::new(self.rng.gen_range(-6.0..6.0), self.rng.gen_range(-6.0..6.0));
            self.spawn_or_absorb(SpawnRequest {
                text,
                pos,
                vel,
                mass_visible: mass_total,
                mass_dust: 0.0,
            });
        }
    }

    fn next_id(&mut self) -> WordId {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn rebuild_spatial_index(&mut self) {
        self.positions.clear();
        self.positions.extend(self.words.iter().map(|w| w.pos));
        self.spatial.rebuild(&self.positions);
    }

    fn apply_gravity_nearby(&mut self, dt: f32) {
        self.acc.clear();
        self.acc.resize(self.words.len(), Vec2::ZERO);
        let cutoff_sq = config::GRAVITY_CUTOFF * config::GRAVITY_CUTOFF;

        for i in 0..self.words.len() {
            let pos = self.words[i].pos;
            self.spatial.query_neighbors(pos, &mut self.neighbors);
            if !self.neighbors.is_empty() {
                self.grav_candidates += self.neighbors.len().saturating_sub(1);
            }
            let mut acc = Vec2::ZERO;
            for &j in &self.neighbors {
                if i == j {
                    continue;
                }
                let other = &self.words[j];
                if other.mass_visible < config::MIN_VISIBLE_MASS {
                    continue;
                }
                let delta = other.pos - pos;
                let dist_sq = delta.length_sq() + config::GRAVITY_SOFTENING;
                if dist_sq > cutoff_sq {
                    continue;
                }
                let dir = delta.normalize();
                let force = config::GRAVITY_G * other.mass_visible / dist_sq;
                acc += dir * force;
            }
            self.acc[i] = acc;
        }

        for (word, acc) in self.words.iter_mut().zip(self.acc.iter()) {
            word.vel += *acc * dt;
        }

        if let Some(sun) = self.sun {
            self.apply_sun_pulse(sun, dt);
        }
    }

    fn integrate(&mut self, dt: f32) {
        for word in &mut self.words {
            word.pos += word.vel * dt;

            if word.pos.x < -config::WORLD_HALF_WIDTH {
                word.pos.x = -config::WORLD_HALF_WIDTH;
                word.vel.x = -word.vel.x * config::BOUNCE_DAMP;
            } else if word.pos.x > config::WORLD_HALF_WIDTH {
                word.pos.x = config::WORLD_HALF_WIDTH;
                word.vel.x = -word.vel.x * config::BOUNCE_DAMP;
            }

            if word.pos.y < -config::WORLD_HALF_HEIGHT {
                word.pos.y = -config::WORLD_HALF_HEIGHT;
                word.vel.y = -word.vel.y * config::BOUNCE_DAMP;
            } else if word.pos.y > config::WORLD_HALF_HEIGHT {
                word.pos.y = config::WORLD_HALF_HEIGHT;
                word.vel.y = -word.vel.y * config::BOUNCE_DAMP;
            }

            Self::record_trail(word);
        }
    }

    fn resolve_collisions(&mut self) {
        for i in 0..self.words.len() {
            let pos = self.words[i].pos;
            self.spatial.query_neighbors(pos, &mut self.neighbors);
            if !self.neighbors.is_empty() {
                self.collision_candidates += self.neighbors.len().saturating_sub(1);
            }
            for &j in &self.neighbors {
                if j <= i {
                    continue;
                }
                let (left, right) = self.words.split_at_mut(j);
                let a = &mut left[i];
                let b = &mut right[0];

                if a.mass_visible < config::MIN_VISIBLE_MASS
                    && b.mass_visible < config::MIN_VISIBLE_MASS
                {
                    continue;
                }

                let delta = b.pos - a.pos;
                let dist = delta.length();
                let min_dist = a.radius + b.radius;
                if dist > 0.0 && dist < min_dist {
                    let normal = delta * (1.0 / dist);
                    let overlap = min_dist - dist;
                    a.pos -= normal * (overlap * 0.5);
                    b.pos += normal * (overlap * 0.5);

                    let rel_vel = b.vel - a.vel;
                    let rel_along = rel_vel.dot(normal);
                    let rel_speed = rel_vel.length();
                    if rel_along < 0.0 {
                        let inv_mass_a = if a.mass_visible > 0.0 {
                            1.0 / a.mass_visible
                        } else {
                            0.0
                        };
                        let inv_mass_b = if b.mass_visible > 0.0 {
                            1.0 / b.mass_visible
                        } else {
                            0.0
                        };
                        let inv_mass_sum = inv_mass_a + inv_mass_b;
                        if inv_mass_sum > 0.0 {
                            let restitution = 0.85;
                            let impulse_mag =
                                -(1.0 + restitution) * rel_along / inv_mass_sum;
                            let impulse = normal * impulse_mag;
                            a.vel -= impulse * inv_mass_a;
                            b.vel += impulse * inv_mass_b;
                        }
                    }

                    let mass_ratio = if a.mass_total > b.mass_total {
                        a.mass_total / b.mass_total.max(0.0001)
                    } else {
                        b.mass_total / a.mass_total.max(0.0001)
                    };

                    if rel_speed <= config::MERGE_REL_SPEED_MAX {
                        self.events.push(Event::Merge { a: a.id, b: b.id });
                    } else if rel_speed >= config::SPLIT_REL_SPEED_MIN
                        || mass_ratio >= config::TIDAL_MASS_RATIO
                    {
                        self.events.push(Event::Split { id: a.id, parts: 0 });
                        self.events.push(Event::Split { id: b.id, parts: 0 });
                    }
                }
            }
        }
    }

    fn emit_events(&mut self) {
        // ここでは特別な検出を追加しない。衝突・潮汐などは resolve_collisions 内で積む。
    }

    fn apply_events(&mut self) {
        if self.events.is_empty() {
            return;
        }

        let mut consumed: Vec<WordId> = Vec::new();
        let mut to_add: Vec<SpawnRequest> = Vec::new();

        let events = self.events.clone();
        for event in events {
            match event {
                Event::Merge { a, b } => {
                    if consumed.contains(&a) || consumed.contains(&b) {
                        continue;
                    }
                    let idx_a = self.find_index(a);
                    let idx_b = self.find_index(b);
                    if let (Some(ia), Some(ib)) = (idx_a, idx_b) {
                        let (first, second) = if ia < ib { (ia, ib) } else { (ib, ia) };
                        let a_clone = self.words[first].clone();
                        let b_clone = self.words[second].clone();
                        let total_mass = a_clone.mass_total + b_clone.mass_total;
                        let mass_visible = a_clone.mass_visible + b_clone.mass_visible;
                        let mass_dust = a_clone.mass_dust + b_clone.mass_dust;
                        let vel = if total_mass > 0.0 {
                            (a_clone.vel * a_clone.mass_total + b_clone.vel * b_clone.mass_total)
                                * (1.0 / total_mass)
                        } else {
                            a_clone.vel
                        };
                        let pos = if total_mass > 0.0 {
                            (a_clone.pos * a_clone.mass_total + b_clone.pos * b_clone.mass_total)
                                * (1.0 / total_mass)
                        } else {
                            a_clone.pos
                        };
                        let merged_text = Self::merge_text(&a_clone.text, &b_clone.text);
                        consumed.push(a_clone.id);
                        consumed.push(b_clone.id);
                        to_add.push(SpawnRequest {
                            text: merged_text,
                            pos,
                            vel,
                            mass_visible,
                            mass_dust,
                        });
                        self.spawn_effect_ring(pos, 8, '+', ColorId::Yellow);
                    }
                }
                Event::Split { id, parts: _ } => {
                    if consumed.contains(&id) {
                        continue;
                    }
                    let idx = match self.find_index(id) {
                        Some(idx) => idx,
                        None => continue,
                    };
                    let base = self.words[idx].clone();
                    let components = Self::components(&base.text);
                    if !base.flags.can_split || base.mass_total <= 1.0 || components.len() < 2 {
                        continue;
                    }
                    consumed.push(base.id);

                    let max_parts = components.len().min(config::SPLIT_PARTS_MAX as usize);
                    let parts = self
                        .rng
                        .gen_range(config::SPLIT_PARTS_MIN as usize..=max_parts);
                    let part_mass = base.mass_total / parts as f32;
                    let part_visible = base.mass_visible / parts as f32;
                    let part_dust = base.mass_dust / parts as f32;
                    let _base_radius =
                        config::WORD_RADIUS_BASE + part_mass * config::WORD_RADIUS_SCALE;

                    let groups = Self::split_groups(&components, parts);
                    for (idx, text) in groups.into_iter().enumerate() {
                        let angle = self.rng.gen_range(0.0..std::f32::consts::TAU);
                        let dir = Vec2::new(angle.cos(), angle.sin());
                        let offset = dir * (base.radius * 0.9);
                        let vel_jitter =
                            Vec2::new(self.rng.gen_range(-2.0..2.0), self.rng.gen_range(-2.0..2.0));
                        let radial = dir * config::SPLIT_RADIAL_SPEED;
                        let pos = base.pos + offset;
                        let vel = base.vel + vel_jitter + radial;
                        let _ = idx;
                        to_add.push(SpawnRequest {
                            text,
                            pos,
                            vel,
                            mass_visible: part_visible,
                            mass_dust: part_dust,
                        });
                    }
                    self.spawn_effect_ring(base.pos, 12, '*', ColorId::Red);
                }
                _ => {}
            }
        }

        if !consumed.is_empty() {
            self.words.retain(|w| !consumed.contains(&w.id));
        }
        if !consumed.is_empty() {
            self.rebuild_text_index();
        }
        for req in to_add {
            self.spawn_or_absorb(req);
        }
        self.events.clear();
    }

    fn find_index(&self, id: WordId) -> Option<usize> {
        self.words.iter().position(|w| w.id == id)
    }

    fn merge_text(a: &str, b: &str) -> String {
        if a.is_empty() {
            return b.to_string();
        }
        if b.is_empty() {
            return a.to_string();
        }
        format!("{}-{}", a, b)
    }

    fn components(text: &str) -> Vec<String> {
        text.split('-')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect()
    }

    fn split_groups(components: &[String], parts: usize) -> Vec<String> {
        let len = components.len();
        let parts = parts.max(2).min(len);
        let base = len / parts;
        let rem = len % parts;
        let mut out = Vec::with_capacity(parts);
        let mut idx = 0;
        for i in 0..parts {
            let take = base + if i < rem { 1 } else { 0 };
            let group = components[idx..idx + take].join("-");
            out.push(group);
            idx += take;
        }
        out
    }

    fn rebuild_text_index(&mut self) {
        self.text_index.clear();
        for word in &self.words {
            self.text_index.insert(word.text.clone(), word.id);
            self.dust_pool.entry(word.text.clone()).or_insert(0.0);
        }
    }

    fn weathering_step(&mut self, dt: f32) {
        for word in &mut self.words {
            let amount = word.mass_visible * config::WEATHERING_RATE * dt;
            word.mass_visible -= amount;
            word.mass_dust += amount;
            word.mass_total = word.mass_visible + word.mass_dust;
            self.dust_pool.insert(word.text.clone(), word.mass_dust);
        }
    }

    fn autogenesis_step(&mut self, dt: f32) {
        let visible_count = self
            .words
            .iter()
            .filter(|w| w.mass_visible >= config::MIN_VISIBLE_MASS)
            .count();

        if visible_count >= config::K_VISIBLE_MIN {
            return;
        }

        let keys: Vec<String> = self.dust_pool.keys().cloned().collect();
        for key in keys {
            let dust = *self.dust_pool.get(&key).unwrap_or(&0.0);
            if dust <= 0.0 {
                continue;
            }
            let amount = dust * config::AUTOGENESIS_RATE * dt;
            let remaining = dust - amount;
            if let Some(&id) = self.text_index.get(&key) {
                if let Some(word) = self.words.iter_mut().find(|w| w.id == id) {
                    word.mass_visible += amount;
                    word.mass_dust = remaining;
                    word.mass_total = word.mass_visible + word.mass_dust;
                    self.dust_pool.insert(key.clone(), word.mass_dust);
                }
            } else {
                let pos = Vec2::new(
                    self.rng
                        .gen_range(-config::WORLD_HALF_WIDTH..config::WORLD_HALF_WIDTH),
                    self.rng
                        .gen_range(-config::WORLD_HALF_HEIGHT..config::WORLD_HALF_HEIGHT),
                );
                let vel = Vec2::new(self.rng.gen_range(-4.0..4.0), self.rng.gen_range(-4.0..4.0));
                self.spawn_or_absorb(SpawnRequest {
                    text: key.clone(),
                    pos,
                    vel,
                    mass_visible: amount,
                    mass_dust: remaining,
                });
            }
        }
    }

    fn apply_sun_pulse(&mut self, sun: Sun, dt: f32) {
        let radius_sq = sun.radius * sun.radius;
        for word in &mut self.words {
            let delta = word.pos - sun.center;
            let dist_sq = delta.length_sq();
            if dist_sq <= radius_sq {
                let dir = if dist_sq > 0.0 {
                    delta.normalize()
                } else {
                    Vec2::new(1.0, 0.0)
                };
                word.vel += dir * (sun.strength * dt);
            }
        }
    }

    fn record_trail(word: &mut Word) {
        word.trail_head = (word.trail_head + 1) % TRAIL_LEN;
        word.trail[word.trail_head] = word.pos;
        if word.trail_len < TRAIL_LEN {
            word.trail_len += 1;
        }
    }

    fn spawn_effect_ring(&mut self, center: Vec2, count: usize, glyph: char, color: ColorId) {
        for i in 0..count {
            let angle = (i as f32 / count as f32) * std::f32::consts::TAU;
            let dir = Vec2::new(angle.cos(), angle.sin());
            let vel = dir * self.rng.gen_range(4.0..10.0);
            self.push_effect(EffectParticle {
                pos: center + dir * 1.0,
                vel,
                ttl: config::EFFECT_TTL,
                glyph,
                color,
            });
        }
    }

    fn push_effect(&mut self, effect: EffectParticle) {
        if config::EFFECT_CAPACITY == 0 {
            return;
        }
        if self.effects.len() < config::EFFECT_CAPACITY {
            self.effects.push(effect);
        } else {
            if self.effect_cursor >= self.effects.len() {
                self.effect_cursor = 0;
            }
            self.effects[self.effect_cursor] = effect;
            self.effect_cursor = (self.effect_cursor + 1) % self.effects.len();
        }
    }

    fn update_effects(&mut self, dt: f32) {
        for effect in &mut self.effects {
            effect.pos += effect.vel * dt;
            effect.ttl -= dt;
        }
        self.effects.retain(|e| e.ttl > 0.0);
        if self.effect_cursor >= self.effects.len() {
            self.effect_cursor = 0;
        }
    }

    fn spawn_or_absorb(&mut self, req: SpawnRequest) {
        let total_mass = req.mass_visible + req.mass_dust;
        if let Some(&id) = self.text_index.get(&req.text) {
            if let Some(word) = self.words.iter_mut().find(|w| w.id == id) {
                let combined_mass = word.mass_total + total_mass;
                let vel = if combined_mass > 0.0 {
                    (word.vel * word.mass_total + req.vel * total_mass) * (1.0 / combined_mass)
                } else {
                    word.vel
                };
                let pos = if combined_mass > 0.0 {
                    (word.pos * word.mass_total + req.pos * total_mass) * (1.0 / combined_mass)
                } else {
                    word.pos
                };
                word.vel = vel;
                word.pos = pos;
                word.mass_visible += req.mass_visible;
                word.mass_dust += req.mass_dust;
                word.mass_total = word.mass_visible + word.mass_dust;
                word.radius =
                    config::WORD_RADIUS_BASE + word.mass_total * config::WORD_RADIUS_SCALE;
                self.dust_pool.insert(word.text.clone(), word.mass_dust);
                let effect_pos = word.pos;
                self.spawn_effect_ring(effect_pos, 6, '+', ColorId::Magenta);
            }
            return;
        }

        let id = self.next_id();
        let radius = config::WORD_RADIUS_BASE + total_mass * config::WORD_RADIUS_SCALE;
        let word = Word {
            id,
            text: req.text.clone(),
            pos: req.pos,
            vel: req.vel,
            radius,
            mass_total: total_mass,
            mass_visible: req.mass_visible,
            mass_dust: req.mass_dust,
            flags: WordFlags { can_split: true },
            trail: [req.pos; TRAIL_LEN],
            trail_head: 0,
            trail_len: 1,
        };
        self.words.push(word);
        self.text_index.insert(req.text.clone(), id);
        self.dust_pool.insert(req.text, req.mass_dust);
    }
}

struct SpawnRequest {
    text: String,
    pos: Vec2,
    vel: Vec2,
    mass_visible: f32,
    mass_dust: f32,
}
