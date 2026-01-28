use std::collections::{HashMap, HashSet};

use rand::{rngs::StdRng, Rng, SeedableRng};

use crate::{
    config,
    spatial::SpatialHash,
    types::{
        ColorId, EffectParticle, GravityDebugStats, Vec2, Word, WordFlags, WordId, WordSnapshot,
        WorldStats, TEXT_MAX_DRAW, TRAIL_LEN,
    },
};

const WORD_JOIN_DISPLAY: char = '-';

#[derive(Clone, Debug)]
pub enum Event {
    Merge { a: WordId, b: WordId },
    Split { id: WordId },
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
    gravity_debug: GravityDebugStats,
    effect_cursor: usize,
    text_index: HashMap<String, WordId>,
    word_indices: HashMap<WordId, usize>,
}

impl World {
    pub fn new() -> Self {
        let mut world = Self {
            words: Vec::new(),
            events: Vec::new(),
            spatial: SpatialHash::new(config::SPATIAL_CELL_SIZE),
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
            gravity_debug: GravityDebugStats::default(),
            effect_cursor: 0,
            text_index: HashMap::new(),
            word_indices: HashMap::new(),
        };
        world.spawn_initial_words();
        world.rebuild_text_index();
        world.rebuild_index_map();
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
        self.consolidate_duplicates();
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
                    text[idx] = if ch == config::WORD_JOIN_SEP {
                        WORD_JOIN_DISPLAY
                    } else {
                        ch
                    };
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
        stats.gravity_debug = self.gravity_debug;
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
        let cutoff = config::GRAVITY_CUTOFF;
        let mut debug = GravityDebugStats::default();
        debug.sample_index = -1;
        let sample_index = self
            .words
            .iter()
            .position(|w| w.mass_visible >= config::MIN_VISIBLE_MASS)
            .or_else(|| if self.words.is_empty() { None } else { Some(0) });
        if let Some(idx) = sample_index {
            debug.sample_index = idx as i32;
        }
        let mut sample_nearest_r_sq = f32::INFINITY;

        for i in 0..self.words.len() {
            let pos = self.words[i].pos;
            self.spatial.query_neighbors_range(
                pos,
                config::SPATIAL_QUERY_RANGE_GRAVITY,
                &mut self.neighbors,
            );
            if !self.neighbors.is_empty() {
                self.grav_candidates += self.neighbors.len().saturating_sub(1);
            }
            let mut acc = Vec2::ZERO;
            let is_sample = debug.sample_index == i as i32;
            if is_sample {
                debug.candidates = self.neighbors.len().saturating_sub(1);
            }
            let mut candidates_after_cutoff = 0usize;
            for &j in &self.neighbors {
                if i == j {
                    continue;
                }
                let other = &self.words[j];
                let delta = other.pos - pos;
                let raw_dist_sq = delta.length_sq();
                if raw_dist_sq < 1.0e-6 {
                    continue;
                }
                let r = raw_dist_sq.sqrt();
                let other_mass_visible = other.mass_visible;
                let other_subvisible = other_mass_visible < config::MIN_VISIBLE_MASS;
                if is_sample && raw_dist_sq < sample_nearest_r_sq {
                    sample_nearest_r_sq = raw_dist_sq;
                    debug.sample_r = r;
                    debug.sample_cutoff_rejected = r >= cutoff;
                    debug.sample_other_mass_visible = other_mass_visible;
                    debug.sample_other_subvisible = other_subvisible;
                }
                let weight = gravity_cutoff_weight(r, cutoff);
                if weight <= 0.0 {
                    continue;
                }
                let dist_sq = raw_dist_sq + config::GRAVITY_SOFTENING;
                let dir = delta * (1.0 / r);
                let mass_for_gravity = other_mass_visible.max(config::GRAVITY_MIN_MASS);
                let force = config::GRAVITY_G * mass_for_gravity * weight / dist_sq;
                acc += dir * force;
                if is_sample {
                    candidates_after_cutoff += 1;
                }
            }
            let mut acc_len = acc.length();
            let mut dv = acc_len * dt;
            if acc_len > 0.0 && dv > config::GRAVITY_DV_MAX {
                let scale = config::GRAVITY_DV_MAX / dv;
                acc = acc * scale;
                acc_len *= scale;
                dv = acc_len * dt;
            }
            if is_sample {
                debug.candidates_after_cutoff = candidates_after_cutoff;
                debug.acc_mag = acc_len;
                debug.dv_mag = dv;
            }
            self.acc[i] = acc;
        }

        for (word, acc) in self.words.iter_mut().zip(self.acc.iter()) {
            word.vel += *acc * dt;
        }

        if let Some(sun) = self.sun {
            self.apply_sun_pulse(sun, dt);
        }

        self.gravity_debug = debug;
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
            self.spatial.query_neighbors_range(
                pos,
                config::SPATIAL_QUERY_RANGE_COLLISION,
                &mut self.neighbors,
            );
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
                if dist < min_dist {
                    let (normal, dist_safe) = if dist > 1.0e-6 {
                        (delta * (1.0 / dist), dist)
                    } else {
                        (Vec2::new(1.0, 0.0), 0.0)
                    };
                    let overlap = min_dist - dist_safe;
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
                        self.events.push(Event::Split { id: a.id });
                        self.events.push(Event::Split { id: b.id });
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

        let mut consumed: HashSet<WordId> = HashSet::new();
        let mut to_add: Vec<SpawnRequest> = Vec::new();

        let events = std::mem::take(&mut self.events);
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
                        consumed.insert(a_clone.id);
                        consumed.insert(b_clone.id);
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
                Event::Split { id } => {
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
                    consumed.insert(base.id);

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
            }
        }

        if !consumed.is_empty() {
            self.words.retain(|w| !consumed.contains(&w.id));
            self.rebuild_text_index();
            self.rebuild_index_map();
        }
        for req in to_add {
            self.spawn_or_absorb(req);
        }
    }

    fn find_index(&self, id: WordId) -> Option<usize> {
        self.word_indices.get(&id).copied()
    }

    fn merge_text(a: &str, b: &str) -> String {
        if a.is_empty() {
            return b.to_string();
        }
        if b.is_empty() {
            return a.to_string();
        }
        let mut out = String::with_capacity(a.len() + b.len() + 1);
        out.push_str(a);
        out.push(config::WORD_JOIN_SEP);
        out.push_str(b);
        out
    }

    fn components(text: &str) -> Vec<String> {
        text.split(config::WORD_JOIN_SEP)
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
        let sep = config::WORD_JOIN_SEP.to_string();
        for i in 0..parts {
            let take = base + if i < rem { 1 } else { 0 };
            let group = components[idx..idx + take].join(&sep);
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

    fn rebuild_index_map(&mut self) {
        self.word_indices.clear();
        for (idx, word) in self.words.iter().enumerate() {
            self.word_indices.insert(word.id, idx);
        }
    }

    fn weathering_step(&mut self, dt: f32) {
        self.dust_pool.clear();
        for word in &mut self.words {
            let amount = (word.mass_visible * config::WEATHERING_RATE * dt).min(word.mass_visible);
            word.mass_visible -= amount;
            word.mass_dust += amount;
            word.mass_total = word.mass_visible + word.mass_dust;
            *self.dust_pool.entry(word.text.clone()).or_insert(0.0) += word.mass_dust;
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
            if self.effect_cursor >= config::EFFECT_CAPACITY {
                self.effect_cursor = 0;
            }
            self.effects[self.effect_cursor] = effect;
            self.effect_cursor = (self.effect_cursor + 1) % config::EFFECT_CAPACITY;
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
                Self::absorb_into_word(word, &req, total_mass);
                self.dust_pool.insert(word.text.clone(), word.mass_dust);
                let effect_pos = word.pos;
                self.spawn_effect_ring(effect_pos, 6, '+', ColorId::Magenta);
                return;
            }
            self.text_index.remove(&req.text);
            if let Some(word) = self.words.iter_mut().find(|w| w.text == req.text) {
                self.text_index.insert(req.text.clone(), word.id);
                Self::absorb_into_word(word, &req, total_mass);
                self.dust_pool.insert(word.text.clone(), word.mass_dust);
                let effect_pos = word.pos;
                self.spawn_effect_ring(effect_pos, 6, '+', ColorId::Magenta);
                return;
            }
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
        self.word_indices.insert(id, self.words.len() - 1);
    }

    fn consolidate_duplicates(&mut self) {
        if self.words.len() < 2 {
            return;
        }
        let mut seen: HashSet<&str> = HashSet::with_capacity(self.words.len());
        let mut has_duplicate = false;
        for word in &self.words {
            if !seen.insert(word.text.as_str()) {
                has_duplicate = true;
                break;
            }
        }
        if !has_duplicate {
            return;
        }

        let mut index: HashMap<String, usize> = HashMap::with_capacity(self.words.len());
        let mut best_mass: Vec<f32> = Vec::with_capacity(self.words.len());
        let mut merged: Vec<Word> = Vec::with_capacity(self.words.len());

        for word in self.words.drain(..) {
            if let Some(&idx) = index.get(&word.text) {
                let target = &mut merged[idx];
                let target_mass = target.mass_total;
                let total_mass = target_mass + word.mass_total;
                if total_mass > 0.0 {
                    target.pos =
                        (target.pos * target_mass + word.pos * word.mass_total) * (1.0 / total_mass);
                    target.vel =
                        (target.vel * target_mass + word.vel * word.mass_total) * (1.0 / total_mass);
                }
                target.mass_visible += word.mass_visible;
                target.mass_dust += word.mass_dust;
                target.mass_total = total_mass;
                target.radius =
                    config::WORD_RADIUS_BASE + target.mass_total * config::WORD_RADIUS_SCALE;
                if word.mass_total > best_mass[idx] {
                    best_mass[idx] = word.mass_total;
                    target.trail = word.trail;
                    target.trail_head = word.trail_head;
                    target.trail_len = word.trail_len;
                }
            } else {
                let idx = merged.len();
                best_mass.push(word.mass_total);
                index.insert(word.text.clone(), idx);
                merged.push(word);
            }
        }

        self.words = merged;
        self.rebuild_text_index();
        self.rebuild_index_map();
    }

    fn absorb_into_word(word: &mut Word, req: &SpawnRequest, total_mass: f32) {
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
        word.radius = config::WORD_RADIUS_BASE + word.mass_total * config::WORD_RADIUS_SCALE;
    }
}

fn gravity_cutoff_weight(r: f32, cutoff: f32) -> f32 {
    if cutoff <= 0.0 {
        return 0.0;
    }
    let fade_start = cutoff * config::GRAVITY_CUTOFF_FADE_START;
    if r >= cutoff {
        0.0
    } else if r <= fade_start {
        1.0
    } else {
        1.0 - smoothstep(fade_start, cutoff, r)
    }
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    if edge1 <= edge0 {
        return if x < edge1 { 1.0 } else { 0.0 };
    }
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

struct SpawnRequest {
    text: String,
    pos: Vec2,
    vel: Vec2,
    mass_visible: f32,
    mass_dust: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    mod helper_functions {
        use super::*;

        mod smoothstep {
            use super::*;

            #[test]
            fn returns_zero_at_edge0() {
                let result = smoothstep(0.0, 1.0, 0.0);
                assert!((result - 0.0).abs() < 1e-6);
            }

            #[test]
            fn returns_one_at_edge1() {
                let result = smoothstep(0.0, 1.0, 1.0);
                assert!((result - 1.0).abs() < 1e-6);
            }

            #[test]
            fn returns_half_at_midpoint() {
                let result = smoothstep(0.0, 1.0, 0.5);
                assert!((result - 0.5).abs() < 1e-6);
            }

            #[test]
            fn clamps_below_edge0() {
                let result = smoothstep(0.0, 1.0, -1.0);
                assert!((result - 0.0).abs() < 1e-6);
            }

            #[test]
            fn clamps_above_edge1() {
                let result = smoothstep(0.0, 1.0, 2.0);
                assert!((result - 1.0).abs() < 1e-6);
            }

            #[test]
            fn handles_equal_edges() {
                let result = smoothstep(1.0, 1.0, 0.5);
                assert!((result - 1.0).abs() < 1e-6);
            }
        }

        mod gravity_cutoff_weight {
            use super::*;

            #[test]
            fn returns_zero_for_zero_cutoff() {
                let result = gravity_cutoff_weight(10.0, 0.0);
                assert_eq!(result, 0.0);
            }

            #[test]
            fn returns_zero_beyond_cutoff() {
                let result = gravity_cutoff_weight(100.0, 50.0);
                assert_eq!(result, 0.0);
            }

            #[test]
            fn returns_one_within_fade_start() {
                // GRAVITY_CUTOFF_FADE_START = 0.7
                // cutoff = 100, fade_start = 70
                let result = gravity_cutoff_weight(10.0, 100.0);
                assert_eq!(result, 1.0);
            }

            #[test]
            fn fades_between_start_and_cutoff() {
                // cutoff = 100, fade_start = 70
                let result = gravity_cutoff_weight(85.0, 100.0);
                assert!(result > 0.0 && result < 1.0);
            }
        }

        mod merge_text {
            use super::*;

            #[test]
            fn joins_two_texts() {
                let result = World::merge_text("foo", "bar");
                assert!(result.contains("foo"));
                assert!(result.contains("bar"));
                assert!(result.contains(config::WORD_JOIN_SEP));
            }

            #[test]
            fn returns_other_if_first_empty() {
                let result = World::merge_text("", "bar");
                assert_eq!(result, "bar");
            }

            #[test]
            fn returns_first_if_second_empty() {
                let result = World::merge_text("foo", "");
                assert_eq!(result, "foo");
            }
        }

        mod components {
            use super::*;

            #[test]
            fn splits_by_separator() {
                let text = format!("foo{}bar{}baz", config::WORD_JOIN_SEP, config::WORD_JOIN_SEP);
                let result = World::components(&text);
                assert_eq!(result.len(), 3);
                assert_eq!(result[0], "foo");
                assert_eq!(result[1], "bar");
                assert_eq!(result[2], "baz");
            }

            #[test]
            fn single_component_returns_one_element() {
                let result = World::components("single");
                assert_eq!(result.len(), 1);
                assert_eq!(result[0], "single");
            }

            #[test]
            fn empty_string_returns_empty_vec() {
                let result = World::components("");
                assert!(result.is_empty());
            }
        }

        mod split_groups {
            use super::*;

            #[test]
            fn splits_into_requested_parts() {
                let components: Vec<String> = vec!["a", "b", "c", "d"].iter().map(|s| s.to_string()).collect();
                let result = World::split_groups(&components, 2);
                assert_eq!(result.len(), 2);
            }

            #[test]
            fn limits_parts_to_component_count() {
                let components: Vec<String> = vec!["a", "b"].iter().map(|s| s.to_string()).collect();
                let result = World::split_groups(&components, 10);
                assert_eq!(result.len(), 2);
            }

            #[test]
            fn minimum_parts_is_two() {
                let components: Vec<String> = vec!["a", "b", "c"].iter().map(|s| s.to_string()).collect();
                let result = World::split_groups(&components, 1);
                assert_eq!(result.len(), 2);
            }
        }
    }

    mod world_creation {
        use super::*;

        #[test]
        fn new_world_has_initial_words() {
            let world = World::new();
            assert!(!world.words.is_empty());
        }

        #[test]
        fn new_world_has_no_events() {
            let world = World::new();
            assert!(world.events.is_empty());
        }

        #[test]
        fn new_world_has_no_sun() {
            let world = World::new();
            assert!(world.sun.is_none());
        }
    }

    mod mass_conservation {
        use super::*;

        #[test]
        fn mass_total_equals_visible_plus_dust() {
            let world = World::new();
            for word in &world.words {
                let expected = word.mass_visible + word.mass_dust;
                assert!((word.mass_total - expected).abs() < 1e-6,
                    "mass_total {} != mass_visible {} + mass_dust {}",
                    word.mass_total, word.mass_visible, word.mass_dust);
            }
        }

        #[test]
        fn weathering_preserves_total_mass() {
            let mut world = World::new();
            let initial_total: f32 = world.words.iter().map(|w| w.mass_total).sum();
            
            // Run several weathering steps
            for _ in 0..100 {
                world.weathering_step(config::DT);
            }
            
            let final_total: f32 = world.words.iter().map(|w| w.mass_total).sum();
            assert!((initial_total - final_total).abs() < 1e-3,
                "Total mass changed: {} -> {}", initial_total, final_total);
        }

        #[test]
        fn weathering_transfers_mass_to_dust() {
            let mut world = World::new();
            let initial_visible: f32 = world.words.iter().map(|w| w.mass_visible).sum();
            let initial_dust: f32 = world.words.iter().map(|w| w.mass_dust).sum();
            
            // Run weathering
            for _ in 0..100 {
                world.weathering_step(config::DT);
            }
            
            let final_visible: f32 = world.words.iter().map(|w| w.mass_visible).sum();
            let final_dust: f32 = world.words.iter().map(|w| w.mass_dust).sum();
            
            // Visible should decrease
            assert!(final_visible < initial_visible);
            // Dust should increase
            assert!(final_dust > initial_dust);
        }

        #[test]
        fn autogenesis_transfers_dust_to_visible() {
            let mut world = World::new();
            world.words.clear();
            world.text_index.clear();
            world.word_indices.clear();
            world.dust_pool.clear();
            
            // Create a word with all mass as dust
            let id = world.next_id();
            let text = "dusty".to_string();
            world.words.push(Word {
                id,
                text: text.clone(),
                pos: Vec2::ZERO,
                vel: Vec2::ZERO,
                radius: 1.0,
                mass_total: 10.0,
                mass_visible: 0.0,  // All dust
                mass_dust: 10.0,
                flags: WordFlags { can_split: false },
                trail: [Vec2::ZERO; TRAIL_LEN],
                trail_head: 0,
                trail_len: 0,
            });
            world.text_index.insert(text.clone(), id);
            world.word_indices.insert(id, 0);
            world.dust_pool.insert(text.clone(), 10.0);
            
            let initial_visible = world.words[0].mass_visible;
            let initial_dust = world.words[0].mass_dust;
            let initial_total = world.words[0].mass_total;
            
            // Run autogenesis (visible count is 0, below K_VISIBLE_MIN)
            for _ in 0..100 {
                world.autogenesis_step(config::DT);
            }
            
            let final_visible = world.words[0].mass_visible;
            let final_dust = world.words[0].mass_dust;
            let final_total = world.words[0].mass_total;
            
            // Visible should increase
            assert!(final_visible > initial_visible, 
                "Visible should increase: {} -> {}", initial_visible, final_visible);
            // Dust should decrease
            assert!(final_dust < initial_dust,
                "Dust should decrease: {} -> {}", initial_dust, final_dust);
            // Total should be conserved
            assert!((initial_total - final_total).abs() < 1e-3,
                "Total should be conserved: {} -> {}", initial_total, final_total);
        }
    }

    mod wall_reflection {
        use super::*;

        #[test]
        fn word_stays_within_bounds_after_integration() {
            let mut world = World::new();
            // Set all words to move toward boundaries
            for word in &mut world.words {
                word.pos = Vec2::new(config::WORLD_HALF_WIDTH - 1.0, config::WORLD_HALF_HEIGHT - 1.0);
                word.vel = Vec2::new(100.0, 100.0);
            }
            
            // Run several integration steps
            for _ in 0..100 {
                world.integrate(config::DT);
            }
            
            for word in &world.words {
                assert!(word.pos.x >= -config::WORLD_HALF_WIDTH && word.pos.x <= config::WORLD_HALF_WIDTH,
                    "Word x position {} out of bounds", word.pos.x);
                assert!(word.pos.y >= -config::WORLD_HALF_HEIGHT && word.pos.y <= config::WORLD_HALF_HEIGHT,
                    "Word y position {} out of bounds", word.pos.y);
            }
        }

        #[test]
        fn velocity_reverses_on_wall_hit() {
            let mut world = World::new();
            // Clear all words and add a single one at the boundary
            world.words.clear();
            world.text_index.clear();
            world.word_indices.clear();
            
            let id = world.next_id();
            world.words.push(Word {
                id,
                text: "test".to_string(),
                pos: Vec2::new(config::WORLD_HALF_WIDTH + 1.0, 0.0),
                vel: Vec2::new(10.0, 0.0),
                radius: 1.0,
                mass_total: 10.0,
                mass_visible: 10.0,
                mass_dust: 0.0,
                flags: WordFlags { can_split: false },
                trail: [Vec2::ZERO; TRAIL_LEN],
                trail_head: 0,
                trail_len: 0,
            });
            
            world.integrate(config::DT);
            
            // Velocity x should be reversed (negative)
            assert!(world.words[0].vel.x < 0.0);
        }
    }

    mod sun_pulse {
        use super::*;

        #[test]
        fn set_sun_creates_sun() {
            let mut world = World::new();
            world.set_sun(Vec2::new(10.0, 10.0));
            assert!(world.sun.is_some());
        }

        #[test]
        fn sun_pulse_affects_nearby_words() {
            let mut world = World::new();
            world.words.clear();
            world.text_index.clear();
            world.word_indices.clear();
            
            let id = world.next_id();
            world.words.push(Word {
                id,
                text: "nearby".to_string(),
                pos: Vec2::new(0.0, 0.0),
                vel: Vec2::ZERO,
                radius: 1.0,
                mass_total: 10.0,
                mass_visible: 10.0,
                mass_dust: 0.0,
                flags: WordFlags { can_split: false },
                trail: [Vec2::ZERO; TRAIL_LEN],
                trail_head: 0,
                trail_len: 0,
            });
            
            let sun = Sun {
                center: Vec2::new(0.0, 0.0),
                radius: config::SUN_PULSE_RADIUS,
                strength: config::SUN_PULSE_STRENGTH,
            };
            
            // Word at center might not change (direction is undefined at center)
            // But let's test with a word offset from center
            world.words[0].pos = Vec2::new(5.0, 0.0);
            world.words[0].vel = Vec2::ZERO;
            world.apply_sun_pulse(sun, config::DT);
            
            // Should have some velocity now
            assert!(world.words[0].vel.length() > 0.0);
        }

        #[test]
        fn sun_pulse_does_not_affect_distant_words() {
            let mut world = World::new();
            world.words.clear();
            world.text_index.clear();
            world.word_indices.clear();
            
            let id = world.next_id();
            world.words.push(Word {
                id,
                text: "distant".to_string(),
                pos: Vec2::new(config::SUN_PULSE_RADIUS + 100.0, 0.0),
                vel: Vec2::ZERO,
                radius: 1.0,
                mass_total: 10.0,
                mass_visible: 10.0,
                mass_dust: 0.0,
                flags: WordFlags { can_split: false },
                trail: [Vec2::ZERO; TRAIL_LEN],
                trail_head: 0,
                trail_len: 0,
            });
            
            let sun = Sun {
                center: Vec2::new(0.0, 0.0),
                radius: config::SUN_PULSE_RADIUS,
                strength: config::SUN_PULSE_STRENGTH,
            };
            
            world.apply_sun_pulse(sun, config::DT);
            
            // Should remain at zero velocity
            assert_eq!(world.words[0].vel, Vec2::ZERO);
        }
    }

    mod add_word {
        use super::*;

        #[test]
        fn adds_new_word_to_world() {
            let mut world = World::new();
            let initial_count = world.words.len();
            world.add_word("新しい言葉".to_string(), 10.0, Vec2::ZERO);
            assert!(world.words.len() >= initial_count);
        }

        #[test]
        fn absorbed_word_increases_mass() {
            let mut world = World::new();
            let text = "テスト".to_string();
            world.add_word(text.clone(), 10.0, Vec2::ZERO);
            
            let initial_mass: f32 = world.words.iter()
                .filter(|w| w.text == text)
                .map(|w| w.mass_total)
                .sum();
            
            world.add_word(text.clone(), 5.0, Vec2::ZERO);
            
            let final_mass: f32 = world.words.iter()
                .filter(|w| w.text == text)
                .map(|w| w.mass_total)
                .sum();
            
            assert!(final_mass > initial_mass);
        }
    }

    mod snapshot {
        use super::*;

        #[test]
        fn excludes_subvisible_words() {
            let mut world = World::new();
            // Set one word to be subvisible
            if let Some(word) = world.words.first_mut() {
                word.mass_visible = config::MIN_VISIBLE_MASS / 2.0;
            }
            
            let mut snapshot = Vec::new();
            world.snapshot(&mut snapshot);
            
            // Subvisible word should not appear in snapshot
            let subvisible_in_snapshot = snapshot.iter()
                .any(|s| s.mass_visible < config::MIN_VISIBLE_MASS);
            assert!(!subvisible_in_snapshot);
        }

        #[test]
        fn includes_visible_words() {
            let world = World::new();
            let visible_count = world.words.iter()
                .filter(|w| w.mass_visible >= config::MIN_VISIBLE_MASS)
                .count();
            
            let mut snapshot = Vec::new();
            world.snapshot(&mut snapshot);
            
            assert_eq!(snapshot.len(), visible_count);
        }
    }

    mod stats {
        use super::*;

        #[test]
        fn counts_visible_words() {
            let world = World::new();
            let expected = world.words.iter()
                .filter(|w| w.mass_visible >= config::MIN_VISIBLE_MASS)
                .count();
            
            let stats = world.stats();
            assert_eq!(stats.visible_count, expected);
        }

        #[test]
        fn sums_total_mass() {
            let world = World::new();
            let expected: f32 = world.words.iter().map(|w| w.mass_total).sum();
            
            let stats = world.stats();
            assert!((stats.total_mass - expected).abs() < 1e-6);
        }
    }

    mod consolidate_duplicates {
        use super::*;

        #[test]
        fn merges_words_with_same_text() {
            let mut world = World::new();
            world.words.clear();
            world.text_index.clear();
            world.word_indices.clear();
            
            let text = "duplicate".to_string();
            let id1 = world.next_id();
            let id2 = world.next_id();
            
            world.words.push(Word {
                id: id1,
                text: text.clone(),
                pos: Vec2::new(0.0, 0.0),
                vel: Vec2::new(1.0, 0.0),
                radius: 1.0,
                mass_total: 10.0,
                mass_visible: 10.0,
                mass_dust: 0.0,
                flags: WordFlags { can_split: false },
                trail: [Vec2::ZERO; TRAIL_LEN],
                trail_head: 0,
                trail_len: 0,
            });
            
            world.words.push(Word {
                id: id2,
                text: text.clone(),
                pos: Vec2::new(10.0, 0.0),
                vel: Vec2::new(-1.0, 0.0),
                radius: 1.0,
                mass_total: 5.0,
                mass_visible: 5.0,
                mass_dust: 0.0,
                flags: WordFlags { can_split: false },
                trail: [Vec2::ZERO; TRAIL_LEN],
                trail_head: 0,
                trail_len: 0,
            });
            
            world.consolidate_duplicates();
            
            // Should have merged into one word
            let count = world.words.iter().filter(|w| w.text == text).count();
            assert_eq!(count, 1);
            
            // Mass should be combined
            let merged = world.words.iter().find(|w| w.text == text).unwrap();
            assert!((merged.mass_total - 15.0).abs() < 1e-6);
        }
    }

    mod trail {
        use super::*;

        #[test]
        fn trail_records_position() {
            let mut word = Word {
                id: 1,
                text: "test".to_string(),
                pos: Vec2::new(5.0, 5.0),
                vel: Vec2::ZERO,
                radius: 1.0,
                mass_total: 10.0,
                mass_visible: 10.0,
                mass_dust: 0.0,
                flags: WordFlags { can_split: false },
                trail: [Vec2::ZERO; TRAIL_LEN],
                trail_head: 0,
                trail_len: 0,
            };
            
            World::record_trail(&mut word);
            
            assert_eq!(word.trail_len, 1);
            assert_eq!(word.trail[word.trail_head], word.pos);
        }

        #[test]
        fn trail_wraps_around() {
            let mut word = Word {
                id: 1,
                text: "test".to_string(),
                pos: Vec2::ZERO,
                vel: Vec2::ZERO,
                radius: 1.0,
                mass_total: 10.0,
                mass_visible: 10.0,
                mass_dust: 0.0,
                flags: WordFlags { can_split: false },
                trail: [Vec2::ZERO; TRAIL_LEN],
                trail_head: 0,
                trail_len: 0,
            };
            
            for i in 0..(TRAIL_LEN * 2) {
                word.pos = Vec2::new(i as f32, 0.0);
                World::record_trail(&mut word);
            }
            
            // Trail length should cap at TRAIL_LEN
            assert_eq!(word.trail_len, TRAIL_LEN);
        }
    }
}
