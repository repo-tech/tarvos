use crate::ast_bridge::{BinaryOperator, CompareOperator, Expr, Module, Stmt};
use std::collections::{HashMap, HashSet};

#[derive(Debug)]
pub struct CodegenError {
    pub message: String,
}
impl CodegenError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}
#[derive(Clone, Copy)]
enum NativeLibrary {
    Numpy,
    Polars,
}
pub fn generate(module: &Module) -> Result<String, CodegenError> {
    Emitter::new(module).module(module)
}

struct Emitter {
    output: String,
    aliases: HashMap<String, NativeLibrary>,
    async_functions: HashSet<String>,
    scopes: Vec<HashSet<String>>,
    indent: usize,
    temporary: usize,
}
impl Emitter {
    fn new(module: &Module) -> Self {
        let aliases = module
            .statements
            .iter()
            .filter_map(|stmt| match stmt {
                Stmt::Import { module, alias } if module == "numpy" => {
                    Some((alias.clone(), NativeLibrary::Numpy))
                }
                Stmt::Import { module, alias } if module == "pandas" => {
                    Some((alias.clone(), NativeLibrary::Polars))
                }
                _ => None,
            })
            .collect();
        let async_functions = module
            .statements
            .iter()
            .filter_map(|stmt| match stmt {
                Stmt::Function {
                    name,
                    is_async: true,
                    ..
                } => Some(name.clone()),
                _ => None,
            })
            .collect();
        Self {
            output: generated_runtime(),
            aliases,
            async_functions,
            scopes: vec![HashSet::new()],
            indent: 0,
            temporary: 0,
        }
    }
    fn module(mut self, module: &Module) -> Result<String, CodegenError> {
        for stmt in &module.statements {
            if matches!(stmt, Stmt::Function { .. }) {
                self.function(stmt)?;
                self.output.push('\n');
            }
        }
        self.line("fn main() {");
        self.indent += 1;
        self.line("let _ = block_on(async {");
        self.indent += 1;
        for stmt in &module.statements {
            if !matches!(stmt, Stmt::Function { .. } | Stmt::Import { .. }) {
                self.stmt(stmt)?;
            }
        }
        self.line("TarvosObject::None");
        self.indent -= 1;
        self.line("});");
        self.indent -= 1;
        self.line("}");
        Ok(self.output)
    }
    fn function(&mut self, stmt: &Stmt) -> Result<(), CodegenError> {
        let Stmt::Function {
            name,
            params,
            is_async,
            body,
        } = stmt
        else {
            return Err(CodegenError::new("internal function lowering error"));
        };
        let rust_params = params
            .iter()
            .map(|param| Ok(format!("{}: TarvosObject", ident(param)?)))
            .collect::<Result<Vec<_>, CodegenError>>()?;
        self.line(&format!(
            "{}fn {}({}) -> TarvosObject {{",
            if *is_async { "async " } else { "" },
            ident(name)?,
            rust_params.join(", ")
        ));
        self.indent += 1;
        self.scopes.push(
            rust_params
                .iter()
                .filter_map(|param| param.split(':').next())
                .map(str::to_owned)
                .collect(),
        );
        for child in body {
            self.stmt(child)?;
        }
        self.line("TarvosObject::None");
        self.scopes.pop();
        self.indent -= 1;
        self.line("}");
        Ok(())
    }
    fn stmt(&mut self, stmt: &Stmt) -> Result<(), CodegenError> {
        match stmt {
            Stmt::Import { .. } => Ok(()),
            Stmt::Function { .. } => Err(CodegenError::new("nested functions are unsupported")),
            Stmt::Assign { targets, value } => self.assign(targets, value),
            Stmt::Expr(Expr::Call { function, args }) if matches!(function.as_ref(), Expr::Name(name) if name == "print") =>
            {
                for arg in args {
                    let value = self.expr(arg)?;
                    self.line(&format!("println!(\"{{}}\", {value});"));
                }
                Ok(())
            }
            Stmt::Expr(value) => {
                let value = self.expr(value)?;
                self.line(&format!("let _ = {value};"));
                Ok(())
            }
            Stmt::If { test, body, orelse } => {
                let test = self.expr(test)?;
                self.line(&format!("if {test}.truthy() {{"));
                self.indent += 1;
                for child in body {
                    self.stmt(child)?;
                }
                self.indent -= 1;
                if orelse.is_empty() {
                    self.line("}");
                } else {
                    self.line("} else {");
                    self.indent += 1;
                    for child in orelse {
                        self.stmt(child)?;
                    }
                    self.indent -= 1;
                    self.line("}");
                }
                Ok(())
            }
            Stmt::While { test, body } => {
                let test = self.expr(test)?;
                self.line(&format!("while {test}.truthy() {{"));
                self.indent += 1;
                for child in body {
                    self.stmt(child)?;
                }
                self.indent -= 1;
                self.line("}");
                Ok(())
            }
            Stmt::For {
                target: Expr::Name(name),
                iter,
                body,
                is_async: false,
            } => {
                let name = ident(name)?;
                let iter = self.expr(iter)?;
                self.current_scope().insert(name.clone());
                self.line(&format!("for {name} in {iter}.iter() {{"));
                self.indent += 1;
                for child in body {
                    self.stmt(child)?;
                }
                self.indent -= 1;
                self.line("}");
                Ok(())
            }
            Stmt::For { is_async: true, .. } => Err(CodegenError::new(
                "async for needs an async iterator runtime",
            )),
            Stmt::For { .. } => Err(CodegenError::new("for target must be a variable name")),
            Stmt::Return(value) => {
                let value = match value {
                    Some(value) => self.expr(value)?,
                    None => "TarvosObject::None".to_owned(),
                };
                self.line(&format!("return {value};"));
                Ok(())
            }
            Stmt::Break => {
                self.line("break;");
                Ok(())
            }
            Stmt::Continue => {
                self.line("continue;");
                Ok(())
            }
            Stmt::Unsupported { kind } => {
                Err(CodegenError::new(format!("unsupported statement: {kind}")))
            }
        }
    }
    fn assign(&mut self, targets: &[Expr], value: &Expr) -> Result<(), CodegenError> {
        if targets.len() != 1 {
            return Err(CodegenError::new("chained assignment is not supported"));
        }
        match &targets[0] {
            Expr::Name(name) => self.assign_name(name, self.expr(value)?),
            Expr::Tuple(names) => {
                let mut rust_names = Vec::with_capacity(names.len());
                for name in names {
                    let Expr::Name(name) = name else {
                        return Err(CodegenError::new(
                            "tuple assignment requires variable names",
                        ));
                    };
                    rust_names.push(ident(name)?);
                }
                if let Expr::Tuple(values) = value {
                    if values.len() == rust_names.len() {
                        for (name, value) in rust_names.iter().zip(values) {
                            self.assign_rust_name(name, self.expr(value)?)?;
                        }
                        return Ok(());
                    }
                }
                let rhs = self.expr(value)?;
                let id = self.next_temp();
                self.line(&format!(
                    "let {id} = {rhs}.unpack({}).expect(\"tuple unpack failed\");",
                    rust_names.len()
                ));
                for (index, name) in rust_names.iter().enumerate() {
                    self.assign_rust_name(name, format!("{id}[{index}].clone()"))?;
                }
                Ok(())
            }
            _ => Err(CodegenError::new("assignment target is unsupported")),
        }
    }
    fn assign_name(&mut self, python_name: &str, value: String) -> Result<(), CodegenError> {
        self.assign_rust_name(&ident(python_name)?, value)
    }
    fn assign_rust_name(&mut self, name: &str, value: String) -> Result<(), CodegenError> {
        if self.declared(name) {
            self.line(&format!("{name} = {value};"));
        } else {
            self.current_scope().insert(name.to_owned());
            self.line(&format!("let mut {name} = {value};"));
        }
        Ok(())
    }
    fn expr(&self, expr: &Expr) -> Result<String, CodegenError> {
        match expr {
            Expr::None => Ok("TarvosObject::None".into()),
            Expr::Bool(value) => Ok(format!("TarvosObject::Bool({value})")),
            Expr::Int(value) => Ok(format!("TarvosObject::Int({value})")),
            Expr::Float(value) => Ok(format!("TarvosObject::Float({value:?})")),
            Expr::String(value) => Ok(format!("TarvosObject::Str({value:?}.into())")),
            Expr::Name(name) => Ok(format!("{}.clone()", ident(name)?)),
            Expr::List(values) => Ok(format!(
                "TarvosObject::List(vec![{}].into_boxed_slice())",
                self.exprs(values)?
            )),
            Expr::Tuple(values) => Ok(format!(
                "TarvosObject::Tuple(vec![{}].into_boxed_slice())",
                self.exprs(values)?
            )),
            Expr::ListRepeat { values, count } if values.len() == 1 => Ok(format!(
                "tarvos_runtime::repeat_one({}, {})",
                self.expr(&values[0])?,
                self.expr(count)?
            )),
            Expr::ListRepeat { values, count } => Ok(format!(
                "tarvos_runtime::repeat_list(TarvosObject::List(vec![{}].into_boxed_slice()), {})",
                self.exprs(values)?,
                self.expr(count)?
            )),
            Expr::Binary {
                left,
                operator,
                right,
            } => {
                let method = match operator {
                    BinaryOperator::Add => "add",
                    BinaryOperator::Sub => "sub",
                    BinaryOperator::Mul => "mul",
                    BinaryOperator::Div => "div",
                };
                Ok(format!(
                    "({}).{}({})",
                    self.expr(left)?,
                    method,
                    self.expr(right)?
                ))
            }
            Expr::Compare {
                left,
                operator,
                right,
            } => {
                let method = match operator {
                    CompareOperator::Eq => "eq",
                    CompareOperator::NotEq => "ne",
                    CompareOperator::Lt => "lt",
                    CompareOperator::LtEq => "le",
                    CompareOperator::Gt => "gt",
                    CompareOperator::GtEq => "ge",
                };
                Ok(format!(
                    "({}).{}({})",
                    self.expr(left)?,
                    method,
                    self.expr(right)?
                ))
            }
            Expr::Call { function, args } => self.call(function, args),
            Expr::Await(value) => {
                let Expr::Call { function, args } = value.as_ref() else {
                    return Err(CodegenError::new("await requires a call"));
                };
                let Expr::Name(name) = function.as_ref() else {
                    return Err(CodegenError::new("await requires a named function"));
                };
                if !self.async_functions.contains(name) {
                    return Err(CodegenError::new(format!(
                        "await targets non-async function {name}"
                    )));
                }
                Ok(format!("{}.await", self.call(function, args)?))
            }
            Expr::Attribute { .. } => Err(CodegenError::new(
                "arbitrary attribute access needs object-model lowering",
            )),
            Expr::Unsupported { kind } => {
                Err(CodegenError::new(format!("unsupported expression: {kind}")))
            }
        }
    }
    fn exprs(&self, values: &[Expr]) -> Result<String, CodegenError> {
        Ok(values
            .iter()
            .map(|value| self.expr(value))
            .collect::<Result<Vec<_>, _>>()?
            .join(", "))
    }
    fn call(&self, function: &Expr, args: &[Expr]) -> Result<String, CodegenError> {
        if let Expr::Attribute { object, attribute } = function {
            if let Expr::Name(alias) = object.as_ref() {
                if let Some(library) = self.aliases.get(alias) {
                    return self.native(*library, attribute, args);
                }
            }
        }
        if let Expr::Name(name) = function {
            if name == "range" {
                return Ok(format!(
                    "tarvos_runtime::range(vec![{}])",
                    self.exprs(args)?
                ));
            }
            if name == "print" {
                return Err(CodegenError::new("print must be an expression statement"));
            }
            return Ok(format!("{}({})", ident(name)?, self.exprs(args)?));
        }
        Err(CodegenError::new("call target is unsupported"))
    }
    fn native(
        &self,
        library: NativeLibrary,
        method: &str,
        args: &[Expr],
    ) -> Result<String, CodegenError> {
        let args = self.exprs(args)?;
        match (library, method) {
            (NativeLibrary::Numpy, "arange") => Ok(format!("tarvos_native::numpy::arange({args})")),
            (NativeLibrary::Numpy, "sum") => Ok(format!("tarvos_native::numpy::sum({args})")),
            (NativeLibrary::Numpy, "dot") => Ok(format!("tarvos_native::numpy::dot({args})")),
            (NativeLibrary::Polars, "DataFrame") => {
                Ok(format!("tarvos_native::polars::data_frame({args})"))
            }
            _ => Err(CodegenError::new(format!(
                "unsupported native capability: {method}"
            ))),
        }
    }
    fn current_scope(&mut self) -> &mut HashSet<String> {
        self.scopes.last_mut().expect("scope exists")
    }
    fn declared(&self, name: &str) -> bool {
        self.scopes.iter().rev().any(|scope| scope.contains(name))
    }
    fn next_temp(&mut self) -> String {
        let value = format!("__tarvos_unpack_{}", self.temporary);
        self.temporary += 1;
        value
    }
    fn line(&mut self, value: &str) {
        self.output.push_str(&"    ".repeat(self.indent));
        self.output.push_str(value);
        self.output.push('\n');
    }
}
fn ident(name: &str) -> Result<String, CodegenError> {
    if name
        .chars()
        .all(|value| value.is_ascii_alphanumeric() || value == '_')
    {
        Ok(format!("py_{name}"))
    } else {
        Err(CodegenError::new(format!("unsupported identifier: {name}")))
    }
}

fn generated_runtime() -> String {
    r##"#![allow(dead_code, unused_mut, unused_variables, unreachable_code)]
use std::fmt; use std::future::Future; use std::pin::Pin; use std::task::{Context,Poll,RawWaker,RawWakerVTable,Waker}

#[derive(Clone,Debug)] enum TarvosObject { None,Bool(bool),Int(i64),Float(f64),Str(Box<str>),List(Box<[TarvosObject]>),Tuple(Box<[TarvosObject]>),Unsupported(Box<str>) }
impl TarvosObject { fn truthy(&self)->bool{match self{Self::None|Self::Unsupported(_)=>false,Self::Bool(v)=>*v,Self::Int(v)=>*v!=0,Self::Float(v)=>*v!=0.0,Self::Str(v)=>!v.is_empty(),Self::List(v)|Self::Tuple(v)=>!v.is_empty()}} fn iter(&self)->Vec<Self>{match self{Self::List(v)|Self::Tuple(v)=>v.to_vec(),_=>Vec::new()}} fn unpack(&self,n:usize)->Result<Vec<Self>,&'static str>{match self{Self::List(v)|Self::Tuple(v)if v.len()==n=>Ok(v.to_vec()),Self::List(_)|Self::Tuple(_)=>Err("unpack length mismatch"),_=>Err("not unpackable")}} fn add(self,r:Self)->Self{match(self,r){(Self::Int(a),Self::Int(b))=>Self::Int(a.saturating_add(b)),(Self::Int(a),Self::Float(b))=>Self::Float(a as f64+b),(Self::Float(a),Self::Int(b))=>Self::Float(a+b as f64),(Self::Float(a),Self::Float(b))=>Self::Float(a+b),(Self::Str(a),Self::Str(b))=>Self::Str(format!("{a}{b}").into()),(a,b)=>Self::Unsupported(format!("add {a:?} {b:?}").into())}} fn sub(self,r:Self)->Self{match(self,r){(Self::Int(a),Self::Int(b))=>Self::Int(a.saturating_sub(b)),(a,b)=>num(a,b,|a,b|a-b,"sub")}} fn mul(self,r:Self)->Self{match(self,r){(Self::Int(a),Self::Int(b))=>Self::Int(a.saturating_mul(b)),(a,b)=>num(a,b,|a,b|a*b,"mul")}} fn div(self,r:Self)->Self{match r{Self::Int(0)|Self::Float(0.0)=>Self::Unsupported("division by zero".into()),r=>num(self,r,|a,b|a/b,"div")}} fn eq(self,r:Self)->Self{TarvosObject::Bool(match(&self,&r){(TarvosObject::None,TarvosObject::None)=>true,(TarvosObject::Bool(a),TarvosObject::Bool(b))=>a==b,(TarvosObject::Int(a),TarvosObject::Int(b))=>a==b,(TarvosObject::Int(a),TarvosObject::Float(b))=>(*a as f64)==*b,(TarvosObject::Float(a),TarvosObject::Int(b))=>*a==(*b as f64),(TarvosObject::Float(a),TarvosObject::Float(b))=>a==b,(TarvosObject::Str(a),TarvosObject::Str(b))=>a==b,_=>false})} fn ne(self,r:Self)->Self{let TarvosObject::Bool(v)=self.eq(r)else{unreachable!()};TarvosObject::Bool(!v)} fn cmp(self,r:Self,f:fn(f64,f64)->bool)->Self{match(self,r){(Self::Int(a),Self::Int(b))=>Self::Bool(f(a as f64,b as f64)),(Self::Int(a),Self::Float(b))=>Self::Bool(f(a as f64,b)),(Self::Float(a),Self::Int(b))=>Self::Bool(f(a,b as f64)),(Self::Float(a),Self::Float(b))=>Self::Bool(f(a,b)),_=>Self::Unsupported("comparison requires numeric operands".into())}} fn lt(self,r:Self)->Self{self.cmp(r,|a,b|a<b)}fn le(self,r:Self)->Self{self.cmp(r,|a,b|a<=b)}fn gt(self,r:Self)->Self{self.cmp(r,|a,b|a>b)}fn ge(self,r:Self)->Self{self.cmp(r,|a,b|a>=b)} }
fn num(a:TarvosObject,b:TarvosObject,f:fn(f64,f64)->f64,n:&str)->TarvosObject{match(a,b){(TarvosObject::Int(a),TarvosObject::Float(b))=>TarvosObject::Float(f(a as f64,b)),(TarvosObject::Float(a),TarvosObject::Int(b))=>TarvosObject::Float(f(a,b as f64)),(TarvosObject::Float(a),TarvosObject::Float(b))=>TarvosObject::Float(f(a,b)),(a,b)=>TarvosObject::Unsupported(format!("{n} {a:?} {b:?}").into())}}
impl fmt::Display for TarvosObject{fn fmt(&self,f:&mut fmt::Formatter<'_>)->fmt::Result{match self{Self::None=>f.write_str("None"),Self::Bool(v)=>write!(f,"{v}"),Self::Int(v)=>write!(f,"{v}"),Self::Float(v)=>write!(f,"{v}"),Self::Str(v)=>f.write_str(v),Self::List(v)|Self::Tuple(v)=>write!(f,"{v:?}"),Self::Unsupported(v)=>write!(f,"<unsupported: {v}>")}}}
mod tarvos_runtime{use super::TarvosObject;pub fn repeat_one(v:TarvosObject,n:TarvosObject)->TarvosObject{match n{TarvosObject::Int(n)if n>=0=>TarvosObject::List(vec![v;n as usize].into_boxed_slice()),_=>TarvosObject::Unsupported("list multiplier must be non-negative int".into())}}pub fn repeat_list(v:TarvosObject,n:TarvosObject)->TarvosObject{let(TarvosObject::List(v),TarvosObject::Int(n))=(v,n)else{return TarvosObject::Unsupported("list multiplier must be int".into())};if n<0{return TarvosObject::List(Box::default())};let mut out=Vec::with_capacity(v.len()*n as usize);for _ in 0..n{out.extend_from_slice(&v)}TarvosObject::List(out.into_boxed_slice())}pub fn range(a:Vec<TarvosObject>)->TarvosObject{let ints:Vec<i64>=a.into_iter().map(|v|if let TarvosObject::Int(v)=v{Some(v)}else{None}).collect::<Option<_>>().unwrap_or_default();let(start,stop,step)=match ints.as_slice(){[stop]=>(0,*stop,1),[start,stop]=>(*start,*stop,1),[start,stop,step]if *step!=0=>(*start,*stop,*step),_=>return TarvosObject::Unsupported("range arguments must be integers".into())};TarvosObject::List((if step>0{(start..stop).step_by(step as usize).collect()}else{let mut v=Vec::new();let mut i=start;while i>stop{v.push(i);i+=step}v}).into_iter().map(TarvosObject::Int).collect::<Vec<_>>().into_boxed_slice())}}
mod tarvos_native{use super::TarvosObject;pub mod numpy{use super::TarvosObject;pub fn arange(stop:TarvosObject)->TarvosObject{super::super::tarvos_runtime::range(vec![stop])}pub fn sum(v:TarvosObject)->TarvosObject{let TarvosObject::List(v)=v else{return TarvosObject::Unsupported("numpy.sum requires list".into())};v.into_vec().into_iter().fold(TarvosObject::Int(0),TarvosObject::add)}pub fn dot(a:TarvosObject,b:TarvosObject)->TarvosObject{let(TarvosObject::List(a),TarvosObject::List(b))=(a,b)else{return TarvosObject::Unsupported("numpy.dot requires lists".into())};if a.len()!=b.len(){return TarvosObject::Unsupported("numpy.dot shape mismatch".into())};a.into_vec().into_iter().zip(b.into_vec()).fold(TarvosObject::Int(0),|s,(a,b)|s.add(a.mul(b)))}}pub mod polars{use super::TarvosObject;pub fn data_frame(rows:TarvosObject)->TarvosObject{rows}}}
fn block_on<F:Future>(future:F)->F::Output{unsafe fn clone(_: *const())->RawWaker{raw()}unsafe fn noop(_: *const()){}static V:RawWakerVTable=RawWakerVTable::new(clone,noop,noop,noop);fn raw()->RawWaker{RawWaker::new(std::ptr::null(),&V)}let w=unsafe{Waker::from_raw(raw())};let mut c=Context::from_waker(&w);let mut f=Box::pin(future);loop{match Pin::as_mut(&mut f).poll(&mut c){Poll::Ready(v)=>return v,Poll::Pending=>std::thread::yield_now()}}}
"##.to_owned()
}

#[cfg(test)]
mod tests {
    use super::generate;
    use crate::ast_bridge::parse_python;

    #[test]
    fn generates_tuple_unpacking_list_repeat_and_range_runtime_calls() {
        let module = parse_python(
            "values = [7] * 3\na, b = values\nfor item in range(5):\n    print(item)\n",
        )
        .expect("source should parse");
        let rust = generate(&module).expect("supported source should generate");

        assert!(rust.contains("unpack(2)"));
        assert!(rust.contains("repeat_list"));
        assert!(rust.contains("tarvos_runtime::range"));
    }
}
