use std::fmt;
use std::ops::{Add, Div, Mul, Sub};

/// Dynamic fallback used only when static specialization is not available.
#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub enum TarvosObject {
    None,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(Box<str>),
    List(Box<[TarvosObject]>),
    Tuple(Box<[TarvosObject]>),
    Unsupported(Box<str>),
}

#[allow(dead_code)]
pub trait TarvosMath: Sized {
    fn add(self, rhs: Self) -> Self;
    fn sub(self, rhs: Self) -> Self;
    fn mul(self, rhs: Self) -> Self;
    fn div(self, rhs: Self) -> Self;
}

#[allow(dead_code)]
impl TarvosObject {
    pub fn truthy(&self) -> bool {
        match self {
            Self::None | Self::Unsupported(_) => false,
            Self::Bool(value) => *value,
            Self::Int(value) => *value != 0,
            Self::Float(value) => *value != 0.0,
            Self::Str(value) => !value.is_empty(),
            Self::List(value) | Self::Tuple(value) => !value.is_empty(),
        }
    }

    pub fn iter(&self) -> Vec<Self> {
        match self {
            Self::List(values) | Self::Tuple(values) => values.to_vec(),
            _ => Vec::new(),
        }
    }

    pub fn unpack(&self, expected: usize) -> Result<Vec<Self>, &'static str> {
        let values = match self {
            Self::List(values) | Self::Tuple(values) => values,
            _ => return Err("value is not unpackable"),
        };
        if values.len() == expected {
            Ok(values.to_vec())
        } else {
            Err("unpack length mismatch")
        }
    }
}

impl TarvosMath for TarvosObject {
    fn add(self, rhs: Self) -> Self {
        match (self, rhs) {
            (Self::Int(a), Self::Int(b)) => Self::Int(a.saturating_add(b)),
            (Self::Int(a), Self::Float(b)) => Self::Float(a as f64 + b),
            (Self::Float(a), Self::Int(b)) => Self::Float(a + b as f64),
            (Self::Float(a), Self::Float(b)) => Self::Float(a + b),
            (Self::Str(a), Self::Str(b)) => Self::Str(format!("{a}{b}").into()),
            (left, right) => Self::Unsupported(format!("cannot add {left:?} and {right:?}").into()),
        }
    }
    fn sub(self, rhs: Self) -> Self {
        numeric_binary(self, rhs, |a, b| a - b, |a, b| a - b, "subtract")
    }
    fn mul(self, rhs: Self) -> Self {
        numeric_binary(self, rhs, |a, b| a * b, |a, b| a * b, "multiply")
    }
    fn div(self, rhs: Self) -> Self {
        match rhs {
            Self::Int(0) | Self::Float(0.0) => Self::Unsupported("division by zero".into()),
            rhs => numeric_binary(self, rhs, |a, b| a / b, |a, b| a / b, "divide"),
        }
    }
}

impl Add for TarvosObject {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        TarvosMath::add(self, rhs)
    }
}

impl Sub for TarvosObject {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        TarvosMath::sub(self, rhs)
    }
}

impl Mul for TarvosObject {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self::Output {
        TarvosMath::mul(self, rhs)
    }
}

impl Div for TarvosObject {
    type Output = Self;

    fn div(self, rhs: Self) -> Self::Output {
        TarvosMath::div(self, rhs)
    }
}

#[allow(dead_code)]
fn numeric_binary(
    in_left: TarvosObject,
    in_right: TarvosObject,
    integer: impl Fn(i64, i64) -> i64,
    float: impl Fn(f64, f64) -> f64,
    name: &str,
) -> TarvosObject {
    match (in_left, in_right) {
        (TarvosObject::Int(a), TarvosObject::Int(b)) => TarvosObject::Int(integer(a, b)),
        (TarvosObject::Int(a), TarvosObject::Float(b)) => TarvosObject::Float(float(a as f64, b)),
        (TarvosObject::Float(a), TarvosObject::Int(b)) => TarvosObject::Float(float(a, b as f64)),
        (TarvosObject::Float(a), TarvosObject::Float(b)) => TarvosObject::Float(float(a, b)),
        (left, right) => {
            TarvosObject::Unsupported(format!("cannot {name} {left:?} and {right:?}").into())
        }
    }
}

impl fmt::Display for TarvosObject {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::None => formatter.write_str("None"),
            Self::Bool(v) => write!(formatter, "{v}"),
            Self::Int(v) => write!(formatter, "{v}"),
            Self::Float(v) => write!(formatter, "{v}"),
            Self::Str(v) => formatter.write_str(v),
            Self::List(v) | Self::Tuple(v) => write!(formatter, "{v:?}"),
            Self::Unsupported(v) => write!(formatter, "<unsupported: {v}>"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::TarvosObject;

    #[test]
    fn operator_traits_preserve_dynamic_numeric_and_string_values() {
        assert_eq!(
            TarvosObject::Int(2) + TarvosObject::Float(0.5),
            TarvosObject::Float(2.5)
        );
        assert_eq!(
            TarvosObject::Str("tar".into()) + TarvosObject::Str("vos".into()),
            TarvosObject::Str("tarvos".into())
        );
        assert!(matches!(
            TarvosObject::Bool(true) * TarvosObject::Int(2),
            TarvosObject::Unsupported(_)
        ));
    }

    #[test]
    fn list_and_tuple_values_support_truthiness_and_unpacking() {
        let values = TarvosObject::Tuple(
            vec![TarvosObject::Int(1), TarvosObject::Int(2)].into_boxed_slice(),
        );
        assert!(values.truthy());
        assert_eq!(
            values.unpack(2).expect("tuple should unpack"),
            vec![TarvosObject::Int(1), TarvosObject::Int(2)]
        );
        assert!(values.unpack(1).is_err());
    }
}
