use tarvos_types::Type;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeFunction {
    pub rust_name: &'static str,
    pub return_type: Type,
}

pub fn module_supported(module: &str) -> bool {
    matches!(
        module,
        "math" | "time" | "os" | "os.path" | "json" | "statistics"
    )
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
        ("os", "getcwd") => ("tarvos_os_getcwd", Type::String),
        ("os", "listdir") => ("tarvos_os_listdir", Type::Array(Box::new(Type::String))),
        ("os", "mkdir") => ("tarvos_os_mkdir", Type::None),
        ("os", "makedirs") => ("tarvos_os_makedirs", Type::None),
        ("os", "chdir") => ("tarvos_os_chdir", Type::None),
        ("os.path", "join") => ("tarvos_os_path_join", Type::String),
        ("os.path", "basename") => ("tarvos_os_path_basename", Type::String),
        ("os.path", "dirname") => ("tarvos_os_path_dirname", Type::String),
        ("os.path", "exists") => ("tarvos_os_path_exists", Type::Bool),
        ("os.path", "isfile") => ("tarvos_os_path_isfile", Type::Bool),
        ("os.path", "isdir") => ("tarvos_os_path_isdir", Type::Bool),
        ("json", "dumps") => ("tarvos_json_dumps_static", Type::String),
        // statistics. `mean`/`fmean` and the dispersion functions always produce
        // a fresh float. The element-returning functions (`median`, `median_low`,
        // `median_high`, `mode`, `multimode`) yield values taken from the input, so
        // they are registered as Unknown: CPython returns the int 2 for
        // `mode([1, 2, 2, 3])`, not 2.0, and the generated Rust must not coerce.
        // `mean` reduces through Fraction and yields an int when integer input
        // divides evenly, so it is Unknown rather than Float: `mean([10,20,30,40,50])`
        // is 30. `fmean` is always a float. The element-returning functions
        // (`median`, `median_low`, `median_high`, `mode`, `multimode`) likewise
        // yield values taken from the input and must not be coerced.
        ("statistics", "mean") => ("tarvos_statistics_mean", Type::Unknown),
        ("statistics", "fmean") => ("tarvos_statistics_fmean", Type::Float),
        ("statistics", "geometric_mean") => ("tarvos_statistics_geometric_mean", Type::Float),
        ("statistics", "harmonic_mean") => ("tarvos_statistics_harmonic_mean", Type::Float),
        ("statistics", "median") => ("tarvos_statistics_median", Type::Unknown),
        ("statistics", "median_low") => ("tarvos_statistics_median_low", Type::Unknown),
        ("statistics", "median_high") => ("tarvos_statistics_median_high", Type::Unknown),
        ("statistics", "mode") => ("tarvos_statistics_mode", Type::Unknown),
        ("statistics", "multimode") => (
            "tarvos_statistics_multimode",
            Type::Array(Box::new(Type::Unknown)),
        ),
        ("statistics", "variance") => ("tarvos_statistics_variance", Type::Float),
        ("statistics", "pvariance") => ("tarvos_statistics_pvariance", Type::Float),
        ("statistics", "stdev") => ("tarvos_statistics_stdev", Type::Float),
        ("statistics", "pstdev") => ("tarvos_statistics_pstdev", Type::Float),
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

/// Receiver type a builtin method is defined on.
///
/// The native backend is statically typed, so a method can only be lowered when
/// the receiver's type is already known to match the Python class it comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MethodReceiver {
    Str,
    List,
    Dict,
}

/// A `str`/`list`/`dict` builtin method lowered to a native Rust helper.
pub struct BuiltinMethod {
    /// Name of the generated runtime helper.
    pub rust_name: &'static str,
    /// Accepted Python argument counts, excluding the receiver.
    ///
    /// Most methods take a fixed number of arguments, but `split()` is valid
    /// with none (whitespace mode) or one (explicit separator).
    pub arity: std::ops::RangeInclusive<usize>,
    /// Statically known return type, or `Type::Unknown` when it depends on the
    /// receiver's element or value type.
    pub return_type: Type,
}

/// A builtin taking exactly one arity.
fn exact(rust_name: &'static str, arity: usize, return_type: Type) -> BuiltinMethod {
    BuiltinMethod {
        rust_name,
        arity: arity..=arity,
        return_type,
    }
}

/// A builtin returning a list of strings.
fn strings(rust_name: &'static str, arity: usize) -> BuiltinMethod {
    exact(rust_name, arity, Type::Array(Box::new(Type::String)))
}

/// Map a Python builtin method onto its native Rust implementation.
///
/// Arity is recorded rather than assumed so a wrong call is reported by the
/// lowering stage with a clear message, instead of generating Rust that fails
/// to compile.
pub fn native_builtin_method(receiver: MethodReceiver, method: &str) -> Option<BuiltinMethod> {
    Some(match (receiver, method) {
        (MethodReceiver::Str, "lower") => exact("tarvos_str_lower", 0, Type::String),
        (MethodReceiver::Str, "upper") => exact("tarvos_str_upper", 0, Type::String),
        (MethodReceiver::Str, "title") => exact("tarvos_str_title", 0, Type::String),
        (MethodReceiver::Str, "capitalize") => exact("tarvos_str_capitalize", 0, Type::String),
        (MethodReceiver::Str, "swapcase") => exact("tarvos_str_swapcase", 0, Type::String),
        (MethodReceiver::Str, "strip") => exact("tarvos_str_strip", 0, Type::String),
        (MethodReceiver::Str, "lstrip") => exact("tarvos_str_lstrip", 0, Type::String),
        (MethodReceiver::Str, "rstrip") => exact("tarvos_str_rstrip", 0, Type::String),
        (MethodReceiver::Str, "ljust") => exact("tarvos_str_ljust", 1, Type::String),
        (MethodReceiver::Str, "rjust") => exact("tarvos_str_rjust", 1, Type::String),
        (MethodReceiver::Str, "zfill") => exact("tarvos_str_zfill", 1, Type::String),
        (MethodReceiver::Str, "center") => exact("tarvos_str_center", 1, Type::String),
        (MethodReceiver::Str, "replace") => exact("tarvos_str_replace", 2, Type::String),
        (MethodReceiver::Str, "join") => exact("tarvos_str_join", 1, Type::String),
        // `split()` with no argument splits on runs of whitespace, which drops
        // empty fields — different from `split(" ")`, so the helper takes an
        // `Option` rather than defaulting to a space.
        (MethodReceiver::Str, "split") => BuiltinMethod {
            rust_name: "tarvos_str_split",
            arity: 0..=1,
            return_type: Type::Array(Box::new(Type::String)),
        },
        (MethodReceiver::Str, "splitlines") => strings("tarvos_str_splitlines", 0),
        (MethodReceiver::Str, "startswith") => exact("tarvos_str_startswith", 1, Type::Bool),
        (MethodReceiver::Str, "endswith") => exact("tarvos_str_endswith", 1, Type::Bool),
        (MethodReceiver::Str, "isdigit") => exact("tarvos_str_isdigit", 0, Type::Bool),
        (MethodReceiver::Str, "isalpha") => exact("tarvos_str_isalpha", 0, Type::Bool),
        (MethodReceiver::Str, "isalnum") => exact("tarvos_str_isalnum", 0, Type::Bool),
        (MethodReceiver::Str, "isspace") => exact("tarvos_str_isspace", 0, Type::Bool),
        (MethodReceiver::Str, "isupper") => exact("tarvos_str_isupper", 0, Type::Bool),
        (MethodReceiver::Str, "islower") => exact("tarvos_str_islower", 0, Type::Bool),
        (MethodReceiver::Str, "count") => exact("tarvos_str_count", 1, Type::Int),
        (MethodReceiver::Str, "find") => exact("tarvos_str_find", 1, Type::Int),
        (MethodReceiver::Str, "rfind") => exact("tarvos_str_rfind", 1, Type::Int),
        (MethodReceiver::Str, "index") => exact("tarvos_str_index", 1, Type::Int),
        (MethodReceiver::List, "index") => exact("tarvos_list_index", 1, Type::Int),
        (MethodReceiver::List, "count") => exact("tarvos_list_count", 1, Type::Int),
        // `pop()` removes and returns the last item; `pop(i)` removes and
        // returns the item at index i, and i may be negative. Python raises
        // IndexError for an empty list or an out-of-range index, so both forms
        // are modelled and both report the same failure the interpreter does.
        (MethodReceiver::List, "pop") => BuiltinMethod {
            rust_name: "tarvos_list_pop",
            arity: 0..=1,
            return_type: Type::Unknown,
        },
        (MethodReceiver::List, "extend") => exact("tarvos_list_extend", 1, Type::None),
        (MethodReceiver::List, "insert") => exact("tarvos_list_insert", 2, Type::None),
        (MethodReceiver::List, "remove") => exact("tarvos_list_remove", 1, Type::None),
        (MethodReceiver::List, "clear") => exact("tarvos_list_clear", 0, Type::None),
        (MethodReceiver::List, "sort") => exact("tarvos_list_sort", 0, Type::None),
        (MethodReceiver::List, "reverse") => exact("tarvos_list_reverse", 0, Type::None),
        (MethodReceiver::Dict, "update") => exact("tarvos_dict_update", 1, Type::None),
        (MethodReceiver::Dict, "setdefault") => exact("tarvos_dict_setdefault", 2, Type::Unknown),
        (MethodReceiver::Dict, "get") => exact("tarvos_dict_get", 1, Type::Unknown),
        (MethodReceiver::Dict, "pop") => exact("tarvos_dict_pop", 1, Type::Unknown),
        (MethodReceiver::Dict, "keys") => strings("tarvos_dict_keys", 0),
        (MethodReceiver::Dict, "values") => exact(
            "tarvos_dict_values",
            0,
            Type::Array(Box::new(Type::Unknown)),
        ),
        (MethodReceiver::Dict, "items") => exact("tarvos_dict_items", 0, Type::Unknown),
        _ => return None,
    })
}

/// Whether a `list` builtin method mutates the receiver in place.
///
/// Mutating methods cannot be lowered to a pure `Value`, so they are routed
/// through the dedicated statement forms instead of a call expression.
pub fn list_method_mutates(method: &str) -> bool {
    matches!(
        method,
        "append" | "extend" | "insert" | "remove" | "clear" | "sort" | "reverse"
    )
}

/// Whether a `dict` builtin method mutates the receiver in place.
pub fn dict_method_mutates(method: &str) -> bool {
    matches!(method, "update" | "setdefault" | "pop" | "clear")
}
