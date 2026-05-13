use std::collections::BTreeMap;
use std::rc::Rc;

use crate::ast::{
    AssignOp, AssignTarget, BinaryOp, Expr, ExprKind, Literal, Stmt, StmtKind, TypeExpr,
    TypeFlavor, UnaryOp,
};
use crate::error::BoryError;
use crate::runtime::Interpreter;
use crate::span::Span;
use crate::value::{TypeDef, UserFunction, Value};

#[derive(Debug, Clone)]
enum ExprInstruction {
    LoadConst(Value),
    LoadName(String, Span),
    MakeList(usize),
    MakeObject(Vec<String>),
    Unary(UnaryOp, Span),
    Binary(BinaryOp, Span),
    Call(usize, Span),
    ReadIndex(Span),
    ReadMember(String, Span),
    JumpIfFalse(usize),
    JumpIfTrue(usize),
    Pop,
}

#[derive(Debug, Clone)]
struct ExprProgram {
    code: Vec<ExprInstruction>,
}

#[derive(Debug, Clone)]
enum StatementInstruction {
    Var {
        name: String,
        type_hint: Option<TypeExpr>,
        initializer: Option<ExprProgram>,
    },
    Use {
        spec: String,
        alias: String,
        span: Span,
    },
    Assign {
        target: AssignTarget,
        op: AssignOp,
        value: ExprProgram,
        span: Span,
    },
    Expr(ExprProgram),
    If {
        branches: Vec<(ExprProgram, StatementProgram)>,
        else_branch: Option<StatementProgram>,
    },
    While {
        condition: ExprProgram,
        body: StatementProgram,
    },
    ForIn {
        name: String,
        iterable: ExprProgram,
        body: StatementProgram,
        span: Span,
    },
    ForRange {
        name: String,
        start: ExprProgram,
        end: ExprProgram,
        step: Option<ExprProgram>,
        body: StatementProgram,
        span: Span,
    },
    Task {
        name: String,
        params: Vec<crate::ast::Param>,
        return_type: Option<TypeExpr>,
        body: Vec<Stmt>,
    },
    TypeDecl {
        flavor: TypeFlavor,
        name: String,
        fields: Vec<crate::ast::FieldDecl>,
        body: Vec<Stmt>,
    },
    Return(Option<ExprProgram>),
    Break,
    Continue,
    Load {
        path: ExprProgram,
        span: Span,
    },
}

#[derive(Debug, Clone)]
struct StatementProgram {
    code: Vec<StatementInstruction>,
}

#[derive(Debug, Clone)]
pub(crate) enum StatementOutcome {
    Next(Value),
    Return(Value),
    Break,
    Continue,
}

pub(crate) fn eval_expr(interpreter: &mut Interpreter, expr: &Expr) -> Result<Value, BoryError> {
    let program = compile_expr_program(expr);
    ExprVm::new(interpreter, program).run()
}

pub(crate) fn execute_program(
    interpreter: &mut Interpreter,
    statements: &[Stmt],
) -> Result<StatementOutcome, BoryError> {
    let program = compile_statement_program(statements);
    StatementVm::new(interpreter, program).run()
}

fn compile_expr_program(expr: &Expr) -> ExprProgram {
    let mut code = Vec::new();
    compile_expr(expr, &mut code);
    ExprProgram { code }
}

fn compile_expr(expr: &Expr, code: &mut Vec<ExprInstruction>) {
    match &expr.kind {
        ExprKind::Literal(literal) => code.push(ExprInstruction::LoadConst(match literal {
            Literal::Number(value) => Value::Number(*value),
            Literal::String(value) => Value::String(value.clone()),
            Literal::Bool(value) => Value::Bool(*value),
            Literal::Nil => Value::Nil,
        })),
        ExprKind::Variable(name) => code.push(ExprInstruction::LoadName(name.clone(), expr.span)),
        ExprKind::List(items) => {
            for item in items {
                compile_expr(item, code);
            }
            code.push(ExprInstruction::MakeList(items.len()));
        }
        ExprKind::Object(entries) => {
            let mut keys = Vec::with_capacity(entries.len());
            for (key, value) in entries {
                keys.push(key.clone());
                compile_expr(value, code);
            }
            code.push(ExprInstruction::MakeObject(keys));
        }
        ExprKind::Unary { op, right } => {
            compile_expr(right, code);
            code.push(ExprInstruction::Unary(*op, expr.span));
        }
        ExprKind::Binary { left, op, right } => match op {
            BinaryOp::And => {
                compile_expr(left, code);
                let jump_index = code.len();
                code.push(ExprInstruction::JumpIfFalse(usize::MAX));
                code.push(ExprInstruction::Pop);
                compile_expr(right, code);
                let end = code.len();
                code[jump_index] = ExprInstruction::JumpIfFalse(end);
            }
            BinaryOp::Or => {
                compile_expr(left, code);
                let jump_index = code.len();
                code.push(ExprInstruction::JumpIfTrue(usize::MAX));
                code.push(ExprInstruction::Pop);
                compile_expr(right, code);
                let end = code.len();
                code[jump_index] = ExprInstruction::JumpIfTrue(end);
            }
            _ => {
                compile_expr(left, code);
                compile_expr(right, code);
                code.push(ExprInstruction::Binary(*op, expr.span));
            }
        },
        ExprKind::Call { callee, args } => {
            compile_expr(callee, code);
            for arg in args {
                compile_expr(arg, code);
            }
            code.push(ExprInstruction::Call(args.len(), expr.span));
        }
        ExprKind::Index { object, index } => {
            compile_expr(object, code);
            compile_expr(index, code);
            code.push(ExprInstruction::ReadIndex(expr.span));
        }
        ExprKind::Member { object, name } => {
            compile_expr(object, code);
            code.push(ExprInstruction::ReadMember(name.clone(), expr.span));
        }
    }
}

fn compile_statement_program(statements: &[Stmt]) -> StatementProgram {
    StatementProgram {
        code: statements.iter().map(compile_statement).collect(),
    }
}

fn compile_statement(statement: &Stmt) -> StatementInstruction {
    match &statement.kind {
        StmtKind::Var {
            name,
            type_hint,
            initializer,
        } => StatementInstruction::Var {
            name: name.clone(),
            type_hint: type_hint.clone(),
            initializer: initializer.as_ref().map(compile_expr_program),
        },
        StmtKind::Use { spec, alias } => StatementInstruction::Use {
            spec: spec.clone(),
            alias: alias.clone(),
            span: statement.span,
        },
        StmtKind::Assign { target, op, value } => StatementInstruction::Assign {
            target: target.clone(),
            op: *op,
            value: compile_expr_program(value),
            span: statement.span,
        },
        StmtKind::Expr(expr) => StatementInstruction::Expr(compile_expr_program(expr)),
        StmtKind::If {
            branches,
            else_branch,
        } => StatementInstruction::If {
            branches: branches
                .iter()
                .map(|(condition, body)| (compile_expr_program(condition), compile_statement_program(body)))
                .collect(),
            else_branch: else_branch
                .as_ref()
                .map(|body| compile_statement_program(body)),
        },
        StmtKind::While { condition, body } => StatementInstruction::While {
            condition: compile_expr_program(condition),
            body: compile_statement_program(body),
        },
        StmtKind::ForIn {
            name,
            iterable,
            body,
        } => StatementInstruction::ForIn {
            name: name.clone(),
            iterable: compile_expr_program(iterable),
            body: compile_statement_program(body),
            span: statement.span,
        },
        StmtKind::ForRange {
            name,
            start,
            end,
            step,
            body,
        } => StatementInstruction::ForRange {
            name: name.clone(),
            start: compile_expr_program(start),
            end: compile_expr_program(end),
            step: step.as_ref().map(compile_expr_program),
            body: compile_statement_program(body),
            span: statement.span,
        },
        StmtKind::Task {
            name,
            params,
            return_type,
            body,
        } => StatementInstruction::Task {
            name: name.clone(),
            params: params.clone(),
            return_type: return_type.clone(),
            body: body.clone(),
        },
        StmtKind::TypeDecl {
            flavor,
            name,
            fields,
            body,
        } => StatementInstruction::TypeDecl {
            flavor: *flavor,
            name: name.clone(),
            fields: fields.clone(),
            body: body.clone(),
        },
        StmtKind::Return(value) => {
            StatementInstruction::Return(value.as_ref().map(compile_expr_program))
        }
        StmtKind::Break => StatementInstruction::Break,
        StmtKind::Continue => StatementInstruction::Continue,
        StmtKind::Load(expr) => StatementInstruction::Load {
            path: compile_expr_program(expr),
            span: statement.span,
        },
    }
}

struct ExprVm<'a> {
    interpreter: &'a mut Interpreter,
    program: ExprProgram,
    stack: Vec<Value>,
    ip: usize,
}

impl<'a> ExprVm<'a> {
    fn new(interpreter: &'a mut Interpreter, program: ExprProgram) -> Self {
        Self {
            interpreter,
            program,
            stack: Vec::new(),
            ip: 0,
        }
    }

    fn run(mut self) -> Result<Value, BoryError> {
        while self.ip < self.program.code.len() {
            let instruction = self.program.code[self.ip].clone();
            self.ip += 1;

            match instruction {
                ExprInstruction::LoadConst(value) => self.stack.push(value),
                ExprInstruction::LoadName(name, span) => {
                    self.stack.push(self.interpreter.read_variable(&name, span)?);
                }
                ExprInstruction::MakeList(count) => {
                    let items = self.pop_many(count)?;
                    self.stack.push(Value::list(items));
                }
                ExprInstruction::MakeObject(keys) => {
                    let values = self.pop_many(keys.len())?;
                    let mut object = BTreeMap::new();
                    for (key, value) in keys.into_iter().zip(values.into_iter()) {
                        object.insert(key, value);
                    }
                    self.stack.push(Value::object(object));
                }
                ExprInstruction::Unary(op, span) => {
                    let right = self.pop()?;
                    let value = match op {
                        UnaryOp::Not => Value::Bool(!right.is_truthy()),
                        UnaryOp::Negate => match right {
                            Value::Number(number) => Value::Number(-number),
                            _ => {
                                return Err(BoryError::runtime(
                                    "Cannot negate a non numeric value",
                                    Some(span),
                                ))
                            }
                        },
                    };
                    self.stack.push(value);
                }
                ExprInstruction::Binary(op, span) => {
                    let right = self.pop()?;
                    let left = self.pop()?;
                    let value = self.interpreter.apply_binary_vm(op, left, right, span)?;
                    self.stack.push(value);
                }
                ExprInstruction::Call(arg_count, span) => {
                    let args = self.pop_many(arg_count)?;
                    let callee = self.pop()?;
                    let value = self.interpreter.call_public(callee, args, Some(span))?;
                    self.stack.push(value);
                }
                ExprInstruction::ReadIndex(span) => {
                    let index = self.pop()?;
                    let object = self.pop()?;
                    let value = self.interpreter.read_index_vm(object, index, span)?;
                    self.stack.push(value);
                }
                ExprInstruction::ReadMember(name, span) => {
                    let object = self.pop()?;
                    let value = self.interpreter.read_member_vm(object, &name, span)?;
                    self.stack.push(value);
                }
                ExprInstruction::JumpIfFalse(target) => {
                    if !self.stack.last().is_some_and(Value::is_truthy) {
                        self.ip = target;
                    }
                }
                ExprInstruction::JumpIfTrue(target) => {
                    if self.stack.last().is_some_and(Value::is_truthy) {
                        self.ip = target;
                    }
                }
                ExprInstruction::Pop => {
                    let _ = self.pop()?;
                }
            }
        }

        Ok(self.stack.pop().unwrap_or(Value::Nil))
    }

    fn pop(&mut self) -> Result<Value, BoryError> {
        self.stack.pop().ok_or_else(|| {
            BoryError::runtime("Internal VM stack underflow", Some(Span::new(1, 1)))
        })
    }

    fn pop_many(&mut self, count: usize) -> Result<Vec<Value>, BoryError> {
        if self.stack.len() < count {
            return Err(BoryError::runtime(
                "Internal VM stack underflow",
                Some(Span::new(1, 1)),
            ));
        }
        let start = self.stack.len() - count;
        Ok(self.stack.drain(start..).collect())
    }
}

struct StatementVm<'a> {
    interpreter: &'a mut Interpreter,
    program: StatementProgram,
}

impl<'a> StatementVm<'a> {
    fn new(interpreter: &'a mut Interpreter, program: StatementProgram) -> Self {
        Self { interpreter, program }
    }

    fn run(&mut self) -> Result<StatementOutcome, BoryError> {
        let mut last_value = Value::Nil;

        for instruction in self.program.code.clone() {
            match self.execute_instruction(instruction)? {
                StatementOutcome::Next(value) => {
                    last_value = value;
                    self.interpreter.maybe_collect_garbage_public();
                }
                other => return Ok(other),
            }
        }

        Ok(StatementOutcome::Next(last_value))
    }

    fn execute_instruction(
        &mut self,
        instruction: StatementInstruction,
    ) -> Result<StatementOutcome, BoryError> {
        match instruction {
            StatementInstruction::Var {
                name,
                type_hint,
                initializer,
            } => {
                let value = if let Some(initializer) = initializer {
                    ExprVm::new(self.interpreter, initializer).run()?
                } else {
                    Value::Nil
                };
                self.interpreter.define_local(name, value.clone(), type_hint);
                Ok(StatementOutcome::Next(value))
            }
            StatementInstruction::Use { spec, alias, span } => {
                let module = self
                    .interpreter
                    .import_module_public(&spec, span)
                    .map_err(|error| error.push_trace(format!("use {spec} as {alias}")))?;
                self.interpreter.define_local(alias, module.clone(), None);
                Ok(StatementOutcome::Next(module))
            }
            StatementInstruction::Assign {
                target,
                op,
                value,
                span,
            } => {
                let resolved = ExprVm::new(self.interpreter, value).run()?;
                let assigned = self
                    .interpreter
                    .assign_resolved_target(&target, op, resolved, span)?;
                Ok(StatementOutcome::Next(assigned))
            }
            StatementInstruction::Expr(program) => {
                let value = ExprVm::new(self.interpreter, program).run()?;
                Ok(StatementOutcome::Next(value))
            }
            StatementInstruction::If {
                branches,
                else_branch,
            } => {
                for (condition, body) in branches {
                    if ExprVm::new(self.interpreter, condition).run()?.is_truthy() {
                        return StatementVm::new(self.interpreter, body).run();
                    }
                }
                if let Some(body) = else_branch {
                    StatementVm::new(self.interpreter, body).run()
                } else {
                    Ok(StatementOutcome::Next(Value::Nil))
                }
            }
            StatementInstruction::While { condition, body } => {
                let mut last_value = Value::Nil;
                while ExprVm::new(self.interpreter, condition.clone()).run()?.is_truthy() {
                    match StatementVm::new(self.interpreter, body.clone()).run()? {
                        StatementOutcome::Next(value) => last_value = value,
                        StatementOutcome::Return(value) => return Ok(StatementOutcome::Return(value)),
                        StatementOutcome::Break => break,
                        StatementOutcome::Continue => continue,
                    }
                }
                Ok(StatementOutcome::Next(last_value))
            }
            StatementInstruction::ForIn {
                name,
                iterable,
                body,
                span,
            } => {
                let iterable_value = ExprVm::new(self.interpreter, iterable).run()?;
                let items = self.interpreter.iterable_items_public(iterable_value, span)?;
                let mut last_value = Value::Nil;
                for item in items {
                    self.interpreter.assign_or_define_public(&name, item);
                    match StatementVm::new(self.interpreter, body.clone()).run()? {
                        StatementOutcome::Next(value) => last_value = value,
                        StatementOutcome::Return(value) => return Ok(StatementOutcome::Return(value)),
                        StatementOutcome::Break => break,
                        StatementOutcome::Continue => continue,
                    }
                }
                Ok(StatementOutcome::Next(last_value))
            }
            StatementInstruction::ForRange {
                name,
                start,
                end,
                step,
                body,
                span,
            } => {
                let start_value =
                    expect_number(ExprVm::new(self.interpreter, start).run()?, span, "Range start must be numeric")?;
                let end_value =
                    expect_number(ExprVm::new(self.interpreter, end).run()?, span, "Range end must be numeric")?;
                let step_value = if let Some(step) = step {
                    expect_number(ExprVm::new(self.interpreter, step).run()?, span, "Range step must be numeric")?
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
                    self.interpreter
                        .assign_or_define_public(&name, Value::Number(current));
                    match StatementVm::new(self.interpreter, body.clone()).run()? {
                        StatementOutcome::Next(value) => last_value = value,
                        StatementOutcome::Return(value) => return Ok(StatementOutcome::Return(value)),
                        StatementOutcome::Break => break,
                        StatementOutcome::Continue => {}
                    }
                    current += step_value;
                }
                Ok(StatementOutcome::Next(last_value))
            }
            StatementInstruction::Task {
                name,
                params,
                return_type,
                body,
            } => {
                let function = Value::Function(Rc::new(UserFunction::new(
                    name.clone(),
                    params,
                    return_type,
                    body,
                    self.interpreter.current_env(),
                )));
                self.interpreter.define_local(name, function.clone(), None);
                Ok(StatementOutcome::Next(function))
            }
            StatementInstruction::TypeDecl {
                flavor,
                name,
                fields,
                body,
            } => {
                let typedef = Value::Type(Rc::new(TypeDef::new(
                    flavor,
                    name.clone(),
                    fields,
                    body,
                    self.interpreter.current_env(),
                )));
                self.interpreter.define_local(name, typedef.clone(), None);
                Ok(StatementOutcome::Next(typedef))
            }
            StatementInstruction::Return(value) => Ok(StatementOutcome::Return(match value {
                Some(program) => ExprVm::new(self.interpreter, program).run()?,
                None => Value::Nil,
            })),
            StatementInstruction::Break => Ok(StatementOutcome::Break),
            StatementInstruction::Continue => Ok(StatementOutcome::Continue),
            StatementInstruction::Load { path, span } => {
                let path_value = ExprVm::new(self.interpreter, path).run()?;
                let path_text = match path_value {
                    Value::String(text) => text,
                    other => other.to_string(),
                };
                let loaded = self.interpreter.load_path_public(&path_text, span)?;
                Ok(StatementOutcome::Next(loaded))
            }
        }
    }
}

fn expect_number(value: Value, span: Span, message: &str) -> Result<f64, BoryError> {
    match value {
        Value::Number(number) => Ok(number),
        _ => Err(BoryError::runtime(message, Some(span))),
    }
}
