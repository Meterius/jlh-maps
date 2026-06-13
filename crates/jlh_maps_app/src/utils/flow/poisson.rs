use super::{MacFace, StaggeredMacFlowGrid};
use bevy::math::USizeVec2;

// Poisson utilities project a MAC flow field toward lower divergence while preserving fixed faces.

pub struct FlowPoissonCorrectionConfig {
    pub iterations: usize,
    pub tolerance: f64,
}

pub fn poisson_correct_flow_grid(
    mut grid: StaggeredMacFlowGrid,
    config: FlowPoissonCorrectionConfig,
) -> StaggeredMacFlowGrid {
    let dim = grid.dim();
    if dim.x == 0 || dim.y == 0 {
        return grid;
    }

    let spacing = 1.0;

    // Measure current MAC divergence using constrained boundary faces as fixed values.

    let mut divergence = mac_face_divergence(&grid, spacing);
    remove_mean(&mut divergence);

    // Poisson projection solves pressure whose gradient cancels discrete divergence.

    let potential = solve_constrained_full_grid_poisson(
        &grid,
        spacing,
        &divergence,
        config.iterations,
        config.tolerance,
    );

    // Subtract grad(pressure) from unconstrained faces while preserving fixed boundaries.

    apply_mac_pressure_correction(&mut grid, spacing, &potential);
    grid.clear_constraints();

    grid
}

fn mac_face_divergence(grid: &StaggeredMacFlowGrid, spacing: f64) -> Vec<f64> {
    let dim = grid.dim();
    let mut divergence = vec![0.0; dim.x * dim.y];

    // Cell-based divergence from outward MAC face flux differences.

    for (coord, index) in grid.points() {
        divergence[index] = (grid.face(MacFace::U, coord.x + 1, coord.y)
            - grid.face(MacFace::U, coord.x, coord.y)
            + grid.face(MacFace::V, coord.x, coord.y + 1)
            - grid.face(MacFace::V, coord.x, coord.y))
            / spacing;
    }

    divergence
}

fn solve_constrained_full_grid_poisson(
    grid: &StaggeredMacFlowGrid,
    spacing: f64,
    rhs: &[f64],
    iterations: usize,
    tolerance: f64,
) -> Vec<f64> {
    let dim = grid.dim();
    let coefficient = 1.0 / (spacing * spacing);
    let mut potential = vec![0.0; dim.x * dim.y];

    for _ in 0..iterations {
        let previous = potential.clone();

        // Jacobi relaxation over the cell-centered pressure potential.

        for (coord, index) in grid.points() {
            let mut weighted_sum = 0.0;
            let mut coefficient_sum = 0.0;

            if coord.x > 0 && !grid.is_face_fixed(MacFace::U, coord.x, coord.y) {
                weighted_sum += previous[grid.coord_to_index(USizeVec2::new(coord.x - 1, coord.y))]
                    * coefficient;
                coefficient_sum += coefficient;
            }
            if coord.x + 1 < dim.x && !grid.is_face_fixed(MacFace::U, coord.x + 1, coord.y) {
                weighted_sum += previous[grid.coord_to_index(USizeVec2::new(coord.x + 1, coord.y))]
                    * coefficient;
                coefficient_sum += coefficient;
            }
            if coord.y > 0 && !grid.is_face_fixed(MacFace::V, coord.x, coord.y) {
                weighted_sum += previous[grid.coord_to_index(USizeVec2::new(coord.x, coord.y - 1))]
                    * coefficient;
                coefficient_sum += coefficient;
            }
            if coord.y + 1 < dim.y && !grid.is_face_fixed(MacFace::V, coord.x, coord.y + 1) {
                weighted_sum += previous[grid.coord_to_index(USizeVec2::new(coord.x, coord.y + 1))]
                    * coefficient;
                coefficient_sum += coefficient;
            }

            if coefficient_sum > 0.0 {
                potential[index] = (weighted_sum - rhs[index]) / coefficient_sum;
            }
        }

        // Neumann-like systems need an arbitrary mean fixed for stable convergence checks.

        remove_mean(&mut potential);
        let max_delta = potential
            .iter()
            .zip(&previous)
            .map(|(current, previous)| (current - previous).abs())
            .fold(0.0_f64, f64::max);
        if max_delta < tolerance {
            break;
        }
    }

    potential
}

fn apply_mac_pressure_correction(grid: &mut StaggeredMacFlowGrid, spacing: f64, potential: &[f64]) {
    let dim = grid.dim();

    // Correct horizontal faces from pressure differences between left/right cells.

    for y in 0..dim.y {
        for x in 1..dim.x {
            if !grid.is_face_fixed(MacFace::U, x, y) {
                let correction = (potential[grid.coord_to_index(USizeVec2::new(x, y))]
                    - potential[grid.coord_to_index(USizeVec2::new(x - 1, y))])
                    / spacing;
                *grid.face_mut(MacFace::U, x, y) -= correction;
            }
        }
    }

    // Correct vertical faces from pressure differences between top/bottom cells.

    for y in 1..dim.y {
        for x in 0..dim.x {
            if !grid.is_face_fixed(MacFace::V, x, y) {
                let correction = (potential[grid.coord_to_index(USizeVec2::new(x, y))]
                    - potential[grid.coord_to_index(USizeVec2::new(x, y - 1))])
                    / spacing;
                *grid.face_mut(MacFace::V, x, y) -= correction;
            }
        }
    }
}

fn remove_mean(values: &mut [f64]) {
    if values.is_empty() {
        return;
    }

    let mean = values.iter().sum::<f64>() / values.len() as f64;
    for value in values {
        *value -= mean;
    }
}
