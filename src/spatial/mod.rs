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

#[cfg(test)]
mod tests {
    use super::*;

    mod spatial_hash_new {
        use super::*;

        #[test]
        fn creates_with_valid_cell_size() {
            let hash = SpatialHash::new(10.0);
            assert_eq!(hash.cell_size, 10.0);
        }

        #[test]
        #[should_panic(expected = "cell_size must be positive and finite")]
        fn panics_with_zero_cell_size() {
            SpatialHash::new(0.0);
        }

        #[test]
        #[should_panic(expected = "cell_size must be positive and finite")]
        fn panics_with_negative_cell_size() {
            SpatialHash::new(-1.0);
        }

        #[test]
        #[should_panic(expected = "cell_size must be positive and finite")]
        fn panics_with_infinite_cell_size() {
            SpatialHash::new(f32::INFINITY);
        }
    }

    mod spatial_hash_rebuild {
        use super::*;

        #[test]
        fn builds_index_from_positions() {
            let mut hash = SpatialHash::new(10.0);
            let positions = vec![
                Vec2::new(5.0, 5.0),
                Vec2::new(15.0, 5.0),
                Vec2::new(5.0, 15.0),
            ];
            hash.rebuild(&positions);
            assert!(!hash.cells.is_empty());
        }

        #[test]
        fn groups_points_in_same_cell() {
            let mut hash = SpatialHash::new(10.0);
            let positions = vec![
                Vec2::new(1.0, 1.0),
                Vec2::new(2.0, 2.0),
                Vec2::new(3.0, 3.0),
            ];
            hash.rebuild(&positions);
            let cell = hash.cells.get(&(0, 0)).expect("Cell (0,0) should exist");
            assert_eq!(cell.len(), 3);
        }

        #[test]
        fn clears_previous_data_on_rebuild() {
            let mut hash = SpatialHash::new(10.0);
            hash.rebuild(&[Vec2::new(5.0, 5.0)]);
            hash.rebuild(&[Vec2::new(15.0, 15.0)]);
            assert!(hash.cells.get(&(0, 0)).is_none());
            assert!(hash.cells.get(&(1, 1)).is_some());
        }
    }

    mod spatial_hash_query_neighbors {
        use super::*;

        #[test]
        fn finds_point_in_same_cell() {
            let mut hash = SpatialHash::new(10.0);
            hash.rebuild(&[Vec2::new(5.0, 5.0)]);
            let mut out = Vec::new();
            hash.query_neighbors(Vec2::new(6.0, 6.0), &mut out);
            assert!(out.contains(&0));
        }

        #[test]
        fn finds_points_in_adjacent_cells() {
            let mut hash = SpatialHash::new(10.0);
            let positions = vec![
                Vec2::new(5.0, 5.0),   // Cell (0, 0)
                Vec2::new(15.0, 5.0),  // Cell (1, 0)
                Vec2::new(5.0, 15.0),  // Cell (0, 1)
            ];
            hash.rebuild(&positions);
            let mut out = Vec::new();
            hash.query_neighbors(Vec2::new(9.0, 9.0), &mut out);
            // Query from edge of cell (0,0), should find all in 3x3 grid
            assert!(out.contains(&0));
            assert!(out.contains(&1));
            assert!(out.contains(&2));
        }

        #[test]
        fn returns_empty_for_isolated_query() {
            let mut hash = SpatialHash::new(10.0);
            hash.rebuild(&[Vec2::new(5.0, 5.0)]);
            let mut out = Vec::new();
            hash.query_neighbors(Vec2::new(100.0, 100.0), &mut out);
            assert!(out.is_empty());
        }
    }

    mod spatial_hash_query_neighbors_range {
        use super::*;

        #[test]
        fn range_zero_queries_only_current_cell() {
            let mut hash = SpatialHash::new(10.0);
            let positions = vec![
                Vec2::new(5.0, 5.0),   // Cell (0, 0)
                Vec2::new(15.0, 5.0),  // Cell (1, 0)
            ];
            hash.rebuild(&positions);
            let mut out = Vec::new();
            hash.query_neighbors_range(Vec2::new(5.0, 5.0), 0, &mut out);
            assert!(out.contains(&0));
            assert!(!out.contains(&1));
        }

        #[test]
        fn larger_range_covers_more_cells() {
            let mut hash = SpatialHash::new(10.0);
            let positions = vec![
                Vec2::new(5.0, 5.0),   // Cell (0, 0)
                Vec2::new(25.0, 5.0),  // Cell (2, 0)
                Vec2::new(35.0, 5.0),  // Cell (3, 0)
            ];
            hash.rebuild(&positions);
            let mut out = Vec::new();
            hash.query_neighbors_range(Vec2::new(5.0, 5.0), 2, &mut out);
            assert!(out.contains(&0));
            assert!(out.contains(&1));
            assert!(!out.contains(&2));
        }

        #[test]
        fn negative_range_treated_as_zero() {
            let mut hash = SpatialHash::new(10.0);
            let positions = vec![
                Vec2::new(5.0, 5.0),
                Vec2::new(15.0, 5.0),
            ];
            hash.rebuild(&positions);
            let mut out = Vec::new();
            hash.query_neighbors_range(Vec2::new(5.0, 5.0), -1, &mut out);
            assert!(out.contains(&0));
            assert!(!out.contains(&1));
        }
    }

    mod spatial_hash_clear {
        use super::*;

        #[test]
        fn removes_all_cells() {
            let mut hash = SpatialHash::new(10.0);
            hash.rebuild(&[Vec2::new(5.0, 5.0)]);
            hash.clear();
            assert!(hash.cells.is_empty());
        }
    }
}
