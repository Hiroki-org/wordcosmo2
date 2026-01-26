use std::collections::HashMap;

use crate::types::Vec2;

#[derive(Debug)]
pub struct SpatialHash {
    cell_size: f32,
    cells: HashMap<(i32, i32), Vec<usize>>,
}

impl SpatialHash {
    pub fn new(cell_size: f32) -> Self {
        assert!(
            cell_size.is_finite() && cell_size > 0.0,
            "cell_size must be positive and finite"
        );
        Self {
            cell_size,
            cells: HashMap::new(),
        }
    }

    pub fn clear(&mut self) {
        self.cells.clear();
    }

    pub fn rebuild(&mut self, positions: &[Vec2]) {
        self.cells.clear();
        for (idx, pos) in positions.iter().enumerate() {
            let key = self.cell_key(*pos);
            self.cells.entry(key).or_default().push(idx);
        }
    }

    pub fn query_neighbors(&self, pos: Vec2, out: &mut Vec<usize>) {
        self.query_neighbors_range(pos, 1, out);
    }

    pub fn query_neighbors_range(&self, pos: Vec2, range: i32, out: &mut Vec<usize>) {
        out.clear();
        let (cx, cy) = self.cell_key(pos);
        let range = range.max(0);
        for dy in -range..=range {
            for dx in -range..=range {
                let key = (cx + dx, cy + dy);
                if let Some(indices) = self.cells.get(&key) {
                    out.extend_from_slice(indices);
                }
            }
        }
    }

    fn cell_key(&self, pos: Vec2) -> (i32, i32) {
        let cx = (pos.x / self.cell_size).floor() as i32;
        let cy = (pos.y / self.cell_size).floor() as i32;
        (cx, cy)
    }
}
