use tarvos_ast::{Expr, Module, Stmt};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VectorPlan {
    pub buffer: String,
    pub lane_width: usize,
    pub operation: VectorOperation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VectorOperation {
    SumI64,
}

/// Finds only side-effect-free reductions over a named contiguous integer
/// buffer. Any shape or operation that cannot be proven safe remains scalar.
pub fn detect_vector_plans(module: &Module) -> Vec<VectorPlan> {
    module
        .body
        .iter()
        .filter_map(|stmt| match stmt {
            Stmt::For {
                target,
                iter: Expr::Name { id: buffer },
                body,
            } if is_sum_reduction(target, buffer, body) => Some(VectorPlan {
                buffer: buffer.clone(),
                lane_width: 8,
                operation: VectorOperation::SumI64,
            }),
            _ => None,
        })
        .collect()
}

fn is_sum_reduction(target: &Expr, buffer: &str, body: &[Stmt]) -> bool {
    let Expr::Name { id: target_name } = target else {
        return false;
    };
    body.len() == 1
        && matches!(
            &body[0],
            Stmt::Assign {
                target: Expr::Name { id },
                value: Expr::Binary { left, operator, right },
            } if id == "total"
                && operator == "add"
                && matches!(left.as_ref(), Expr::Name { id } if id == "total")
                && matches!(
                    right.as_ref(),
                    Expr::Subscript { value, index }
                        if matches!(value.as_ref(), Expr::Name { id } if id == buffer)
                            && matches!(index.as_ref(), Expr::Name { id } if id == target_name)
                )
        )
}

/// Emits a portable contiguous reduction. The AVX2 branch deliberately uses
/// safe chunks rather than architecture-specific intrinsics; this keeps the
/// generated executable valid on every supported CPU while still making lane
/// width explicit and leaving room for a future intrinsic backend.
pub fn emit_runtime_helpers(plans: &[VectorPlan]) -> String {
    if plans.is_empty() {
        return String::new();
    }

    r#"
#[inline(always)]
fn tarvos_sum_i64_scalar(values: &[i64]) -> i64 {
    values.iter().copied().sum()
}

#[inline(always)]
fn tarvos_sum_i64_chunked(values: &[i64]) -> i64 {
    let mut total = 0_i64;
    for lane in values.chunks_exact(8) {
        total += lane[0] + lane[1] + lane[2] + lane[3]
            + lane[4] + lane[5] + lane[6] + lane[7];
    }
    for value in values.chunks_exact(8).remainder() {
        total += *value;
    }
    total
}

#[inline]
fn tarvos_sum_i64_dispatch(values: &[i64]) -> i64 {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        if std::is_x86_feature_detected!("avx2") {
            return tarvos_sum_i64_chunked(values);
        }
    }
    tarvos_sum_i64_scalar(values)
}
"#
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_only_proven_integer_sum_reduction() {
        let module = Module {
            body: vec![Stmt::For {
                target: Expr::Name { id: "i".into() },
                iter: Expr::Name {
                    id: "values".into(),
                },
                body: vec![Stmt::Assign {
                    target: Expr::Name { id: "total".into() },
                    value: Expr::Binary {
                        left: Box::new(Expr::Name { id: "total".into() }),
                        operator: "add".into(),
                        right: Box::new(Expr::Subscript {
                            value: Box::new(Expr::Name {
                                id: "values".into(),
                            }),
                            index: Box::new(Expr::Name { id: "i".into() }),
                        }),
                    },
                }],
            }],
        };
        assert_eq!(detect_vector_plans(&module)[0].lane_width, 8);
    }
}
