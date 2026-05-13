use std::collections::BTreeMap;

use crate::ast::{
    AssignOp, AssignTarget, BinaryOp, Expr, ExprKind, Literal, Stmt, StmtKind, TypeExpr,
    UnaryOp,
};
use crate::error::BoryError;
use crate::span::Span;

#[derive(Debug, Clone, PartialEq, Eq)]
enum StaticType {
    Unknown,
    Any,
    Nil,
    Bool,
    Number,
    Text,
    List(Box<StaticType>),
    Object,
    Function(FunctionSig),
    TypeCtor(String),
    Instance(String),
    NativeTask,
    Job,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FunctionSig {
    params: Vec<StaticType>,
    return_type: Box<StaticType>,
}

#[derive(Clone, Default)]
struct TypeEnv {
    values: BTreeMap<String, StaticType>,
}

pub fn check_program(statements: &[Stmt]) -> Result<(), BoryError> {
    let mut checker = TypeChecker::new();
    checker.check_block(statements).map(|_| ())
}

struct TypeChecker {
    scopes: Vec<TypeEnv>,
}

impl TypeChecker {
    fn new() -> Self {
        let mut checker = Self {
            scopes: vec![TypeEnv::default()],
        };
        checker.seed_builtins();
        checker
    }

    fn seed_builtins(&mut self) {
        let globals = self.scopes.last_mut().unwrap();
        globals.values.insert(
            "echo".to_string(),
            StaticType::Function(FunctionSig {
                params: vec![StaticType::Any],
                return_type: Box::new(StaticType::Nil),
            }),
        );
        globals.values.insert(
            "size".to_string(),
            StaticType::Function(FunctionSig {
                params: vec![StaticType::Any],
                return_type: Box::new(StaticType::Number),
            }),
        );
        for module in [
            "math", "rand", "sys", "json", "text", "matrix", "clock", "net", "http", "flow",
            "gc", "screen",
        ] {
            globals.values.insert(module.to_string(), StaticType::Object);
        }
    }

    fn check_block(&mut self, statements: &[Stmt]) -> Result<StaticType, BoryError> {
        let mut return_type = StaticType::Unknown;
        for statement in statements {
            let stmt_return = self.check_stmt(statement)?;
            return_type = merge_types(return_type, stmt_return);
        }
        Ok(return_type)
    }

    fn check_stmt(&mut self, statement: &Stmt) -> Result<StaticType, BoryError> {
        match &statement.kind {
            StmtKind::Var {
                name,
                type_hint,
                initializer,
            } => {
                let inferred = if let Some(initializer) = initializer {
                    self.infer_expr(initializer)?
                } else {
                    StaticType::Nil
                };
                let final_type = if let Some(type_hint) = type_hint {
                    let declared = static_from_type_expr(type_hint);
                    self.ensure_compatible(
                        &inferred,
                        &declared,
                        statement.span,
                        &format!("Variable '{name}'"),
                    )?;
                    declared
                } else {
                    inferred.clone()
                };
                self.define(name.clone(), final_type);
                Ok(StaticType::Unknown)
            }
            StmtKind::Assign { target, op, value } => {
                let value_type = self.infer_expr(value)?;
                match target {
                    AssignTarget::Variable(name) => {
                        let current = self.lookup(name);
                        let next = if *op == AssignOp::Set {
                            value_type.clone()
                        } else {
                            let current = current.clone().unwrap_or(StaticType::Unknown);
                            self.infer_assign_result(&current, &value_type, *op, statement.span)?
                        };
                        if let Some(current) = current.as_ref() {
                            self.ensure_compatible(
                                &next,
                                current,
                                statement.span,
                                &format!("Variable '{name}'"),
                            )?;
                        }
                        self.assign(name, next);
                    }
                    AssignTarget::Index { object, .. } => {
                        let object_type = self.infer_expr(object)?;
                        match object_type {
                            StaticType::List(item) => {
                                self.ensure_compatible(
                                    &value_type,
                                    &item,
                                    statement.span,
                                    "List assignment",
                                )?;
                            }
                            StaticType::Object | StaticType::Unknown | StaticType::Any => {}
                            _ => {
                                return Err(type_error(
                                    statement.span,
                                    "Indexed assignment needs a list or object",
                                ))
                            }
                        }
                    }
                    AssignTarget::Member { .. } => {}
                }
                Ok(StaticType::Unknown)
            }
            StmtKind::Expr(expr) => {
                let _ = self.infer_expr(expr)?;
                Ok(StaticType::Unknown)
            }
            StmtKind::If {
                branches,
                else_branch,
            } => {
                let mut return_type = StaticType::Unknown;
                for (condition, body) in branches {
                    let condition_type = self.infer_expr(condition)?;
                    self.ensure_condition(&condition_type, condition.span)?;
                    self.push_scope();
                    let branch_return = self.check_block(body)?;
                    self.pop_scope();
                    return_type = merge_types(return_type, branch_return);
                }
                if let Some(body) = else_branch {
                    self.push_scope();
                    let branch_return = self.check_block(body)?;
                    self.pop_scope();
                    return_type = merge_types(return_type, branch_return);
                }
                Ok(return_type)
            }
            StmtKind::While { condition, body } => {
                let condition_type = self.infer_expr(condition)?;
                self.ensure_condition(&condition_type, condition.span)?;
                self.push_scope();
                let branch_return = self.check_block(body)?;
                self.pop_scope();
                Ok(branch_return)
            }
            StmtKind::ForIn {
                name,
                iterable,
                body,
            } => {
                let iterable_type = self.infer_expr(iterable)?;
                let item_type = match iterable_type {
                    StaticType::List(item) => *item,
                    StaticType::Text => StaticType::Text,
                    StaticType::Object | StaticType::Unknown | StaticType::Any => StaticType::Any,
                    _ => {
                        return Err(type_error(
                            iterable.span,
                            "for-in expects a list, text, or object",
                        ))
                    }
                };
                self.push_scope();
                self.define(name.clone(), item_type);
                let branch_return = self.check_block(body)?;
                self.pop_scope();
                Ok(branch_return)
            }
            StmtKind::ForRange {
                name,
                start,
                end,
                step,
                body,
            } => {
                let start_type = self.infer_expr(start)?;
                let end_type = self.infer_expr(end)?;
                self.ensure_compatible(
                    &start_type,
                    &StaticType::Number,
                    start.span,
                    "Range start",
                )?;
                self.ensure_compatible(
                    &end_type,
                    &StaticType::Number,
                    end.span,
                    "Range end",
                )?;
                if let Some(step) = step {
                    let step_type = self.infer_expr(step)?;
                    self.ensure_compatible(
                        &step_type,
                        &StaticType::Number,
                        step.span,
                        "Range step",
                    )?;
                }
                self.push_scope();
                self.define(name.clone(), StaticType::Number);
                let branch_return = self.check_block(body)?;
                self.pop_scope();
                Ok(branch_return)
            }
            StmtKind::Task {
                name,
                params,
                return_type,
                body,
            } => {
                let param_types = params
                    .iter()
                    .map(|param| {
                        param.type_hint.as_ref().map_or(StaticType::Unknown, static_from_type_expr)
                    })
                    .collect::<Vec<_>>();
                let declared_return = return_type
                    .as_ref()
                    .map_or(StaticType::Unknown, static_from_type_expr);

                self.define(
                    name.clone(),
                    StaticType::Function(FunctionSig {
                        params: param_types.clone(),
                        return_type: Box::new(declared_return.clone()),
                    }),
                );

                self.push_scope();
                for (param, ty) in params.iter().zip(param_types.into_iter()) {
                    self.define(param.name.clone(), ty);
                }
                let inferred_return = self.check_block(body)?;
                self.pop_scope();

                let final_return = if return_type.is_some() {
                    self.ensure_compatible(
                        &inferred_return,
                        &declared_return,
                        statement.span,
                        &format!("Task '{name}' return"),
                    )?;
                    declared_return
                } else {
                    inferred_return
                };

                self.assign(
                    name,
                    StaticType::Function(FunctionSig {
                        params: params
                            .iter()
                            .map(|param| {
                                param.type_hint.as_ref().map_or(StaticType::Unknown, static_from_type_expr)
                            })
                            .collect(),
                        return_type: Box::new(final_return),
                    }),
                );
                Ok(StaticType::Unknown)
            }
            StmtKind::TypeDecl {
                flavor,
                name,
                fields,
                body,
            } => {
                let _ = flavor;
                self.define(name.clone(), StaticType::TypeCtor(name.clone()));
                self.push_scope();
                self.define("self".to_string(), StaticType::Instance(name.clone()));
                for field in fields {
                    self.define(
                        field.name.clone(),
                        field.type_hint.as_ref().map_or(StaticType::Unknown, static_from_type_expr),
                    );
                }
                let _ = self.check_block(body)?;
                self.pop_scope();
                Ok(StaticType::Unknown)
            }
            StmtKind::Use { alias, .. } => {
                self.define(alias.clone(), StaticType::Object);
                Ok(StaticType::Unknown)
            }
            StmtKind::Return(value) => Ok(match value {
                Some(expr) => self.infer_expr(expr)?,
                None => StaticType::Nil,
            }),
            StmtKind::Break | StmtKind::Continue => Ok(StaticType::Unknown),
            StmtKind::Load(_) => Ok(StaticType::Unknown),
        }
    }

    fn infer_expr(&mut self, expr: &Expr) -> Result<StaticType, BoryError> {
        Ok(match &expr.kind {
            ExprKind::Literal(literal) => match literal {
                Literal::Number(_) => StaticType::Number,
                Literal::String(_) => StaticType::Text,
                Literal::Bool(_) => StaticType::Bool,
                Literal::Nil => StaticType::Nil,
            },
            ExprKind::Variable(name) => self.lookup(name).unwrap_or(StaticType::Unknown),
            ExprKind::List(items) => {
                let mut item_type = StaticType::Unknown;
                for item in items {
                    item_type = merge_types(item_type, self.infer_expr(item)?);
                }
                StaticType::List(Box::new(item_type))
            }
            ExprKind::Object(entries) => {
                for (_, value) in entries {
                    let _ = self.infer_expr(value)?;
                }
                StaticType::Object
            }
            ExprKind::Unary { op, right } => {
                let right_type = self.infer_expr(right)?;
                match op {
                    UnaryOp::Not => {
                        self.ensure_condition(&right_type, expr.span)?;
                        StaticType::Bool
                    }
                    UnaryOp::Negate => {
                        self.ensure_compatible(&right_type, &StaticType::Number, expr.span, "Unary '-'")?;
                        StaticType::Number
                    }
                }
            }
            ExprKind::Binary { left, op, right } => {
                let left_type = self.infer_expr(left)?;
                let right_type = self.infer_expr(right)?;
                self.infer_binary(&left_type, *op, &right_type, expr.span)?
            }
            ExprKind::Call { callee, args } => {
                let callee_type = self.infer_expr(callee)?;
                let arg_types = args
                    .iter()
                    .map(|arg| self.infer_expr(arg))
                    .collect::<Result<Vec<_>, _>>()?;
                match callee_type {
                    StaticType::Function(signature) => {
                        for (index, expected) in signature.params.iter().enumerate() {
                            if let Some(actual) = arg_types.get(index) {
                                self.ensure_compatible(
                                    actual,
                                    expected,
                                    expr.span,
                                    &format!("Argument {}", index + 1),
                                )?;
                            }
                        }
                        *signature.return_type
                    }
                    StaticType::TypeCtor(name) => StaticType::Instance(name),
                    StaticType::NativeTask | StaticType::Unknown | StaticType::Any | StaticType::Object => {
                        StaticType::Unknown
                    }
                    _ => {
                        return Err(type_error(
                            expr.span,
                            "That expression is not statically callable",
                        ))
                    }
                }
            }
            ExprKind::Index { object, .. } => match self.infer_expr(object)? {
                StaticType::List(item) => *item,
                StaticType::Text => StaticType::Text,
                StaticType::Object | StaticType::Unknown | StaticType::Any => StaticType::Unknown,
                _ => {
                    return Err(type_error(
                        expr.span,
                        "Indexed access expects a list, text, or object",
                    ))
                }
            },
            ExprKind::Member { object, name } => match self.infer_expr(object)? {
                StaticType::List(_) | StaticType::Text if name == "size" => StaticType::Number,
                StaticType::Object
                | StaticType::Instance(_)
                | StaticType::Unknown
                | StaticType::Any => StaticType::Unknown,
                _ => StaticType::Unknown,
            },
        })
    }

    fn infer_binary(
        &self,
        left: &StaticType,
        op: BinaryOp,
        right: &StaticType,
        span: Span,
    ) -> Result<StaticType, BoryError> {
        Ok(match op {
            BinaryOp::Add => match (left, right) {
                (StaticType::Number, StaticType::Number) => StaticType::Number,
                (StaticType::Text, _) | (_, StaticType::Text) => StaticType::Text,
                (StaticType::List(a), StaticType::List(b)) => {
                    StaticType::List(Box::new(merge_types((**a).clone(), (**b).clone())))
                }
                (StaticType::Object, StaticType::Object) => StaticType::Object,
                (StaticType::Unknown, _) | (_, StaticType::Unknown) | (StaticType::Any, _) | (_, StaticType::Any) => {
                    StaticType::Unknown
                }
                _ => return Err(type_error(span, "Operator '+' is not valid for those static types")),
            },
            BinaryOp::Subtract | BinaryOp::Divide | BinaryOp::Modulo | BinaryOp::Power => {
                self.compat_numbers(left, right, span)?;
                StaticType::Number
            }
            BinaryOp::Multiply => match (left, right) {
                (StaticType::Number, StaticType::Number) => StaticType::Number,
                (StaticType::Text, StaticType::Number) | (StaticType::Number, StaticType::Text) => {
                    StaticType::Text
                }
                (StaticType::List(item), StaticType::Number)
                | (StaticType::Number, StaticType::List(item)) => StaticType::List(item.clone()),
                (StaticType::Unknown, _) | (_, StaticType::Unknown) | (StaticType::Any, _) | (_, StaticType::Any) => {
                    StaticType::Unknown
                }
                _ => return Err(type_error(span, "Operator '*' is not valid for those static types")),
            },
            BinaryOp::Equal
            | BinaryOp::NotEqual
            | BinaryOp::Greater
            | BinaryOp::GreaterEqual
            | BinaryOp::Less
            | BinaryOp::LessEqual
            | BinaryOp::In => StaticType::Bool,
            BinaryOp::And | BinaryOp::Or => StaticType::Bool,
        })
    }

    fn compat_numbers(
        &self,
        left: &StaticType,
        right: &StaticType,
        span: Span,
    ) -> Result<(), BoryError> {
        self.ensure_compatible(left, &StaticType::Number, span, "Left numeric operand")?;
        self.ensure_compatible(right, &StaticType::Number, span, "Right numeric operand")
    }

    fn infer_assign_result(
        &self,
        current: &StaticType,
        value: &StaticType,
        op: AssignOp,
        span: Span,
    ) -> Result<StaticType, BoryError> {
        let binary = match op {
            AssignOp::Set => return Ok(value.clone()),
            AssignOp::Add => BinaryOp::Add,
            AssignOp::Subtract => BinaryOp::Subtract,
            AssignOp::Multiply => BinaryOp::Multiply,
            AssignOp::Divide => BinaryOp::Divide,
            AssignOp::Modulo => BinaryOp::Modulo,
        };
        self.infer_binary(current, binary, value, span)
    }

    fn ensure_condition(&self, actual: &StaticType, span: Span) -> Result<(), BoryError> {
        match actual {
            StaticType::Number
            | StaticType::Text
            | StaticType::Bool
            | StaticType::List(_)
            | StaticType::Object
            | StaticType::Nil
            | StaticType::Unknown
            | StaticType::Any
            | StaticType::Instance(_)
            | StaticType::Function(_)
            | StaticType::NativeTask
            | StaticType::Job => Ok(()),
            _ => Err(type_error(span, "That expression cannot be used as a condition")),
        }
    }

    fn ensure_compatible(
        &self,
        actual: &StaticType,
        expected: &StaticType,
        span: Span,
        label: &str,
    ) -> Result<(), BoryError> {
        if is_compatible(actual, expected) {
            Ok(())
        } else {
            Err(BoryError::runtime(
                format!(
                    "{label} expected static type '{}' but inferred '{}'",
                    render_static_type(expected),
                    render_static_type(actual)
                ),
                Some(span),
            )
            .with_code("TYPECHECK001")
            .with_hint("Fix the declaration, add an annotation, or change the expression"))
        }
    }

    fn push_scope(&mut self) {
        self.scopes.push(TypeEnv::default());
    }

    fn pop_scope(&mut self) {
        let _ = self.scopes.pop();
    }

    fn define(&mut self, name: String, ty: StaticType) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.values.insert(name, ty);
        }
    }

    fn assign(&mut self, name: &str, ty: StaticType) {
        for scope in self.scopes.iter_mut().rev() {
            if scope.values.contains_key(name) {
                scope.values.insert(name.to_string(), ty);
                return;
            }
        }
        self.define(name.to_string(), ty);
    }

    fn lookup(&self, name: &str) -> Option<StaticType> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.values.get(name).cloned())
    }
}

fn static_from_type_expr(expr: &TypeExpr) -> StaticType {
    match expr.name.as_str() {
        "any" => StaticType::Any,
        "number" => StaticType::Number,
        "text" => StaticType::Text,
        "bool" => StaticType::Bool,
        "nil" => StaticType::Nil,
        "object" => StaticType::Object,
        "task" => StaticType::Function(FunctionSig {
            params: vec![StaticType::Any],
            return_type: Box::new(StaticType::Any),
        }),
        "native-task" => StaticType::NativeTask,
        "job" => StaticType::Job,
        "list" => {
            let item = expr
                .args
                .first()
                .map(static_from_type_expr)
                .unwrap_or(StaticType::Any);
            StaticType::List(Box::new(item))
        }
        other => StaticType::Instance(other.to_string()),
    }
}

fn merge_types(left: StaticType, right: StaticType) -> StaticType {
    if left == StaticType::Unknown {
        return right;
    }
    if right == StaticType::Unknown {
        return left;
    }
    if left == right {
        return left;
    }
    match (left, right) {
        (StaticType::List(a), StaticType::List(b)) => StaticType::List(Box::new(merge_types(*a, *b))),
        _ => StaticType::Any,
    }
}

fn is_compatible(actual: &StaticType, expected: &StaticType) -> bool {
    match (actual, expected) {
        (_, StaticType::Any) | (StaticType::Unknown, _) | (_, StaticType::Unknown) => true,
        (a, b) if a == b => true,
        (StaticType::List(a), StaticType::List(b)) => is_compatible(a, b),
        (StaticType::Nil, StaticType::Instance(_)) => false,
        (StaticType::Instance(a), StaticType::Instance(b)) => a == b,
        (StaticType::Function(_), StaticType::Function(_)) => true,
        _ => false,
    }
}

fn render_static_type(ty: &StaticType) -> String {
    match ty {
        StaticType::Unknown => "unknown".to_string(),
        StaticType::Any => "any".to_string(),
        StaticType::Nil => "nil".to_string(),
        StaticType::Bool => "bool".to_string(),
        StaticType::Number => "number".to_string(),
        StaticType::Text => "text".to_string(),
        StaticType::List(item) => format!("list[{}]", render_static_type(item)),
        StaticType::Object => "object".to_string(),
        StaticType::Function(signature) => format!(
            "task({}) -> {}",
            signature
                .params
                .iter()
                .map(render_static_type)
                .collect::<Vec<_>>()
                .join(", "),
            render_static_type(&signature.return_type)
        ),
        StaticType::TypeCtor(name) => format!("type<{name}>"),
        StaticType::Instance(name) => name.clone(),
        StaticType::NativeTask => "native-task".to_string(),
        StaticType::Job => "job".to_string(),
    }
}

fn type_error(span: Span, message: &str) -> BoryError {
    BoryError::runtime(message, Some(span))
        .with_code("TYPECHECK001")
        .with_hint("Review the expression types before execution")
}
