use anyhow::Result;
use std::collections::HashMap;
use tarvos_ir::{Module, Stmt, Value};

#[derive(Default)]
pub struct TypeSpecializer {
    value_types: HashMap<String, String>,
}

impl TypeSpecializer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn specialize(module: &Module) -> Result<Module> {
        let mut spec = TypeSpecializer::new();
        let statements = module
            .statements
            .iter()
            .map(|s| spec.specialize_stmt(s))
            .collect::<Result<Vec<_>>>()?;
        Ok(Module { statements })
    }

    fn specialize_stmt(&mut self, stmt: &Stmt) -> Result<Stmt> {
        match stmt {
            Stmt::Let { name, ty, value } => {
                self.value_types.insert(name.clone(), format!("{}", ty));
                Ok(Stmt::Let {
                    name: name.clone(),
                    ty: ty.clone(),
                    value: self.specialize_value(value)?,
                })
            }
            Stmt::Assign { name, value } => Ok(Stmt::Assign {
                name: name.clone(),
                value: self.specialize_value(value)?,
            }),
            Stmt::IndexAssign {
                target,
                index,
                value,
            } => Ok(Stmt::IndexAssign {
                target: target.clone(),
                index: self.specialize_value(index)?,
                value: self.specialize_value(value)?,
            }),
            Stmt::Print(vs) => {
                let values = vs
                    .iter()
                    .map(|v| self.specialize_value(v))
                    .collect::<Result<Vec<_>>>()?;
                Ok(Stmt::Print(values))
            }
            Stmt::If { test, body, orelse } => {
                let test = self.specialize_value(test)?;
                let body = body
                    .iter()
                    .map(|s| self.specialize_stmt(s))
                    .collect::<Result<_>>()?;
                let orelse = orelse
                    .iter()
                    .map(|s| self.specialize_stmt(s))
                    .collect::<Result<_>>()?;
                Ok(Stmt::If { test, body, orelse })
            }
            Stmt::While { test, body } => {
                let test = self.specialize_value(test)?;
                let body = body
                    .iter()
                    .map(|s| self.specialize_stmt(s))
                    .collect::<Result<_>>()?;
                Ok(Stmt::While { test, body })
            }
            s => Ok(s.clone()),
        }
    }

    fn specialize_value(&self, value: &Value) -> Result<Value> {
        Ok(value.clone())
    }
}
