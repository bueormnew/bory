use crate::ast::{AssignTarget, Expr, ExprKind, FieldDecl, Literal, Param, Stmt, StmtKind};
use crate::error::BoryError;
use crate::lexer::tokenize;
use crate::parser::parse;

pub fn format_source(source: &str, source_name: &str) -> Result<String, BoryError> {
    let tokens = tokenize(source).map_err(|error| error.with_source_context(source_name, source))?;
    let statements = parse(tokens).map_err(|error| error.with_source_context(source_name, source))?;
    Ok(format_program(&statements))
}

pub fn format_program(statements: &[Stmt]) -> String {
    let mut lines = Vec::new();
    for statement in statements {
        format_stmt(statement, 0, &mut lines);
    }
    let mut output = lines.join("\n");
    output.push('\n');
    output
}

fn format_stmt(statement: &Stmt, indent: usize, out: &mut Vec<String>) {
    let pad = " ".repeat(indent);
    match &statement.kind {
        StmtKind::Var {
            name,
            type_hint,
            initializer,
        } => {
            let mut line = format!("{pad}var {name}");
            if let Some(type_hint) = type_hint {
                line.push_str(&format!(": {}", type_hint.render()));
            }
            if let Some(initializer) = initializer {
                line.push_str(&format!(" = {}", format_expr(initializer)));
            }
            out.push(line);
        }
        StmtKind::Assign { target, op, value } => {
            let operator = match op {
                crate::ast::AssignOp::Set => "=",
                crate::ast::AssignOp::Add => "+=",
                crate::ast::AssignOp::Subtract => "-=",
                crate::ast::AssignOp::Multiply => "*=",
                crate::ast::AssignOp::Divide => "/=",
                crate::ast::AssignOp::Modulo => "%=",
            };
            out.push(format!(
                "{pad}{} {operator} {}",
                format_target(target),
                format_expr(value)
            ));
        }
        StmtKind::Expr(expr) => out.push(format!("{pad}{}", format_expr(expr))),
        StmtKind::If {
            branches,
            else_branch,
        } => {
            for (index, (condition, body)) in branches.iter().enumerate() {
                if index == 0 {
                    out.push(format!("{pad}if {} =>", format_expr(condition)));
                } else {
                    out.push(format!("{pad}elif {} =>", format_expr(condition)));
                }
                format_block(body, indent + 4, out);
            }
            if let Some(body) = else_branch {
                out.push(format!("{pad}else =>"));
                format_block(body, indent + 4, out);
            }
        }
        StmtKind::While { condition, body } => {
            out.push(format!("{pad}while {} =>", format_expr(condition)));
            format_block(body, indent + 4, out);
        }
        StmtKind::ForIn {
            name,
            iterable,
            body,
        } => {
            out.push(format!("{pad}for {name} in {} =>", format_expr(iterable)));
            format_block(body, indent + 4, out);
        }
        StmtKind::ForRange {
            name,
            start,
            end,
            step,
            body,
        } => {
            let mut line = format!(
                "{pad}for {name} from {} to {}",
                format_expr(start),
                format_expr(end)
            );
            if let Some(step) = step {
                line.push_str(&format!(" step {}", format_expr(step)));
            }
            line.push_str(" =>");
            out.push(line);
            format_block(body, indent + 4, out);
        }
        StmtKind::Task {
            name,
            params,
            return_type,
            body,
        } => {
            let params = params.iter().map(format_param).collect::<Vec<_>>().join(", ");
            let mut line = format!("{pad}task {name}({params})");
            if let Some(return_type) = return_type {
                line.push_str(&format!(" -> {}", return_type.render()));
            }
            line.push_str(" =>");
            out.push(line);
            format_block(body, indent + 4, out);
        }
        StmtKind::TypeDecl {
            flavor,
            name,
            fields,
            body,
        } => {
            let keyword = match flavor {
                crate::ast::TypeFlavor::Struct => "struct",
                crate::ast::TypeFlavor::Class => "class",
            };
            let fields = fields.iter().map(format_field).collect::<Vec<_>>().join(", ");
            out.push(format!("{pad}{keyword} {name}({fields}) =>"));
            format_block(body, indent + 4, out);
        }
        StmtKind::Use { spec, alias } => out.push(format!("{pad}use {spec} as {alias}")),
        StmtKind::Return(value) => {
            if let Some(value) = value {
                out.push(format!("{pad}give {}", format_expr(value)));
            } else {
                out.push(format!("{pad}give"));
            }
        }
        StmtKind::Break => out.push(format!("{pad}stop")),
        StmtKind::Continue => out.push(format!("{pad}skip")),
        StmtKind::Load(expr) => out.push(format!("{pad}load {}", format_expr(expr))),
    }
}

fn format_block(body: &[Stmt], indent: usize, out: &mut Vec<String>) {
    for statement in body {
        format_stmt(statement, indent, out);
    }
}

fn format_param(param: &Param) -> String {
    match &param.type_hint {
        Some(type_hint) => format!("{}: {}", param.name, type_hint.render()),
        None => param.name.clone(),
    }
}

fn format_field(field: &FieldDecl) -> String {
    match &field.type_hint {
        Some(type_hint) => format!("{}: {}", field.name, type_hint.render()),
        None => field.name.clone(),
    }
}

fn format_target(target: &AssignTarget) -> String {
    match target {
        AssignTarget::Variable(name) => name.clone(),
        AssignTarget::Index { object, index } => {
            format!("{}[{}]", format_expr(object), format_expr(index))
        }
        AssignTarget::Member { object, name } => format!("{}.{}", format_expr(object), name),
    }
}

fn format_expr(expr: &Expr) -> String {
    match &expr.kind {
        ExprKind::Literal(literal) => match literal {
            Literal::Number(value) => crate::value::format_number(*value),
            Literal::String(value) => format!("{value:?}"),
            Literal::Bool(true) => "yes".to_string(),
            Literal::Bool(false) => "no".to_string(),
            Literal::Nil => "nil".to_string(),
        },
        ExprKind::Variable(name) => name.clone(),
        ExprKind::List(items) => {
            let items = items.iter().map(format_expr).collect::<Vec<_>>().join(", ");
            format!("[{items}]")
        }
        ExprKind::Object(entries) => {
            let entries = entries
                .iter()
                .map(|(key, value)| format!("{key}: {}", format_expr(value)))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{{{entries}}}")
        }
        ExprKind::Unary { op, right } => match op {
            crate::ast::UnaryOp::Negate => format!("-{}", format_expr(right)),
            crate::ast::UnaryOp::Not => format!("not {}", format_expr(right)),
        },
        ExprKind::Binary { left, op, right } => {
            let operator = match op {
                crate::ast::BinaryOp::Add => "+",
                crate::ast::BinaryOp::Subtract => "-",
                crate::ast::BinaryOp::Multiply => "*",
                crate::ast::BinaryOp::Divide => "/",
                crate::ast::BinaryOp::Modulo => "%",
                crate::ast::BinaryOp::Power => "^",
                crate::ast::BinaryOp::Equal => "==",
                crate::ast::BinaryOp::NotEqual => "!=",
                crate::ast::BinaryOp::Greater => ">",
                crate::ast::BinaryOp::GreaterEqual => ">=",
                crate::ast::BinaryOp::Less => "<",
                crate::ast::BinaryOp::LessEqual => "<=",
                crate::ast::BinaryOp::And => "and",
                crate::ast::BinaryOp::Or => "or",
                crate::ast::BinaryOp::In => "in",
            };
            format!("({} {operator} {})", format_expr(left), format_expr(right))
        }
        ExprKind::Call { callee, args } => {
            let args = args.iter().map(format_expr).collect::<Vec<_>>().join(", ");
            format!("{}({args})", format_expr(callee))
        }
        ExprKind::Index { object, index } => format!("{}[{}]", format_expr(object), format_expr(index)),
        ExprKind::Member { object, name } => format!("{}.{}", format_expr(object), name),
    }
}
