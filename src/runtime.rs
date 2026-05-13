use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::ast::{AssignOp, AssignTarget, BinaryOp, Expr, Stmt, StmtKind, TypeExpr};
use crate::builtins;
use crate::env::{EnvRef, Environment};
use crate::error::BoryError;
use crate::lexer::tokenize;
use crate::parser::parse;
use crate::span::Span;
use crate::value::{collect_heap_with_roots, heap_stats};
use crate::typecheck::check_program;
use crate::value::{TypeDef, UserFunction, Value, values_equal};
use crate::vm::{self, StatementOutcome};

pub fn check_source(source: &str, source_name: &str) -> Result<(), BoryError> {
    let statements = parse_source(source, source_name)?;
    check_program(&statements).map_err(|error| attach_source(error, source_name, source))
}

pub struct Interpreter {
    globals: EnvRef,
    env: EnvRef,
    module_stack: Vec<PathBuf>,
    module_cache: BTreeMap<PathBuf, Value>,
    rng: SimpleRng,
    gc_statement_counter: usize,
    gc_minor_interval: usize,
    gc_heap_threshold: usize,
}

enum Flow {
    Next(Value),
    Return(Value),
    Break,
    Continue,
}

impl Interpreter {
    pub fn new() -> Self {
        let globals = Environment::global();
        let mut interpreter = Self {
            globals: globals.clone(),
            env: globals,
            module_stack: Vec::new(),
            module_cache: BTreeMap::new(),
            rng: SimpleRng::from_time(),
            gc_statement_counter: 0,
            gc_minor_interval: 24,
            gc_heap_threshold: 128,
        };
        builtins::install(&mut interpreter);
        interpreter
    }

    pub fn run_source(&mut self, source: &str, source_name: &str) -> Result<Value, BoryError> {
        let statements = parse_source(source, source_name)?;
        check_program(&statements).map_err(|error| attach_source(error, source_name, source))?;
        self.execute_program(&statements)
            .map_err(|error| attach_source(error, source_name, source))
    }

    pub fn run_file(&mut self, path: &Path) -> Result<Value, BoryError> {
        let resolved = if path.exists() {
            normalize_path(path)
        } else {
            self.resolve_script_path(path.to_string_lossy().as_ref()).ok_or_else(|| {
                BoryError::io(format!("Could not resolve source file {}", path.display()))
            })?
        };
        let source = std::fs::read_to_string(&resolved).map_err(|error| {
            BoryError::io(format!("Could not read {}: {error}", resolved.display()))
        })?;

        self.module_stack.push(resolved.clone());
        let source_name = resolved.display().to_string();
        let result = self.run_source(&source, &source_name);
        self.module_stack.pop();
        result
    }

    pub fn define_global(&mut self, name: &str, value: Value) {
        Environment::define(&self.globals, name.to_string(), value);
    }

    pub fn get_global(&self, name: &str) -> Option<Value> {
        Environment::get(&self.globals, name)
    }

    pub fn current_base_dir(&self) -> PathBuf {
        self.module_stack
            .last()
            .and_then(|path| path.parent().map(Path::to_path_buf))
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."))
    }

    pub fn seed_random(&mut self, seed: u64) {
        self.rng.seed(seed);
    }

    pub fn next_random_u64(&mut self) -> u64 {
        self.rng.next_u64()
    }

    pub fn next_random_f64(&mut self) -> f64 {
        self.rng.next_f64()
    }

    pub fn call_public(
        &mut self,
        value: Value,
        args: Vec<Value>,
        span: Option<Span>,
    ) -> Result<Value, BoryError> {
        self.call_value(value, args, span)
    }

    pub(crate) fn read_variable(&self, name: &str, span: Span) -> Result<Value, BoryError> {
        Environment::get(&self.env, name).ok_or_else(|| {
            BoryError::runtime(format!("Variable '{name}' does not exist"), Some(span))
                .with_hint(format!("Declare it first with: var {name} = ..."))
        })
    }

    pub(crate) fn apply_binary_vm(
        &mut self,
        op: BinaryOp,
        left: Value,
        right: Value,
        span: Span,
    ) -> Result<Value, BoryError> {
        self.apply_binary(op, left, right, span)
    }

    pub(crate) fn read_index_vm(
        &self,
        object: Value,
        index: Value,
        span: Span,
    ) -> Result<Value, BoryError> {
        self.read_index(object, index, span)
    }

    pub(crate) fn read_member_vm(
        &self,
        object: Value,
        name: &str,
        span: Span,
    ) -> Result<Value, BoryError> {
        self.read_member(object, name, span)
    }

    pub(crate) fn define_local(
        &mut self,
        name: String,
        value: Value,
        type_hint: Option<TypeExpr>,
    ) {
        Environment::define_typed(&self.env, name, value, type_hint);
    }

    pub(crate) fn current_env(&self) -> EnvRef {
        self.env.clone()
    }

    pub(crate) fn import_module_public(&mut self, spec: &str, span: Span) -> Result<Value, BoryError> {
        self.import_module(spec, span)
    }

    pub(crate) fn load_path_public(&mut self, path_text: &str, span: Span) -> Result<Value, BoryError> {
        let resolved = self.resolve_script_path(path_text).ok_or_else(|| {
            BoryError::runtime(
                format!("Could not resolve source file '{path_text}'"),
                Some(span),
            )
            .with_hint("Use a relative path like \"lib/tools.boy\" or an absolute path")
        })?;
        self.run_file(&resolved)
            .map_err(|error| error.push_trace(format!("load {path_text}")))
    }

    pub(crate) fn iterable_items_public(&self, value: Value, span: Span) -> Result<Vec<Value>, BoryError> {
        self.iterable_items(value, span)
    }

    pub(crate) fn assign_or_define_public(&mut self, name: &str, value: Value) {
        self.assign_or_define(name, value);
    }

    pub(crate) fn assign_resolved_target(
        &mut self,
        target: &AssignTarget,
        op: AssignOp,
        value: Value,
        span: Span,
    ) -> Result<Value, BoryError> {
        match target {
            AssignTarget::Variable(name) => {
                let next_value = if op == AssignOp::Set {
                    value
                } else {
                    let current = Environment::get_binding(&self.env, name).ok_or_else(|| {
                        BoryError::runtime(format!("Variable '{name}' does not exist"), Some(span))
                            .with_hint(format!("Declare it first with: var {name} = ..."))
                    })?;
                    self.apply_assign_op(op, current.value, value, span)?
                };

                if let Some(binding) = Environment::get_binding(&self.env, name) {
                    if let Some(type_hint) = &binding.type_hint {
                        ensure_type_match(
                            &next_value,
                            type_hint,
                            span,
                            &format!("Variable '{name}'"),
                        )?;
                    }
                }

                if !Environment::assign(&self.env, name, next_value.clone()) {
                    return Err(BoryError::runtime(
                        format!("Variable '{name}' does not exist"),
                        Some(span),
                    )
                    .with_hint(format!("Declare it first with: var {name} = ...")));
                }
                Ok(next_value)
            }
            AssignTarget::Index { object, index } => {
                let object_value = self.eval(object)?;
                let index_value = self.eval(index)?;
                self.write_index(object_value, index_value, value, op, span)
            }
            AssignTarget::Member { object, name } => {
                let object_value = self.eval(object)?;
                self.write_member(object_value, name, value, op, span)
            }
        }
    }

    pub(crate) fn maybe_collect_garbage_public(&mut self) {
        self.maybe_collect_garbage();
    }

    pub fn resolve_path(&self, raw: &str) -> PathBuf {
        let path = PathBuf::from(raw);
        if path.is_absolute() {
            path
        } else {
            self.current_base_dir().join(path)
        }
    }

    pub(crate) fn heap_stats_public(&self) -> crate::value::HeapStats {
        heap_stats()
    }

    pub(crate) fn collect_garbage_minor(&mut self) -> crate::value::HeapStats {
        collect_heap_with_roots(&self.gc_roots(), false)
    }

    pub(crate) fn collect_garbage_major(&mut self) -> crate::value::HeapStats {
        collect_heap_with_roots(&self.gc_roots(), true)
    }

    fn execute_program(&mut self, statements: &[Stmt]) -> Result<Value, BoryError> {
        match vm::execute_program(self, statements)? {
            StatementOutcome::Next(value) => Ok(value),
            StatementOutcome::Return(_) => Err(BoryError::runtime(
                "Cannot use 'give' outside a task",
                statements.first().map(|stmt| stmt.span),
            )),
            StatementOutcome::Break => Err(BoryError::runtime(
                "Cannot use 'stop' outside a loop",
                statements.first().map(|stmt| stmt.span),
            )),
            StatementOutcome::Continue => Err(BoryError::runtime(
                "Cannot use 'skip' outside a loop",
                statements.first().map(|stmt| stmt.span),
            )),
        }
    }

    fn execute_statements(&mut self, statements: &[Stmt]) -> Result<Flow, BoryError> {
        let mut last_value = Value::Nil;
        for statement in statements {
            match self.execute_statement(statement)? {
                Flow::Next(value) => {
                    last_value = value;
                    self.maybe_collect_garbage();
                }
                other => return Ok(other),
            }
        }
        Ok(Flow::Next(last_value))
    }

    fn execute_statement(&mut self, statement: &Stmt) -> Result<Flow, BoryError> {
        match &statement.kind {
            StmtKind::Var {
                name,
                type_hint,
                initializer,
            } => {
                if initializer.is_none() && type_hint.is_some() {
                    return Err(BoryError::runtime(
                        format!("Typed variable '{name}' requires an initializer"),
                        Some(statement.span),
                    )
                    .with_hint(format!("Initialize it with: var {name}: ... = ...")));
                }
                let value = if let Some(initializer) = initializer {
                    self.eval(initializer)?
                } else {
                    Value::Nil
                };
                if let Some(type_hint) = type_hint {
                    ensure_type_match(
                        &value,
                        type_hint,
                        statement.span,
                        &format!("Variable '{name}'"),
                    )?;
                }
                Environment::define_typed(&self.env, name.clone(), value.clone(), type_hint.clone());
                Ok(Flow::Next(value))
            }
            StmtKind::Use { spec, alias } => {
                let module = self
                    .import_module(spec, statement.span)
                    .map_err(|error| error.push_trace(format!("use {spec} as {alias}")))?;
                Environment::define(&self.env, alias.clone(), module.clone());
                Ok(Flow::Next(module))
            }
            StmtKind::Assign { target, op, value } => {
                let assigned = self.assign_target(target, *op, value, statement.span)?;
                Ok(Flow::Next(assigned))
            }
            StmtKind::Expr(expr) => Ok(Flow::Next(self.eval(expr)?)),
            StmtKind::If {
                branches,
                else_branch,
            } => {
                for (condition, body) in branches {
                    if self.eval(condition)?.is_truthy() {
                        return self.execute_statements(body);
                    }
                }
                if let Some(body) = else_branch {
                    self.execute_statements(body)
                } else {
                    Ok(Flow::Next(Value::Nil))
                }
            }
            StmtKind::While { condition, body } => self.execute_while(condition, body),
            StmtKind::ForIn {
                name,
                iterable,
                body,
            } => self.execute_for_in(name, iterable, body, statement.span),
            StmtKind::ForRange {
                name,
                start,
                end,
                step,
                body,
            } => self.execute_for_range(name, start, end, step.as_ref(), body, statement.span),
            StmtKind::Task {
                name,
                params,
                return_type,
                body,
            } => {
                let function = Value::Function(Rc::new(UserFunction::new(
                    name.clone(),
                    params.clone(),
                    return_type.clone(),
                    body.clone(),
                    self.env.clone(),
                )));
                Environment::define(&self.env, name.clone(), function.clone());
                Ok(Flow::Next(function))
            }
            StmtKind::TypeDecl {
                flavor,
                name,
                fields,
                body,
            } => {
                let typedef = Value::Type(Rc::new(TypeDef::new(
                    *flavor,
                    name.clone(),
                    fields.clone(),
                    body.clone(),
                    self.env.clone(),
                )));
                Environment::define(&self.env, name.clone(), typedef.clone());
                Ok(Flow::Next(typedef))
            }
            StmtKind::Return(value) => {
                let result = if let Some(expr) = value {
                    self.eval(expr)?
                } else {
                    Value::Nil
                };
                Ok(Flow::Return(result))
            }
            StmtKind::Break => Ok(Flow::Break),
            StmtKind::Continue => Ok(Flow::Continue),
            StmtKind::Load(path_expr) => {
                let path_value = self.eval(path_expr)?;
                let path_text = value_to_path(&path_value);
                let resolved = self.resolve_script_path(&path_text).ok_or_else(|| {
                    BoryError::runtime(
                        format!("Could not resolve source file '{path_text}'"),
                        Some(statement.span),
                    )
                    .with_hint("Use a relative path like \"lib/tools.boy\" or an absolute path")
                })?;
                let loaded = self
                    .run_file(&resolved)
                    .map_err(|error| error.push_trace(format!("load {path_text}")))?;
                Ok(Flow::Next(loaded))
            }
        }
    }

    fn execute_while(&mut self, condition: &Expr, body: &[Stmt]) -> Result<Flow, BoryError> {
        let mut last_value = Value::Nil;
        while self.eval(condition)?.is_truthy() {
            match self.execute_statements(body)? {
                Flow::Next(value) => last_value = value,
                Flow::Return(value) => return Ok(Flow::Return(value)),
                Flow::Break => break,
                Flow::Continue => continue,
            }
        }
        Ok(Flow::Next(last_value))
    }

    fn execute_for_in(
        &mut self,
        name: &str,
        iterable: &Expr,
        body: &[Stmt],
        span: Span,
    ) -> Result<Flow, BoryError> {
        let iterable_value = self.eval(iterable)?;
        let items = self.iterable_items(iterable_value, span)?;
        let mut last_value = Value::Nil;

        for item in items {
            self.assign_or_define(name, item);
            match self.execute_statements(body)? {
                Flow::Next(value) => last_value = value,
                Flow::Return(value) => return Ok(Flow::Return(value)),
                Flow::Break => break,
                Flow::Continue => continue,
            }
        }

        Ok(Flow::Next(last_value))
    }

    fn execute_for_range(
        &mut self,
        name: &str,
        start: &Expr,
        end: &Expr,
        step: Option<&Expr>,
        body: &[Stmt],
        span: Span,
    ) -> Result<Flow, BoryError> {
        let start_value = expect_number(self.eval(start)?, span, "Range start must be numeric")?;
        let end_value = expect_number(self.eval(end)?, span, "Range end must be numeric")?;
        let step_value = if let Some(step) = step {
            expect_number(self.eval(step)?, span, "Range step must be numeric")?
        } else if start_value <= end_value {
            1.0
        } else {
            -1.0
        };

        if step_value == 0.0 {
            return Err(BoryError::runtime("Range step cannot be 0", Some(span)));
        }

        let mut current = start_value;
        let mut last_value = Value::Nil;
        while if step_value > 0.0 {
            current < end_value
        } else {
            current > end_value
        } {
            self.assign_or_define(name, Value::Number(current));
            match self.execute_statements(body)? {
                Flow::Next(value) => last_value = value,
                Flow::Return(value) => return Ok(Flow::Return(value)),
                Flow::Break => break,
                Flow::Continue => {}
            }
            current += step_value;
        }

        Ok(Flow::Next(last_value))
    }

    fn eval(&mut self, expr: &Expr) -> Result<Value, BoryError> {
        vm::eval_expr(self, expr)
    }

    fn apply_binary(
        &mut self,
        op: BinaryOp,
        left: Value,
        right: Value,
        span: Span,
    ) -> Result<Value, BoryError> {
        match op {
            BinaryOp::Add => self.binary_add(left, right, span),
            BinaryOp::Subtract => Ok(Value::Number(
                expect_number(left, span, "Subtraction requires numbers")?
                    - expect_number(right, span, "Subtraction requires numbers")?,
            )),
            BinaryOp::Multiply => self.binary_multiply(left, right, span),
            BinaryOp::Divide => {
                let divisor = expect_number(right, span, "Division requires numbers")?;
                if divisor == 0.0 {
                    return Err(BoryError::runtime("Cannot divide by 0", Some(span)));
                }
                Ok(Value::Number(
                    expect_number(left, span, "Division requires numbers")? / divisor,
                ))
            }
            BinaryOp::Modulo => {
                let divisor = expect_number(right, span, "Modulo requires numbers")?;
                if divisor == 0.0 {
                    return Err(BoryError::runtime("Cannot apply modulo by 0", Some(span)));
                }
                Ok(Value::Number(
                    expect_number(left, span, "Modulo requires numbers")? % divisor,
                ))
            }
            BinaryOp::Power => Ok(Value::Number(
                expect_number(left, span, "Power requires numbers")?
                    .powf(expect_number(right, span, "Power requires numbers")?),
            )),
            BinaryOp::Equal => Ok(Value::Bool(values_equal(&left, &right))),
            BinaryOp::NotEqual => Ok(Value::Bool(!values_equal(&left, &right))),
            BinaryOp::Greater => compare_values(left, right, span, |a, b| a > b, |a, b| a > b),
            BinaryOp::GreaterEqual => {
                compare_values(left, right, span, |a, b| a >= b, |a, b| a >= b)
            }
            BinaryOp::Less => compare_values(left, right, span, |a, b| a < b, |a, b| a < b),
            BinaryOp::LessEqual => {
                compare_values(left, right, span, |a, b| a <= b, |a, b| a <= b)
            }
            BinaryOp::And | BinaryOp::Or => unreachable!(),
            BinaryOp::In => self.binary_in(left, right, span),
        }
    }

    fn binary_add(&self, left: Value, right: Value, span: Span) -> Result<Value, BoryError> {
        match (left, right) {
            (Value::Number(a), Value::Number(b)) => Ok(Value::Number(a + b)),
            (Value::String(a), b) => Ok(Value::String(format!("{a}{b}"))),
            (a, Value::String(b)) => Ok(Value::String(format!("{a}{b}"))),
            (Value::List(a), Value::List(b)) => {
                let mut combined = a.borrow().iter().cloned().collect::<Vec<_>>();
                combined.extend(b.borrow().iter().cloned());
                Ok(Value::list(combined))
            }
            (Value::Object(a), Value::Object(b)) => {
                let mut combined = a.borrow().clone();
                for (key, value) in b.borrow().iter() {
                    combined.insert(key.clone(), value.clone());
                }
                Ok(Value::object(combined))
            }
            _ => Err(BoryError::runtime(
                "Cannot add those two value types",
                Some(span),
            )),
        }
    }

    fn binary_multiply(&self, left: Value, right: Value, span: Span) -> Result<Value, BoryError> {
        match (left, right) {
            (Value::Number(a), Value::Number(b)) => Ok(Value::Number(a * b)),
            (Value::String(text), Value::Number(times))
            | (Value::Number(times), Value::String(text)) => {
                let count = number_to_count(times, span)?;
                Ok(Value::String(text.repeat(count)))
            }
            (Value::List(list), Value::Number(times))
            | (Value::Number(times), Value::List(list)) => {
                let count = number_to_count(times, span)?;
                let original = list.borrow().clone();
                let mut result = Vec::new();
                for _ in 0..count {
                    result.extend(original.iter().cloned());
                }
                Ok(Value::list(result))
            }
            _ => Err(BoryError::runtime(
                "Cannot multiply those two value types",
                Some(span),
            )),
        }
    }

    fn binary_in(&self, left: Value, right: Value, span: Span) -> Result<Value, BoryError> {
        match right {
            Value::List(values) => Ok(Value::Bool(
                values.borrow().iter().any(|value| values_equal(value, &left)),
            )),
            Value::Object(values) => Ok(Value::Bool(values.borrow().contains_key(&left.to_string()))),
            Value::String(text) => Ok(Value::Bool(text.contains(&left.to_string()))),
            _ => Err(BoryError::runtime(
                "Operator 'in' only works with lists, objects, or text",
                Some(span),
            )),
        }
    }

    fn iterable_items(&self, value: Value, span: Span) -> Result<Vec<Value>, BoryError> {
        match value {
            Value::List(values) => Ok(values.borrow().iter().cloned().collect()),
            Value::String(text) => Ok(text
                .chars()
                .map(|ch| Value::String(ch.to_string()))
                .collect()),
            Value::Object(values) => Ok(values
                .borrow()
                .keys()
                .map(|key| Value::String(key.clone()))
                .collect()),
            _ => Err(BoryError::runtime(
                "That value cannot be iterated with 'for'",
                Some(span),
            )),
        }
    }

    fn call_value(
        &mut self,
        callee: Value,
        args: Vec<Value>,
        span: Option<Span>,
    ) -> Result<Value, BoryError> {
        match callee {
            Value::Type(typedef) => self.construct_type(typedef, args, span),
            Value::Function(function) => self.call_user_function(function, args, span),
            Value::NativeFunction(function) => {
                check_arity(&function.name, args.len(), function.min_arity, function.max_arity, span)?;
                (function.func)(self, args)
                    .map_err(|error| maybe_attach_span(error, span))
                    .map_err(|error| error.push_trace(format!("native {}", function.name)))
            }
            _ => Err(BoryError::runtime("That value is not callable", span)),
        }
    }

    fn construct_type(
        &mut self,
        typedef: Rc<TypeDef>,
        args: Vec<Value>,
        span: Option<Span>,
    ) -> Result<Value, BoryError> {
        check_arity(
            &typedef.name,
            args.len(),
            typedef.fields.len(),
            Some(typedef.fields.len()),
            span,
        )?;

        let mut instance = BTreeMap::new();
        let mut field_types = BTreeMap::new();
        instance.insert("__type__".to_string(), Value::String(typedef.name.clone()));
        instance.insert(
            "__kind__".to_string(),
            Value::String(typedef.flavor_name().to_string()),
        );
        for (field, value) in typedef.fields.iter().zip(args.into_iter()) {
            if let Some(type_hint) = &field.type_hint {
                ensure_type_match(
                    &value,
                    type_hint,
                    span.unwrap_or(Span::new(1, 1)),
                    &format!("Field '{}.{}'", typedef.name, field.name),
                )?;
                field_types.insert(field.name.clone(), Value::String(type_hint.render()));
            }
            instance.insert(field.name.clone(), value);
        }
        if !field_types.is_empty() {
            instance.insert("__field_types__".to_string(), Value::object(field_types));
        }

        let object = Value::object(instance);
        let instance_env = Environment::child(typedef.closure.clone());
        Environment::define(&instance_env, "self", object.clone());

        match self.execute_in_env(instance_env.clone(), |interpreter| vm::execute_program(interpreter, &typedef.body))
        .map_err(|error| maybe_attach_span(error, span))
        .map_err(|error| error.push_trace(format!("{} {}()", typedef.flavor_name(), typedef.name)))? {
            StatementOutcome::Next(_) => {}
            StatementOutcome::Return(_) => {
                return Err(BoryError::runtime(
                    "Cannot use 'give' directly inside a struct/class body",
                    span,
                ))
            }
            StatementOutcome::Break => {
                return Err(BoryError::runtime(
                    "Cannot use 'stop' outside a loop",
                    span,
                ))
            }
            StatementOutcome::Continue => {
                return Err(BoryError::runtime(
                    "Cannot use 'skip' outside a loop",
                    span,
                ))
            }
        }

        let local_members = Environment::snapshot_local(&instance_env);
        if let Value::Object(map) = &object {
            let mut borrowed = map.borrow_mut();
            for (key, value) in local_members {
                if key == "self" || key.starts_with('_') {
                    continue;
                }
                borrowed.insert(key, value);
            }
        }

        Ok(object)
    }

    fn call_user_function(
        &mut self,
        function: Rc<UserFunction>,
        args: Vec<Value>,
        span: Option<Span>,
    ) -> Result<Value, BoryError> {
        check_arity(
            &function.name,
            args.len(),
            function.params.len(),
            Some(function.params.len()),
            span,
        )?;

        let local = Environment::child(function.closure.clone());
        for (param, arg) in function.params.iter().zip(args.into_iter()) {
            if let Some(type_hint) = &param.type_hint {
                ensure_type_match(
                    &arg,
                    type_hint,
                    span.unwrap_or(Span::new(1, 1)),
                    &format!("Parameter '{}'", param.name),
                )?;
            }
            Environment::define_typed(&local, param.name.clone(), arg, param.type_hint.clone());
        }

        let result = self.execute_in_env(local, |interpreter| vm::execute_program(interpreter, &function.body));
        match result
            .map_err(|error| maybe_attach_span(error, span))
            .map_err(|error| error.push_trace(format!("task {}()", function.name)))?
        {
            StatementOutcome::Next(_) => {
                if let Some(return_type) = &function.return_type {
                    ensure_type_match(
                        &Value::Nil,
                        return_type,
                        span.unwrap_or(Span::new(1, 1)),
                        &format!("Return value from task '{}'", function.name),
                    )?;
                }
                Ok(Value::Nil)
            }
            StatementOutcome::Return(value) => {
                if let Some(return_type) = &function.return_type {
                    ensure_type_match(
                        &value,
                        return_type,
                        span.unwrap_or(Span::new(1, 1)),
                        &format!("Return value from task '{}'", function.name),
                    )?;
                }
                Ok(value)
            }
            StatementOutcome::Break => Err(BoryError::runtime("Cannot use 'stop' outside a loop", span)),
            StatementOutcome::Continue => Err(BoryError::runtime("Cannot use 'skip' outside a loop", span)),
        }
    }

    fn assign_target(
        &mut self,
        target: &AssignTarget,
        op: AssignOp,
        value_expr: &Expr,
        span: Span,
    ) -> Result<Value, BoryError> {
        let value = self.eval(value_expr)?;
        match target {
            AssignTarget::Variable(name) => {
                let next_value = if op == AssignOp::Set {
                    value
                } else {
                    let current = Environment::get_binding(&self.env, name).ok_or_else(|| {
                        BoryError::runtime(format!("Variable '{name}' does not exist"), Some(span))
                            .with_hint(format!("Declare it first with: var {name} = ..."))
                    })?;
                    self.apply_assign_op(op, current.value, value, span)?
                };

                if let Some(binding) = Environment::get_binding(&self.env, name) {
                    if let Some(type_hint) = &binding.type_hint {
                        ensure_type_match(
                            &next_value,
                            type_hint,
                            span,
                            &format!("Variable '{name}'"),
                        )?;
                    }
                }

                if !Environment::assign(&self.env, name, next_value.clone()) {
                    return Err(BoryError::runtime(
                        format!("Variable '{name}' does not exist"),
                        Some(span),
                    )
                    .with_hint(format!("Declare it first with: var {name} = ...")));
                }
                Ok(next_value)
            }
            AssignTarget::Index { object, index } => {
                let object_value = self.eval(object)?;
                let index_value = self.eval(index)?;
                self.write_index(object_value, index_value, value, op, span)
            }
            AssignTarget::Member { object, name } => {
                let object_value = self.eval(object)?;
                self.write_member(object_value, name, value, op, span)
            }
        }
    }

    fn apply_assign_op(
        &mut self,
        op: AssignOp,
        current: Value,
        value: Value,
        span: Span,
    ) -> Result<Value, BoryError> {
        match op {
            AssignOp::Set => Ok(value),
            AssignOp::Add => self.apply_binary(BinaryOp::Add, current, value, span),
            AssignOp::Subtract => self.apply_binary(BinaryOp::Subtract, current, value, span),
            AssignOp::Multiply => self.apply_binary(BinaryOp::Multiply, current, value, span),
            AssignOp::Divide => self.apply_binary(BinaryOp::Divide, current, value, span),
            AssignOp::Modulo => self.apply_binary(BinaryOp::Modulo, current, value, span),
        }
    }

    fn read_index(&self, object: Value, index: Value, span: Span) -> Result<Value, BoryError> {
        match object {
            Value::List(values) => {
                let idx = adjust_index(index, values.borrow().len(), span)?;
                Ok(values.borrow()[idx].clone())
            }
            Value::String(text) => {
                let chars = text.chars().collect::<Vec<_>>();
                let idx = adjust_index(index, chars.len(), span)?;
                Ok(Value::String(chars[idx].to_string()))
            }
            Value::Object(values) => values
                .borrow()
                .get(&index.to_string())
                .cloned()
                .ok_or_else(|| BoryError::runtime("That object key does not exist", Some(span))),
            _ => Err(BoryError::runtime(
                "That value does not support index access",
                Some(span),
            )),
        }
    }

    fn write_index(
        &mut self,
        object: Value,
        index: Value,
        value: Value,
        op: AssignOp,
        span: Span,
    ) -> Result<Value, BoryError> {
        match object {
            Value::List(values) => {
                let idx = adjust_index(index, values.borrow().len(), span)?;
                let next = {
                    let current = values.borrow()[idx].clone();
                    self.apply_assign_op(op, current, value, span)?
                };
                values.borrow_mut()[idx] = next.clone();
                Ok(next)
            }
            Value::Object(values) => {
                let key = index.to_string();
                let next = if op == AssignOp::Set {
                    value
                } else {
                    let current = values.borrow().get(&key).cloned().ok_or_else(|| {
                        BoryError::runtime("That object key does not exist", Some(span))
                    })?;
                    self.apply_assign_op(op, current, value, span)?
                };
                values.borrow_mut().insert(key, next.clone());
                Ok(next)
            }
            _ => Err(BoryError::runtime(
                "That value does not support index assignment",
                Some(span),
            )),
        }
    }

    fn read_member(&self, object: Value, name: &str, span: Span) -> Result<Value, BoryError> {
        match object {
            Value::Object(values) => values
                .borrow()
                .get(name)
                .cloned()
                .ok_or_else(|| BoryError::runtime(format!("Member '{name}' does not exist"), Some(span))),
            Value::List(values) if name == "size" => Ok(Value::Number(values.borrow().len() as f64)),
            Value::String(text) if name == "size" => Ok(Value::Number(text.chars().count() as f64)),
            _ => Err(BoryError::runtime(
                "Only objects expose members through '.'",
                Some(span),
            )),
        }
    }

    fn write_member(
        &mut self,
        object: Value,
        name: &str,
        value: Value,
        op: AssignOp,
        span: Span,
    ) -> Result<Value, BoryError> {
        match object {
            Value::Object(values) => {
                let next = if op == AssignOp::Set {
                    value
                } else {
                    let current = values.borrow().get(name).cloned().ok_or_else(|| {
                        BoryError::runtime(format!("Member '{name}' does not exist"), Some(span))
                    })?;
                    self.apply_assign_op(op, current, value, span)?
                };
                values.borrow_mut().insert(name.to_string(), next.clone());
                Ok(next)
            }
            _ => Err(BoryError::runtime(
                "Only objects support member assignment",
                Some(span),
            )),
        }
    }

    fn assign_or_define(&mut self, name: &str, value: Value) {
        if !Environment::assign(&self.env, name, value.clone()) {
            Environment::define(&self.env, name.to_string(), value);
        }
    }

    fn import_module(&mut self, spec: &str, span: Span) -> Result<Value, BoryError> {
        let resolved = self.resolve_module_path(spec, span)?;
        if let Some(cached) = self.module_cache.get(&resolved) {
            return Ok(cached.clone());
        }

        if self.module_stack.iter().any(|current| current == &resolved) {
            return Err(BoryError::runtime(
                format!("Circular module import detected for '{}'", resolved.display()),
                Some(span),
            ));
        }

        let source = std::fs::read_to_string(&resolved).map_err(|error| {
            BoryError::runtime(
                format!("Could not read module {}: {error}", resolved.display()),
                Some(span),
            )
        })?;

        let statements = parse_source(&source, &resolved.display().to_string())?;
        self.module_stack.push(resolved.clone());
        let module_env = Environment::child(self.globals.clone());
        Environment::define(
            &module_env,
            "__file__",
            Value::String(resolved.display().to_string()),
        );
        Environment::define(
            &module_env,
            "__name__",
            Value::String(module_name_from_path(&resolved)),
        );

        let execution = self.execute_in_env(module_env.clone(), |interpreter| interpreter.execute_program(&statements));
        self.module_stack.pop();

        execution?;
        let namespace = module_namespace_from_env(module_env);
        self.module_cache.insert(resolved, namespace.clone());
        Ok(namespace)
    }

    fn execute_in_env<T>(
        &mut self,
        env: EnvRef,
        action: impl FnOnce(&mut Self) -> Result<T, BoryError>,
    ) -> Result<T, BoryError> {
        let previous = self.env.clone();
        self.env = env;
        let result = action(self);
        self.env = previous;
        result
    }

    fn resolve_script_path(&self, raw: &str) -> Option<PathBuf> {
        let direct = self.resolve_path(raw);
        for candidate in source_candidates(direct) {
            if candidate.exists() {
                return Some(normalize_path(&candidate));
            }
        }
        None
    }

    fn resolve_module_path(&self, spec: &str, span: Span) -> Result<PathBuf, BoryError> {
        let looks_like_path = spec.contains('/') || spec.contains('\\') || spec.starts_with('.');
        if looks_like_path {
            return self.resolve_script_path(spec).ok_or_else(|| {
                BoryError::runtime(format!("Could not resolve module '{spec}'"), Some(span))
                    .with_hint("Use a relative path like \"lib/tools.boy\" or a dotted module name like toolkit.stats")
            });
        }

        let relative = PathBuf::from(spec.replace('.', "\\"));
        let mut roots = vec![
            self.current_base_dir(),
            self.current_base_dir().join("lib"),
            self.current_base_dir().join("libs"),
            self.current_base_dir().join("packages"),
        ];

        if let Ok(cwd) = std::env::current_dir() {
            roots.push(cwd.join("stdlib"));
            roots.push(cwd.join("lib"));
            roots.push(cwd.join("libs"));
            roots.push(cwd.join("packages"));
        }

        if let Ok(exe) = std::env::current_exe() {
            if let Some(bin_dir) = exe.parent() {
                roots.push(bin_dir.to_path_buf());
                roots.push(bin_dir.join("stdlib"));
                roots.push(bin_dir.join("lib"));
                roots.push(bin_dir.join("libs"));
                roots.push(bin_dir.join("packages"));
            }
        }

        for root in roots {
            for candidate in source_candidates(root.join(&relative)) {
                if candidate.exists() {
                    return Ok(normalize_path(&candidate));
                }
            }
        }

        Err(BoryError::runtime(
            format!("Could not resolve module '{spec}'"),
            Some(span),
        )
        .with_hint("Create <module>.boy, <module>/main.boy, or <module>/mod.boy"))
    }

    fn gc_roots(&self) -> Vec<Value> {
        let mut roots = Environment::snapshot_chain(&self.globals);
        roots.extend(Environment::snapshot_chain(&self.env));
        roots.extend(self.module_cache.values().cloned());
        roots
    }

    fn maybe_collect_garbage(&mut self) {
        self.gc_statement_counter += 1;
        let stats = heap_stats();
        if stats.active_lists + stats.active_objects >= self.gc_heap_threshold
            && self.gc_statement_counter >= self.gc_minor_interval
        {
            let _ = self.collect_garbage_minor();
            self.gc_statement_counter = 0;
        }
    }
}

impl Default for Interpreter {
    fn default() -> Self {
        Self::new()
    }
}

struct SimpleRng {
    state: u64,
}

impl SimpleRng {
    fn from_time() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos() as u64)
            .unwrap_or(0x9E3779B97F4A7C15);
        Self {
            state: if nanos == 0 { 0x9E3779B97F4A7C15 } else { nanos },
        }
    }

    fn seed(&mut self, seed: u64) {
        self.state = if seed == 0 { 0x9E3779B97F4A7C15 } else { seed };
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }

    fn next_f64(&mut self) -> f64 {
        self.next_u64() as f64 / u64::MAX as f64
    }
}

fn parse_source(source: &str, source_name: &str) -> Result<Vec<Stmt>, BoryError> {
    let tokens = tokenize(source).map_err(|error| attach_source(error, source_name, source))?;
    parse(tokens).map_err(|error| attach_source(error, source_name, source))
}

fn attach_source(mut error: BoryError, source_name: &str, source: &str) -> BoryError {
    if error.source_name.is_none() {
        error.source_name = Some(source_name.to_string());
    }
    if error.source_code.is_none() {
        error.source_code = Some(source.to_string());
    }
    error
}

fn maybe_attach_span(error: BoryError, span: Option<Span>) -> BoryError {
    match span {
        Some(span) => error.with_span_if_missing(span),
        None => error,
    }
}

fn normalize_path(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(path)
        }
    })
}

fn source_candidates(base: PathBuf) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if base.extension().is_some() {
        candidates.push(base);
    } else {
        candidates.push(base.with_extension("boy"));
        candidates.push(base.join("main.boy"));
        candidates.push(base.join("mod.boy"));
    }
    candidates
}

fn module_namespace_from_env(env: EnvRef) -> Value {
    let snapshot = Environment::snapshot_local(&env);
    let public_values = snapshot
        .into_iter()
        .filter(|(name, _)| !name.starts_with('_'))
        .collect::<BTreeMap<_, _>>();
    Value::object(public_values)
}

fn module_name_from_path(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("module")
        .to_string()
}

fn check_arity(
    name: &str,
    got: usize,
    min: usize,
    max: Option<usize>,
    span: Option<Span>,
) -> Result<(), BoryError> {
    if got < min {
        return Err(BoryError::runtime(
            format!("'{name}' expected at least {min} arguments but received {got}"),
            span,
        ));
    }
    if let Some(max) = max {
        if got > max {
            return Err(BoryError::runtime(
                format!("'{name}' expected at most {max} arguments but received {got}"),
                span,
            ));
        }
    }
    Ok(())
}

fn ensure_type_match(
    value: &Value,
    type_expr: &TypeExpr,
    span: Span,
    label: &str,
) -> Result<(), BoryError> {
    if type_matches(value, type_expr) {
        Ok(())
    } else {
        Err(BoryError::runtime(
            format!(
                "{label} expected type '{}' but received '{}'",
                type_expr.render(),
                value.type_name()
            ),
            Some(span),
        )
        .with_code("TYPE001")
        .with_note(format!("Runtime value: {value}"))
        .with_hint("Adjust the declared type or pass a value with the expected shape"))
    }
}

fn type_matches(value: &Value, type_expr: &TypeExpr) -> bool {
    match type_expr.name.as_str() {
        "any" => true,
        "number" => matches!(value, Value::Number(_)),
        "text" => matches!(value, Value::String(_)),
        "bool" => matches!(value, Value::Bool(_)),
        "nil" => matches!(value, Value::Nil),
        "list" => match value {
            Value::List(items) => {
                if let Some(item_type) = type_expr.args.first() {
                    items.borrow().iter().all(|item| type_matches(item, item_type))
                } else {
                    true
                }
            }
            _ => false,
        },
        "object" => matches!(value, Value::Object(_)),
        "task" => matches!(value, Value::Function(_)),
        "native-task" => matches!(value, Value::NativeFunction(_)),
        "job" => matches!(value, Value::Job(_)),
        "struct" => is_typed_instance(value, "struct", None),
        "class" => is_typed_instance(value, "class", None),
        custom_name => is_typed_instance(value, "", Some(custom_name)),
    }
}

fn is_typed_instance(value: &Value, expected_kind: &str, expected_name: Option<&str>) -> bool {
    let Value::Object(object) = value else {
        return false;
    };
    let borrowed = object.borrow();

    if let Some(expected_name) = expected_name {
        return borrowed
            .get("__type__")
            .is_some_and(|value| matches!(value, Value::String(name) if name == expected_name));
    }

    borrowed
        .get("__kind__")
        .is_some_and(|value| matches!(value, Value::String(kind) if kind == expected_kind))
}

fn expect_number(value: Value, span: Span, message: &str) -> Result<f64, BoryError> {
    match value {
        Value::Number(number) => Ok(number),
        _ => Err(BoryError::runtime(message, Some(span))),
    }
}

fn compare_values(
    left: Value,
    right: Value,
    span: Span,
    compare_number: impl FnOnce(f64, f64) -> bool,
    compare_text: impl FnOnce(String, String) -> bool,
) -> Result<Value, BoryError> {
    match (left, right) {
        (Value::Number(a), Value::Number(b)) => Ok(Value::Bool(compare_number(a, b))),
        (Value::String(a), Value::String(b)) => Ok(Value::Bool(compare_text(a, b))),
        _ => Err(BoryError::runtime(
            "Comparison requires two numbers or two text values",
            Some(span),
        )),
    }
}

fn adjust_index(index: Value, len: usize, span: Span) -> Result<usize, BoryError> {
    let raw = expect_number(index, span, "Index must be numeric")?;
    if raw.fract() != 0.0 {
        return Err(BoryError::runtime("Index must be an integer", Some(span)));
    }
    let raw = raw as isize;
    let resolved = if raw < 0 { len as isize + raw } else { raw };
    if resolved < 0 || resolved as usize >= len {
        return Err(BoryError::runtime("Index out of range", Some(span)));
    }
    Ok(resolved as usize)
}

fn number_to_count(value: f64, span: Span) -> Result<usize, BoryError> {
    if value.fract() != 0.0 || value < 0.0 {
        return Err(BoryError::runtime(
            "That number must be a positive integer",
            Some(span),
        ));
    }
    Ok(value as usize)
}

fn value_to_path(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        _ => value.to_string(),
    }
}
