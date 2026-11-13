mod lower;
mod profile;
mod stdlib;
mod types;

pub use lower::lower_module;
pub use profile::{analyze_module, ProfileStats};
pub(crate) use stdlib::{
    module_supported, native_builtin_method, native_constant, native_function,
};
pub use types::{infer_expr_type, TypeInference};
pub mod dependency;
pub mod native_detector;
pub mod native_specialization;
pub mod vectorize;
