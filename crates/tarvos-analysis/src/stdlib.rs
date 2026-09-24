use tarvos_types::Type;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeFunction {
    pub rust_name: &'static str,
    pub return_type: Type,
}

pub fn module_supported(module: &str) -> bool {
    matches!(module, "math" | "time" | "os.path")
}

pub fn native_function(module: &str, name: &str) -> Option<NativeFunction> {
    let (rust_name, return_type) = match (module, name) {
        ("math", "sqrt") => ("tarvos_math_sqrt", Type::Float),
        ("math", "sin") => ("tarvos_math_sin", Type::Float),
        ("math", "cos") => ("tarvos_math_cos", Type::Float),
        ("math", "tan") => ("tarvos_math_tan", Type::Float),
        ("math", "asin") => ("tarvos_math_asin", Type::Float),
        ("math", "acos") => ("tarvos_math_acos", Type::Float),
        ("math", "atan") => ("tarvos_math_atan", Type::Float),
        ("math", "exp") => ("tarvos_math_exp", Type::Float),
        ("math", "log") => ("tarvos_math_log", Type::Float),
        ("math", "log10") => ("tarvos_math_log10", Type::Float),
        ("math", "floor") => ("tarvos_math_floor", Type::Int),
        ("math", "ceil") => ("tarvos_math_ceil", Type::Int),
        ("math", "fabs") => ("tarvos_math_fabs", Type::Float),
        ("math", "pow") => ("tarvos_math_pow", Type::Float),
        ("math", "hypot") => ("tarvos_math_hypot", Type::Float),
        ("math", "atan2") => ("tarvos_math_atan2", Type::Float),
        ("math", "isfinite") => ("tarvos_math_isfinite", Type::Bool),
        ("math", "isnan") => ("tarvos_math_isnan", Type::Bool),
        ("math", "isinf") => ("tarvos_math_isinf", Type::Bool),
        ("time", "perf_counter") => ("tarvos_perf_counter", Type::Float),
        ("time", "monotonic") => ("tarvos_perf_counter", Type::Float),
        ("time", "time") => ("tarvos_time", Type::Float),
        ("time", "sleep") => ("tarvos_sleep", Type::None),
        ("os.path", "join") => ("tarvos_os_path_join", Type::String),
        ("os.path", "basename") => ("tarvos_os_path_basename", Type::String),
        ("os.path", "dirname") => ("tarvos_os_path_dirname", Type::String),
        ("os.path", "exists") => ("tarvos_os_path_exists", Type::Bool),
        ("os.path", "isfile") => ("tarvos_os_path_isfile", Type::Bool),
        ("os.path", "isdir") => ("tarvos_os_path_isdir", Type::Bool),
        _ => return None,
    };
    Some(NativeFunction {
        rust_name,
        return_type,
    })
}

pub fn native_constant(module: &str, name: &str) -> Option<f64> {
    match (module, name) {
        ("math", "pi") => Some(std::f64::consts::PI),
        ("math", "e") => Some(std::f64::consts::E),
        ("math", "tau") => Some(std::f64::consts::TAU),
        ("math", "inf") => Some(f64::INFINITY),
        ("math", "nan") => Some(f64::NAN),
        _ => None,
    }
}
