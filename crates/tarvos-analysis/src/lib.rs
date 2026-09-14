mod lower;
mod profile;
mod types;

pub use lower::lower_module;
pub use profile::{analyze_module, ProfileStats};
pub use types::{infer_expr_type, TypeInference};
pub mod native_detector;
pub mod native_specialization;
pub mod vectorize;
