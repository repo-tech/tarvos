/// Runtime for a name whose type is not fixed at compile time.
///
/// This file is included verbatim into a generated program, so it is Rust
/// source rather than a Rust literal and must stand on its own.
///
/// The parse helpers it calls (`__tarvos_parse_int`, `__tarvos_parse_float`)
/// live in the conversion runtime, which lowering emits alongside this block
/// whenever a dynamic value can reach them.
#[derive(Clone, Debug)]
pub enum __TarvosValue {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    None,
    List(Vec<__TarvosValue>),
    Tuple(Vec<__TarvosValue>),
    Dict(std::collections::HashMap<String, __TarvosValue>),
}

/// The operations the arithmetic dispatcher acts on.
///
/// Lowering emits one call per operation rather than a single generic entry
/// point: a runtime match inside a hot loop is measurably slower than letting
/// the optimiser see a fixed `Op` at the call site.
pub mod tarvos_binop {
    #[derive(Clone, Copy)]
    pub enum Op {
        Add,
        Sub,
        Mul,
        Div,
        Mod,
        Pow,
        FloorDiv,
    }
}

impl __TarvosValue {
    pub fn type_name(&self) -> &'static str {
        match self {
            __TarvosValue::Int(_) => "int",
            __TarvosValue::Float(_) => "float",
            __TarvosValue::Bool(_) => "bool",
            __TarvosValue::Str(_) => "str",
            __TarvosValue::None => "NoneType",
            __TarvosValue::List(_) => "list",
            __TarvosValue::Tuple(_) => "tuple",
            __TarvosValue::Dict(_) => "dict",
        }
    }

    /// Python truthiness: empty containers and zero/empty strings are false.
    pub fn truthy(&self) -> bool {
        match self {
            __TarvosValue::Int(i) => *i != 0,
            __TarvosValue::Float(v) => *v != 0.0,
            __TarvosValue::Bool(b) => *b,
            __TarvosValue::Str(s) => !s.is_empty(),
            __TarvosValue::None => false,
            __TarvosValue::List(items) | __TarvosValue::Tuple(items) => !items.is_empty(),
            __TarvosValue::Dict(entries) => !entries.is_empty(),
        }
    }

    pub fn as_float(&self) -> Option<f64> {
        match self {
            __TarvosValue::Int(i) => Some(*i as f64),
            __TarvosValue::Float(f) => Some(*f),
            _ => None,
        }
    }

    fn as_int(&self) -> Option<i64> {
        match self {
            __TarvosValue::Int(i) => Some(*i),
            __TarvosValue::Bool(b) => Some(*b as i64),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            __TarvosValue::Str(s) => Some(s),
            _ => None,
        }
    }

    /// Ordering. Python compares across types only where the types are
    /// numerically compatible; anything else is a `TypeError` rather than an
    /// arbitrary order, so this reports `None` instead of guessing one.
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        use std::cmp::Ordering;
        if let (Self::Str(a), Self::Str(b)) = (self, other) {
            return Some(a.cmp(b));
        }
        // Lists and tuples compare element by element, and only fall through to
        // the length check when every element so far was equal.
        let (a, b) = match (self, other) {
            (Self::List(a), Self::List(b)) | (Self::Tuple(a), Self::Tuple(b)) => (a, b),
            _ => {
                return match (self.as_float(), other.as_float()) {
                    (Some(a), Some(b)) => a.partial_cmp(&b),
                    _ => None,
                }
            }
        };
        for (x, y) in a.iter().zip(b.iter()) {
            match x.partial_cmp(y) {
                Some(Ordering::Equal) => continue,
                decided => return decided,
            }
        }
        Some(a.len().cmp(&b.len()))
    }

    /// Numeric view of a value, raising Python's `TypeError` when there is none.
    fn num_for(&self, other: &Self) -> f64 {
        self.as_float().unwrap_or_else(|| {
            panic!(
                "TypeError: unsupported operand type(s) for this operation: '{}' and '{}'",
                self.type_name(),
                other.type_name()
            )
        })
    }

    /// Arithmetic on two numbers, keeping `int` only when both sides are `int`.
    fn arith(
        &self,
        other: &Self,
        int_op: fn(i64, i64) -> i64,
        float_op: fn(f64, f64) -> f64,
    ) -> Self {
        if let (Self::Int(a), Self::Int(b)) = (self, other) {
            return Self::Int(int_op(*a, *b));
        }
        __TarvosValue::Float(float_op(self.num_for(other), other.num_for(self)))
    }
fn binop(&self, op: tarvos_binop::Op, other: &Self) -> Self {
        use tarvos_binop::Op;
        match op {
            // `+` concatenates sequences as well as adding numbers, and the
            // string case is the one Python programs hit most often.
            Op::Add => {
                // `if let` rather than a tuple match: the scrutinee is a pair
                // of references, and matching a tuple pattern against
                // `(&Self, &Self)` does not auto-deref each element, so the
                // tuple form would not compile at all.
                if let (Self::Str(a), Self::Str(b)) = (self, other) {
                    return Self::Str(format!("{}{}", a, b));
                }
                if let (Self::List(a), Self::List(b)) = (self, other) {
                    return Self::List(join(a, b));
                }
                if let (Self::Tuple(a), Self::Tuple(b)) = (self, other) {
                    return Self::Tuple(join(a, b));
                }
                self.arith(other, |a, b| a + b, |a, b| a + b)
            }
            Op::Sub => self.arith(other, |a, b| a - b, |a, b| a - b),
            Op::Mul => {
                // A sequence multiplied by an int repeats: `[0] * 3` is three
                // elements, not a numeric product.
                if let Some(count) = other.as_int() {
                    if let Self::List(items) = self {
                        return Self::List(repeat(items, count));
                    }
                    if let Self::Str(s) = self {
                        return Self::Str(s.repeat(count.max(0) as usize));
                    }
                }
                if let Some(count) = self.as_int() {
                    if let Self::List(items) = other {
                        return Self::List(repeat(items, count));
                    }
                    if let Self::Str(s) = other {
                        return Self::Str(s.repeat(count.max(0) as usize));
                    }
                }
                self.arith(other, |a, b| a * b, |a, b| a * b)
            }
            Op::Div => {
                // True division always yields a float, so `7 / 2` is 3.5 and
                // not the 3 that Rust integer division would produce.
                let a = self.num_for(other);
                let b = other.num_for(self);
                if b == 0.0 {
                    panic!("ZeroDivisionError: division by zero");
                }
                __TarvosValue::Float(a / b)
            }
            Op::FloorDiv => {
                let a = self.num_for(other);
                let b = other.num_for(self);
                if b == 0.0 {
                    panic!("ZeroDivisionError: division by zero");
                }
                __TarvosValue::Float((a / b).floor())
            }
            Op::Mod => {
                if let (Self::Str(a), Self::Str(b)) = (self, other) {
                    return Self::Str(format!("{}{}", a, b));
                }
                let a = self.num_for(other);
                let b = other.num_for(self);
                if b == 0.0 {
                    panic!("ZeroDivisionError: modulo by zero");
                }
                // Rust's `%` is a remainder and goes negative; Python's `%`
                // always lands in [0, divisor) for a positive divisor.
                __TarvosValue::Float(a - b * (a / b).floor())
            }
            Op::Pow => {
                let a = self.num_for(other);
                let b = other.num_for(self);
                __TarvosValue::Float(a.powf(b))
            }
        }
    }

    /// Read an element: `value[key]`.
    pub fn index(&self, key: &__TarvosValue) -> __TarvosValue {
        match self {
            __TarvosValue::List(items) | __TarvosValue::Tuple(items) => {
                items[self.resolve_index(key, items.len())].clone()
            }
            __TarvosValue::Str(text) => text
                .chars()
                .nth(self.resolve_index(key, text.chars().count()))
                .map(|c| __TarvosValue::Str(c.to_string()))
                .unwrap_or(__TarvosValue::None),
            __TarvosValue::Dict(entries) => {
                // A non-string key is rendered the way `print` would show it.
                // `Display` is implemented for the value rather than the
                // reference, so it is called on a deref.
                let key = match key {
                    __TarvosValue::Str(s) => s.clone(),
                    other => (*other).to_string(),
                };
                entries.get(&key).cloned().unwrap_or(__TarvosValue::None)
            }
            other => panic!("TypeError: '{}' object is not subscriptable", other.type_name()),
        }
    }

    /// Turn a possibly-negative Python index into a valid offset, raising
    /// `IndexError` rather than panicking on an out-of-range read.
    fn resolve_index(&self, key: &__TarvosValue, len: usize) -> usize {
        let raw = match key {
            __TarvosValue::Int(i) => *i,
            __TarvosValue::Bool(b) => *b as i64,
            other => panic!("TypeError: indices must be integers, not '{}'", other.type_name()),
        };
        let position = if raw < 0 { len as i64 + raw } else { raw };
        if position < 0 || position >= len as i64 {
            panic!("IndexError: index out of range");
        }
        position as usize
    }
}

fn join(left: &[__TarvosValue], right: &[__TarvosValue]) -> Vec<__TarvosValue> {
    let mut merged = left.to_vec();
    merged.extend_from_slice(right);
    merged
}

fn repeat(items: &[__TarvosValue], count: i64) -> Vec<__TarvosValue> {
    if count <= 0 {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(items.len() * count as usize);
    for _ in 0..count {
        out.extend_from_slice(items);
    }
    out
}

/// Python equality is value-based and cross-type: `1 == 1.0` and `1 == True`
/// are both true. A derived `PartialEq` compares variants first and would call
/// all three false.
impl PartialEq for __TarvosValue {
    fn eq(&self, other: &Self) -> bool {
        use __TarvosValue::*;
        match (self, other) {
            (Int(a), Int(b)) => a == b,
            (Float(a), Float(b)) => a == b,
            // `bool` is a subclass of `int`, so `True == 1` holds.
            (Bool(a), Int(b)) | (Int(b), Bool(a)) => (*a as i64) == *b,
            (Bool(a), Bool(b)) => a == b,
            (Str(a), Str(b)) => a == b,
            (None, None) => true,
            (List(a), List(b)) => a == b,
            (Tuple(a), Tuple(b)) => a == b,
            (Dict(a), Dict(b)) => a == b,
            // Numeric comparison is what makes `[1, 2] == [1.0, 2.0]` true.
            (Int(a), Float(b)) | (Float(b), Int(a)) => (*a as f64) == *b,
            _ => false,
        }
    }
}

impl std::fmt::Display for __TarvosValue {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            __TarvosValue::Str(s) => write!(f, "{}", s),
            __TarvosValue::Bool(b) => write!(f, "{}", if *b { "True" } else { "False" }),
            __TarvosValue::None => write!(f, "None"),
            __TarvosValue::Int(i) => write!(f, "{}", i),
            __TarvosValue::Float(v) => write!(f, "{}", v),
            __TarvosValue::List(items) => {
                let parts = items.iter().map(render_nested).collect::<Vec<_>>();
                write!(f, "[{}]", parts.join(", "))
            }
            __TarvosValue::Tuple(items) => {
                let parts = items.iter().map(render_nested).collect::<Vec<_>>();
                // A one-element tuple keeps its trailing comma; without it
                // `(1,)` would print as `(1)` and read as a plain int.
                if items.len() == 1 {
                    write!(f, "({},)", parts[0])
                } else {
                    write!(f, "({})", parts.join(", "))
                }
            }
            __TarvosValue::Dict(entries) => {
                let mut parts = entries
                    .iter()
                    .map(|(k, v)| {
                        format!(
                            "{}: {}",
                            render_nested(&__TarvosValue::Str(k.clone())),
                            render_nested(v)
                        )
                    })
                    .collect::<Vec<_>>();
                // HashMap iteration order is arbitrary, so sort to keep output
                // reproducible across runs.
                parts.sort();
                write!(f, "{{{}}}", parts.join(", "))
            }
        }
    }
}

/// Nested strings render quoted, the way `print([1, 'a'])` does in Python.
fn render_nested(value: &__TarvosValue) -> String {
    match value {
        __TarvosValue::Str(s) => format!("'{}'", s),
        other => (*other).to_string(),
    }
}

#[allow(dead_code)]
#[inline]
pub fn __tarvos_binop(a: &__TarvosValue, b: &__TarvosValue, op: tarvos_binop::Op) -> __TarvosValue {
    a.binop(op, b)
}

/// `op` is one of `<`, `>`, `<=`, `>=`. A `None` ordering means the types
/// cannot be ordered at all, which Python reports as a `TypeError` rather than
/// quietly answering false.
#[allow(dead_code)]
#[inline]
pub fn __tarvos_compare(a: &__TarvosValue, b: &__TarvosValue, op: char) -> bool {
    use std::cmp::Ordering;
    let ordering = match op {
        '<' => a.partial_cmp(b),
        '>' => b.partial_cmp(a),
        '=' => return a == b,
        _ => return false,
    };
    match ordering {
        Some(Ordering::Less) => op == '<',
        Some(Ordering::Greater) => op == '>',
        Some(Ordering::Equal) => matches!(op, '<' | '>'),
        None => panic!(
            "TypeError: '{}' not supported between instances of '{}' and '{}'",
            op,
            a.type_name(),
            b.type_name()
        ),
    }
}

#[allow(dead_code)]
#[inline]
pub fn __tarvos_eq(a: &__TarvosValue, b: &__TarvosValue) -> bool {
    a == b
}

#[allow(dead_code)]
#[inline]
pub fn __tarvos_truthy(value: &__TarvosValue) -> bool {
    value.truthy()
}

#[allow(dead_code)]
#[inline]
pub fn __tarvos_display(value: &__TarvosValue) -> String {
    value.to_string()
}

#[allow(dead_code)]
#[inline]
pub fn __tarvos_type_name(value: &__TarvosValue) -> &'static str {
    value.type_name()
}

/// Clone a tagged value.
///
/// `let x = y;` on a dynamic name reads `y` and stores a copy, because Rust
/// would otherwise move out of a binding that is still read later.
#[allow(dead_code)]
#[inline]
pub fn __tarvos_identity(value: &__TarvosValue) -> __TarvosValue {
    value.clone()
}

/// Build a tagged dictionary from key/value pairs.
#[allow(dead_code)]
#[inline]
pub fn __tarvos_map_of(pairs: &[(String, __TarvosValue)]) -> __TarvosValue {
    let mut map = std::collections::HashMap::with_capacity(pairs.len());
    for (key, value) in pairs {
        map.insert(key.clone(), value.clone());
    }
    __TarvosValue::Dict(map)
}

/// Arithmetic negation. `-x` on an `int` stays an `int`, on a `float` a float.
#[allow(dead_code)]
#[inline]
pub fn __tarvos_neg(value: __TarvosValue) -> __TarvosValue {
    match value {
        __TarvosValue::Int(i) => __TarvosValue::Int(-i),
        __TarvosValue::Float(f) => __TarvosValue::Float(-f),
        other => panic!(
            "TypeError: bad operand type for unary -: '{}'",
            other.type_name()
        ),
    }
}

#[allow(dead_code)]
#[inline]
pub fn __tarvos_to_int(value: &__TarvosValue) -> i64 {
    match value {
        __TarvosValue::Int(i) => *i,
        __TarvosValue::Bool(b) => *b as i64,
        __TarvosValue::Float(f) if f.fract() == 0.0 => *f as i64,
        __TarvosValue::Float(_) => panic!("ValueError: cannot convert float to integer"),
        __TarvosValue::Str(s) => __tarvos_parse_int(s),
        other => panic!(
            "TypeError: int() argument must be a string or a number, not '{}'",
            other.type_name()
        ),
    }
}

#[allow(dead_code)]
#[inline]
pub fn __tarvos_to_float(value: &__TarvosValue) -> f64 {
    match value {
        __TarvosValue::Int(i) => *i as f64,
        __TarvosValue::Float(f) => *f,
        __TarvosValue::Str(s) => __tarvos_parse_float(s),
        other => panic!(
            "TypeError: float() argument must be a string or a number, not '{}'",
            other.type_name()
        ),
    }
}

#[allow(dead_code)]
#[inline]
pub fn __tarvos_len(value: &__TarvosValue) -> i64 {
    match value {
        __TarvosValue::Str(s) => s.chars().count() as i64,
        __TarvosValue::List(items) | __TarvosValue::Tuple(items) => items.len() as i64,
        __TarvosValue::Dict(entries) => entries.len() as i64,
        other => panic!("TypeError: object of type '{}' has no len()", other.type_name()),
    }
}

#[allow(dead_code)]
#[inline]
pub fn __tarvos_append(target: &mut __TarvosValue, item: __TarvosValue) {
    match target {
        __TarvosValue::List(items) => items.push(item),
        other => panic!(
            "AttributeError: '{}' object has no attribute 'append'",
            other.type_name()
        ),
    }
}
