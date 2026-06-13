mod cell;
mod geometry;
mod poisson;
mod staggered_mac;

pub use cell::CellFlowGrid;
pub use staggered_mac::{MacFace, StaggeredMacFlowGrid};

pub use geometry::{
    FlowFromGeometryConfig, FluidGeometryConfig, apply_fluid_boundary, apply_fluid_exterior,
    create_flow_grid_from_geometry,
};

pub use poisson::{FlowPoissonCorrectionConfig, poisson_correct_flow_grid};
