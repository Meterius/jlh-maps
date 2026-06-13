use super::CellFlowGrid;
use bevy::math::{DVec2, USizeVec2, dvec2};

// MAC faces split vector components across the grid: U is horizontal flow, V is vertical flow.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MacFace {
    U,
    V,
}

// A staggered MAC grid stores horizontal flow on vertical faces and vertical flow on horizontal faces.

#[derive(Clone)]
pub struct StaggeredMacFlowGrid {
    dim: USizeVec2,
    u_faces: Vec<f64>,
    v_faces: Vec<f64>,
    u_face_constraints: Vec<bool>,
    v_face_constraints: Vec<bool>,
}

impl StaggeredMacFlowGrid {
    // Face-based storage allocation.

    pub fn zero(dim: USizeVec2) -> Self {
        let dim_x = dim.x;
        let dim_y = dim.y;
        Self {
            dim,
            u_faces: vec![0.0; dim_y * (dim_x + 1)],
            v_faces: vec![0.0; (dim_y + 1) * dim_x],
            u_face_constraints: vec![false; dim_y * (dim_x + 1)],
            v_face_constraints: vec![false; (dim_y + 1) * dim_x],
        }
    }

    // Cell-based metadata and iteration.

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

    // Cell-based operations translated onto neighboring MAC faces.

    pub fn get_cell(&self, coord: USizeVec2) -> DVec2 {
        dvec2(
            0.5 * (self.face(MacFace::U, coord.x, coord.y)
                + self.face(MacFace::U, coord.x + 1, coord.y)),
            0.5 * (self.face(MacFace::V, coord.x, coord.y)
                + self.face(MacFace::V, coord.x, coord.y + 1)),
        )
    }

    pub fn set_cell(&mut self, coord: USizeVec2, value: DVec2) {
        self.set_cell_inner(coord, value, false);
    }

    pub fn set_cell_fixed(&mut self, coord: USizeVec2, value: DVec2) {
        self.set_cell_inner(coord, value, true);
    }

    // Cell-grid conversion for visualization/export; partial cells have some, but not all, fixed faces.

    pub fn to_cell_flow_grid(&self, include_partial_cells: bool) -> CellFlowGrid {
        self.to_cell_flow_grid_window(USizeVec2::ZERO, self.dim, include_partial_cells)
    }

    pub fn to_cell_flow_grid_window(
        &self,
        origin: USizeVec2,
        dim: USizeVec2,
        include_partial_cells: bool,
    ) -> CellFlowGrid {
        assert!(origin.x + dim.x <= self.dim.x);
        assert!(origin.y + dim.y <= self.dim.y);

        let has_constraints = self.has_constraints();
        let mut data = vec![None; dim.x * dim.y];

        for y in 0..dim.y {
            for x in 0..dim.x {
                let coord = USizeVec2::new(origin.x + x, origin.y + y);
                let index = x + y * dim.x;

                if has_constraints {
                    let fixed_face_count = self.fixed_cell_face_count(coord);
                    if fixed_face_count == 0 || (fixed_face_count < 4 && !include_partial_cells) {
                        continue;
                    }
                }

                data[index] = Some(self.get_cell(coord));
            }
        }

        CellFlowGrid::from_cells(dim, data)
    }

    pub fn get_face(&self, face: MacFace, x: usize, y: usize) -> f64 {
        self.face(face, x, y)
    }

    // Face-based operations for geometry setup and numerical solvers.

    pub(super) fn face(&self, face: MacFace, x: usize, y: usize) -> f64 {
        let index = self.face_index(face, x, y);
        match face {
            MacFace::U => self.u_faces[index],
            MacFace::V => self.v_faces[index],
        }
    }

    pub(super) fn face_mut(&mut self, face: MacFace, x: usize, y: usize) -> &mut f64 {
        let index = self.face_index(face, x, y);
        match face {
            MacFace::U => &mut self.u_faces[index],
            MacFace::V => &mut self.v_faces[index],
        }
    }

    pub(super) fn is_face_fixed(&self, face: MacFace, x: usize, y: usize) -> bool {
        let index = self.face_index(face, x, y);
        match face {
            MacFace::U => self.u_face_constraints[index],
            MacFace::V => self.v_face_constraints[index],
        }
    }

    pub(super) fn face_constraint_mut(&mut self, face: MacFace, x: usize, y: usize) -> &mut bool {
        let index = self.face_index(face, x, y);
        match face {
            MacFace::U => &mut self.u_face_constraints[index],
            MacFace::V => &mut self.v_face_constraints[index],
        }
    }

    pub(super) fn clear_constraints(&mut self) {
        self.u_face_constraints.fill(false);
        self.v_face_constraints.fill(false);
    }

    fn set_cell_inner(&mut self, coord: USizeVec2, value: DVec2, fixed: bool) {
        *self.face_mut(MacFace::U, coord.x, coord.y) = value.x;
        *self.face_mut(MacFace::U, coord.x + 1, coord.y) = value.x;
        *self.face_constraint_mut(MacFace::U, coord.x, coord.y) = fixed;
        *self.face_constraint_mut(MacFace::U, coord.x + 1, coord.y) = fixed;

        *self.face_mut(MacFace::V, coord.x, coord.y) = value.y;
        *self.face_mut(MacFace::V, coord.x, coord.y + 1) = value.y;
        *self.face_constraint_mut(MacFace::V, coord.x, coord.y) = fixed;
        *self.face_constraint_mut(MacFace::V, coord.x, coord.y + 1) = fixed;
    }

    fn fixed_cell_face_count(&self, coord: USizeVec2) -> usize {
        [
            self.is_face_fixed(MacFace::U, coord.x, coord.y),
            self.is_face_fixed(MacFace::U, coord.x + 1, coord.y),
            self.is_face_fixed(MacFace::V, coord.x, coord.y),
            self.is_face_fixed(MacFace::V, coord.x, coord.y + 1),
        ]
        .into_iter()
        .filter(|fixed| *fixed)
        .count()
    }

    fn has_constraints(&self) -> bool {
        self.u_face_constraints
            .iter()
            .chain(&self.v_face_constraints)
            .any(|fixed| *fixed)
    }

    fn face_index(&self, face: MacFace, x: usize, y: usize) -> usize {
        match face {
            MacFace::U => x + y * (self.dim.x + 1),
            MacFace::V => x + y * self.dim.x,
        }
    }
}
