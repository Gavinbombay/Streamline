//! Hands-off input staging, runner interface, cleanup verification, cross-family QC
//! dispatch, and trajectory JSONL export for Streamline.

pub mod error;
pub mod runner;
pub mod staging;
pub mod trajectory;

#[cfg(test)]
mod tests;

pub use error::ConveyorError;
pub use runner::{
    execute_conveyor_blocking, verify_cross_family, DispatchOutcome, DispatchRequest,
};
pub use staging::{
    compute_tree_manifest, stage_readonly_inputs, unfreeze_readonly_dir, verify_cleanup_clean,
    verify_no_symlink_components, StagedManifest,
};
pub use trajectory::{append_trajectory_jsonl, TrajectoryExportInput};
