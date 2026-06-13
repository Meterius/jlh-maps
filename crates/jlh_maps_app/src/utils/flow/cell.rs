use bevy::math::{DVec2, USizeVec2};

// Cell grids are sparse cell-center views used for visualization and export, not simulation.

#[derive(Clone)]
pub struct CellFlowGrid {
    dim: USizeVec2,
    data: Vec<Option<DVec2>>,
}

impl CellFlowGrid {
    // Construction stays inside `flow`; callers receive cell values through methods.

    pub(super) fn from_cells(dim: USizeVec2, data: Vec<Option<DVec2>>) -> Self {
        Self { dim, data }
    }

    // Cell-based metadata and indexing.

    pub fn dim(&self) -> USizeVec2 {
        self.dim
    }

    pub fn coord_to_index(&self, coord: USizeVec2) -> usize {
        coord.x + coord.y * self.dim.x
    }

    pub fn points(&self) -> impl Iterator<Item = (USizeVec2, usize)> {
        let dim_x = self.dim.x;
        let dim_y = self.dim.y;

        (0..dim_x).flat_map(move |x| (0..dim_y).map(move |y| (USizeVec2::new(x, y), x + y * dim_x)))
    }

    // Cell-based value access.

    pub fn get_cell(&self, coord: USizeVec2) -> Option<DVec2> {
        self.data[self.coord_to_index(coord)]
    }
}
