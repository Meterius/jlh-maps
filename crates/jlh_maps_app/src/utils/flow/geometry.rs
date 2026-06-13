use super::{MacFace, StaggeredMacFlowGrid};
use bevy::math::{DVec2, USizeVec2, dvec2};
use geo::Contains;
use itertools::Itertools;
use rstar::primitives::Line;
use rstar::{AABB, RTree};

// Geometry utilities connect water polygons and waterway alignments to MAC flow grids.

pub struct FlowFromGeometryConfig {
    pub bounds: (DVec2, DVec2),
    pub fluid_region: geo_types::MultiPolygon,
    pub flow_alignments: Vec<geo_types::LineString>,

    pub sigma: f64,
    pub max_nearest_neighbor: usize,
}

pub fn create_flow_grid_from_geometry(
    grid_dim: USizeVec2,
    config: FlowFromGeometryConfig,
) -> StaggeredMacFlowGrid {
    // Build helper spatial index over waterway alignment segments.

    let mut alignment_edges = Vec::new();

    for alignment in &config.flow_alignments {
        let coords = alignment.coords().collect::<Vec<_>>();
        coords.windows(2).for_each(|pair| {
            alignment_edges.push(Line::<[f64; 2]> {
                from: [pair[0].x, pair[0].y],
                to: [pair[1].x, pair[1].y],
            });
        });
    }

    let alignment_edges = RTree::bulk_load(alignment_edges);
    let dim_x = grid_dim.x;
    let dim_y = grid_dim.y;
    let mut grid = StaggeredMacFlowGrid::zero(grid_dim);

    // Sample horizontal MAC faces from nearby alignment segment directions.

    for y in 0..dim_y {
        for x in 0..=dim_x {
            let pos = grid_position(
                config.bounds,
                dvec2(x as f64 / dim_x as f64, (y as f64 + 0.5) / dim_y as f64),
            );
            let direction = sample_alignment_direction(
                &alignment_edges,
                pos,
                config.sigma,
                config.max_nearest_neighbor,
            )
            .x;
            *grid.face_mut(MacFace::U, x, y) = direction;
        }
    }

    // Sample vertical MAC faces from nearby alignment segment directions.

    for y in 0..=dim_y {
        for x in 0..dim_x {
            let pos = grid_position(
                config.bounds,
                dvec2((x as f64 + 0.5) / dim_x as f64, y as f64 / dim_y as f64),
            );
            let direction = sample_alignment_direction(
                &alignment_edges,
                pos,
                config.sigma,
                config.max_nearest_neighbor,
            )
            .y;
            *grid.face_mut(MacFace::V, x, y) = direction;
        }
    }

    grid
}

fn sample_alignment_direction(
    alignment_edges: &RTree<Line<[f64; 2]>>,
    pos: DVec2,
    sigma: f64,
    max_nearest_neighbor: usize,
) -> DVec2 {
    let mut direction_sum = DVec2::ZERO;
    let mut count = 0;

    for (alignment, dist_sq) in
        alignment_edges.nearest_neighbor_iter_with_distance_2(&[pos.x, pos.y])
    {
        let weight = (-dist_sq / 2.0 * (sigma * sigma)).exp();

        let from = dvec2(alignment.from[0], alignment.from[1]);
        let to = dvec2(alignment.to[0], alignment.to[1]);
        direction_sum += (from - to).normalize_or_zero() * weight;

        count += 1;
        if count >= max_nearest_neighbor {
            break;
        }
    }

    direction_sum.normalize_or_zero()
}

pub struct FluidGeometryConfig<'a> {
    pub bounds: (DVec2, DVec2),
    pub fluid_region: &'a geo_types::MultiPolygon,
}

pub fn apply_fluid_boundary(grid: &mut StaggeredMacFlowGrid, config: FluidGeometryConfig<'_>) {
    let dim = grid.dim();
    let dim_x = dim.x;
    let dim_y = dim.y;
    if dim_x == 0 || dim_y == 0 {
        return;
    }

    // Build helper data structures for repeated boundary-cell queries.

    let border_edges = fluid_border_edges(config.fluid_region);
    let cell_size = (config.bounds.1 - config.bounds.0) / dim.as_dvec2();

    // Mutate the exterior-side neighbor of each boundary cell using the boundary cell's projected flow.

    let mut boundary_constraints = vec![None; dim_x * dim_y];
    for y in 0..dim_y {
        for x in 0..dim_x {
            let coord = USizeVec2::new(x, y);
            let min = config.bounds.0 + coord.as_dvec2() * cell_size;
            let max = min + cell_size;
            let center = min + cell_size * 0.5;

            let Some(edge) =
                nearest_boundary_edge_intersecting_cell(&border_edges, min, max, center)
            else {
                continue;
            };

            for target_coord in outer_neighbor_cells(config.fluid_region, config.bounds, coord, dim)
            {
                let target_center = cell_center(config.bounds, target_coord, dim);
                let distance_sq = segment_distance_sq_to_point(
                    DVec2::from_array(edge.from),
                    DVec2::from_array(edge.to),
                    target_center,
                );
                let target_index = grid.coord_to_index(target_coord);

                if boundary_constraints[target_index]
                    .map(|constraint: BoundaryConstraint| distance_sq < constraint.distance_sq)
                    .unwrap_or(true)
                {
                    boundary_constraints[target_index] = Some(BoundaryConstraint {
                        source_coord: coord,
                        edge,
                        distance_sq,
                    });
                }
            }
        }
    }

    for y in 0..dim_y {
        for x in 0..dim_x {
            let target_coord = USizeVec2::new(x, y);
            let target_index = grid.coord_to_index(target_coord);
            let Some(constraint) = boundary_constraints[target_index] else {
                continue;
            };

            let tangent = (DVec2::from_array(constraint.edge.to)
                - DVec2::from_array(constraint.edge.from))
            .normalize_or_zero();
            let projected = grid
                .get_cell(constraint.source_coord)
                .project_onto_normalized(tangent);
            grid.set_cell_fixed(target_coord, projected);
        }
    }
}

#[derive(Clone, Copy)]
struct BoundaryConstraint {
    source_coord: USizeVec2,
    edge: Line<[f64; 2]>,
    distance_sq: f64,
}

fn outer_neighbor_cells(
    fluid_region: &geo_types::MultiPolygon,
    bounds: (DVec2, DVec2),
    coord: USizeVec2,
    dim: USizeVec2,
) -> Vec<USizeVec2> {
    let mut neighbors = Vec::with_capacity(4);
    let candidates = [
        coord.x.checked_sub(1).map(|x| USizeVec2::new(x, coord.y)),
        (coord.x + 1 < dim.x).then_some(USizeVec2::new(coord.x + 1, coord.y)),
        coord.y.checked_sub(1).map(|y| USizeVec2::new(coord.x, y)),
        (coord.y + 1 < dim.y).then_some(USizeVec2::new(coord.x, coord.y + 1)),
    ];

    for candidate in candidates.into_iter().flatten() {
        let center = cell_center(bounds, candidate, dim);
        if !fluid_region.contains(&geo_types::Point::new(center.x, center.y)) {
            neighbors.push(candidate);
        }
    }

    neighbors
}

pub fn apply_fluid_exterior(grid: &mut StaggeredMacFlowGrid, config: FluidGeometryConfig<'_>) {
    let dim = grid.dim();
    let dim_x = dim.x;
    let dim_y = dim.y;
    if dim_x == 0 || dim_y == 0 {
        return;
    }

    // Build helper data structures for outside/inside classification.

    let border_edges = fluid_border_edges(config.fluid_region);
    let cell_size = (config.bounds.1 - config.bounds.0) / dim.as_dvec2();

    // Mutate exterior cells to fixed zero flow.

    for y in 0..dim_y {
        for x in 0..dim_x {
            let coord = USizeVec2::new(x, y);
            let min = config.bounds.0 + coord.as_dvec2() * cell_size;
            let max = min + cell_size;

            if cell_completely_outside_fluid(config.fluid_region, &border_edges, min, max) {
                grid.set_cell_fixed(coord, DVec2::ZERO);
            }
        }
    }
}

fn fluid_border_edges(fluid_region: &geo_types::MultiPolygon) -> RTree<Line<[f64; 2]>> {
    // Helper spatial index over all exterior and interior fluid boundary segments.

    let mut border_edges = Vec::new();

    for region in fluid_region {
        for boundary in region
            .interiors()
            .iter()
            .chain(std::iter::once(region.exterior()))
        {
            for (a, b) in boundary.coords().tuple_windows() {
                border_edges.push(Line::<[f64; 2]> {
                    from: [a.x, a.y],
                    to: [b.x, b.y],
                });
            }
        }
    }

    RTree::bulk_load(border_edges)
}

fn nearest_boundary_edge_intersecting_cell(
    border_edges: &RTree<Line<[f64; 2]>>,
    min: DVec2,
    max: DVec2,
    center: DVec2,
) -> Option<Line<[f64; 2]>> {
    // Helper query: use R-tree envelopes first, then exact segment/AABB intersection.

    let envelope = AABB::from_corners([min.x, min.y], [max.x, max.y]);
    let mut nearest_intersecting_edge = None;

    for edge in border_edges.locate_in_envelope_intersecting(&envelope) {
        let edge_from = DVec2::from_array(edge.from);
        let edge_to = DVec2::from_array(edge.to);
        if !segment_intersects_aabb(edge_from, edge_to, min, max) {
            continue;
        }

        let distance_sq = segment_distance_sq_to_point(edge_from, edge_to, center);
        if nearest_intersecting_edge
            .map(|(_, nearest_distance_sq)| distance_sq < nearest_distance_sq)
            .unwrap_or(true)
        {
            nearest_intersecting_edge = Some((*edge, distance_sq));
        }
    }

    nearest_intersecting_edge.map(|(edge, _)| edge)
}

fn cell_completely_outside_fluid(
    fluid_region: &geo_types::MultiPolygon,
    border_edges: &RTree<Line<[f64; 2]>>,
    min: DVec2,
    max: DVec2,
) -> bool {
    // A cell with a boundary crossing is partial, not completely exterior.

    let center = (min + max) * 0.5;
    if nearest_boundary_edge_intersecting_cell(border_edges, min, max, center).is_some() {
        return false;
    }

    [min, dvec2(max.x, min.y), max, dvec2(min.x, max.y), center]
        .into_iter()
        .all(|point| !fluid_region.contains(&geo_types::Point::new(point.x, point.y)))
}

fn cell_center(bounds: (DVec2, DVec2), coord: USizeVec2, dim: USizeVec2) -> DVec2 {
    let cell_size = (bounds.1 - bounds.0) / dim.as_dvec2();
    bounds.0 + (coord.as_dvec2() + DVec2::splat(0.5)) * cell_size
}

fn grid_position(bounds: (DVec2, DVec2), normalized: DVec2) -> DVec2 {
    bounds.0 + (bounds.1 - bounds.0) * normalized
}

fn segment_intersects_aabb(from: DVec2, to: DVec2, min: DVec2, max: DVec2) -> bool {
    let delta = to - from;
    let mut t_min = 0.0;
    let mut t_max = 1.0;

    clip_segment_axis(from.x, delta.x, min.x, max.x, &mut t_min, &mut t_max)
        && clip_segment_axis(from.y, delta.y, min.y, max.y, &mut t_min, &mut t_max)
}

fn clip_segment_axis(
    origin: f64,
    delta: f64,
    min: f64,
    max: f64,
    t_min: &mut f64,
    t_max: &mut f64,
) -> bool {
    if delta.abs() <= f64::EPSILON {
        return min <= origin && origin <= max;
    }

    let inv_delta = 1.0 / delta;
    let mut near = (min - origin) * inv_delta;
    let mut far = (max - origin) * inv_delta;
    if near > far {
        std::mem::swap(&mut near, &mut far);
    }

    *t_min = t_min.max(near);
    *t_max = t_max.min(far);
    *t_min <= *t_max
}

fn segment_distance_sq_to_point(from: DVec2, to: DVec2, point: DVec2) -> f64 {
    let segment = to - from;
    let segment_length_sq = segment.length_squared();
    if segment_length_sq <= f64::EPSILON {
        return point.distance_squared(from);
    }

    let t = ((point - from).dot(segment) / segment_length_sq).clamp(0.0, 1.0);
    point.distance_squared(from + segment * t)
}
