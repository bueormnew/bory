use crate::ast::{
    AssignOp, AssignTarget, BinaryOp, Expr, ExprKind, FieldDecl, Literal, Param, Stmt, StmtKind,
    TypeExpr, TypeFlavor, UnaryOp,
};
use crate::error::BoryError;
use crate::span::Span;
use crate::token::{Token, TokenKind};

pub fn parse(tokens: Vec<Token>) -> Result<Vec<Stmt>, BoryError> {
    Parser::new(tokens).parse_program()
}

struct Parser {
    tokens: Vec<Token>,
    current: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, current: 0 }
    }

    fn parse_program(&mut self) -> Result<Vec<Stmt>, BoryError> {
        let mut statements = Vec::new();
        self.consume_separators();

        while !self.is_at_end() {
            statements.push(self.statement()?);
            self.consume_separators();
        }

        Ok(statements)
    }

    fn statement(&mut self) -> Result<Stmt, BoryError> {
        if self.match_kind(TokenKind::Var) {
            return self.var_statement();
        }
        if self.match_kind(TokenKind::Use) {
            return self.use_statement();
        }
        if self.match_kind(TokenKind::Task) {
            return self.task_statement();
        }
        if self.match_kind(TokenKind::Struct) {
            return self.type_decl_statement(TypeFlavor::Struct);
        }
        if self.match_kind(TokenKind::Class) {
            return self.type_decl_statement(TypeFlavor::Class);
        }
        if self.match_kind(TokenKind::If) {
            return self.if_statement();
        }
        if self.match_kind(TokenKind::While) {
            return self.while_statement();
        }
        if self.match_kind(TokenKind::For) {
            return self.for_statement();
        }
        if self.match_kind(TokenKind::Give) {
            return self.return_statement();
        }
        if self.match_kind(TokenKind::Stop) {
            return Ok(Stmt::new(StmtKind::Break, self.previous().span));
        }
        if self.match_kind(TokenKind::Skip) {
            return Ok(Stmt::new(StmtKind::Continue, self.previous().span));
        }
        if self.match_kind(TokenKind::Load) {
            return self.load_statement();
        }

        self.expression_or_assignment_statement()
    }

    fn var_statement(&mut self) -> Result<Stmt, BoryError> {
        let name = self.consume(TokenKind::Identifier, "Expected a name after 'var'")?;
        let type_hint = self.parse_optional_type_hint()?;
        let initializer = if self.match_kind(TokenKind::Equal) {
            Some(self.expression()?)
        } else {
            None
        };
        Ok(Stmt::new(
            StmtKind::Var {
                name: name.lexeme,
                type_hint,
                initializer,
            },
            name.span,
        ))
    }

    fn use_statement(&mut self) -> Result<Stmt, BoryError> {
        let span = self.previous().span;
        let (spec, default_alias) = self.module_spec()?;
        let alias = if self.match_kind(TokenKind::As) {
            self.consume(TokenKind::Identifier, "Expected an alias after 'as'")?
                .lexeme
        } else {
            default_alias
        };

        Ok(Stmt::new(StmtKind::Use { spec, alias }, span))
    }

    fn module_spec(&mut self) -> Result<(String, String), BoryError> {
        if self.match_kind(TokenKind::String) {
            let spec = self.previous().lexeme.clone();
            let alias = default_path_alias(&spec);
            return Ok((spec, alias));
        }

        let mut parts = vec![
            self.consume(
                TokenKind::Identifier,
                "Expected a module name or quoted path after 'use'",
            )?
            .lexeme,
        ];

        while self.match_kind(TokenKind::Dot) {
            let segment = self.consume(
                TokenKind::Identifier,
                "Expected a module segment after '.'",
            )?;
            parts.push(segment.lexeme);
        }

        let alias = parts
            .last()
            .cloned()
            .ok_or_else(|| BoryError::parse("Expected a module name", self.peek().span))?;
        Ok((parts.join("."), alias))
    }

    fn task_statement(&mut self) -> Result<Stmt, BoryError> {
        let name = self.consume(TokenKind::Identifier, "Expected a task name after 'task'")?;
        self.consume(TokenKind::LeftParen, "Expected '(' after the task name")?;

        let mut params = Vec::new();
        if !self.check(TokenKind::RightParen) {
            loop {
                let parameter_name = self.consume(
                    TokenKind::Identifier,
                    "Expected a parameter name inside the task",
                )?;
                let type_hint = self.parse_optional_type_hint()?;
                params.push(Param {
                    name: parameter_name.lexeme,
                    type_hint,
                });
                if !self.match_kind(TokenKind::Comma) {
                    break;
                }
            }
        }

        self.consume(
            TokenKind::RightParen,
            "Expected ')' after the parameter list",
        )?;
        let return_type = if self.match_kind(TokenKind::Arrow) {
            Some(self.type_expr()?)
        } else {
            None
        };
        self.consume(
            TokenKind::FatArrow,
            "Expected '=>' after the task signature",
        )?;
        let (body, indent_closed) = self.block(&[TokenKind::End])?;
        if !indent_closed {
            self.consume(TokenKind::End, "Expected 'end' to close the task")?;
        } else {
            self.match_kind(TokenKind::End);
        }

        Ok(Stmt::new(
            StmtKind::Task {
                name: name.lexeme,
                params,
                return_type,
                body,
            },
            name.span,
        ))
    }

    fn type_decl_statement(&mut self, flavor: TypeFlavor) -> Result<Stmt, BoryError> {
        let name = self.consume(
            TokenKind::Identifier,
            "Expected a type name after 'struct' or 'class'",
        )?;
        self.consume(TokenKind::LeftParen, "Expected '(' after the type name")?;

        let mut fields = Vec::new();
        if !self.check(TokenKind::RightParen) {
            loop {
                let field_name = self.consume(
                    TokenKind::Identifier,
                    "Expected a field name inside the type declaration",
                )?;
                let type_hint = self.parse_optional_type_hint()?;
                fields.push(FieldDecl {
                    name: field_name.lexeme,
                    type_hint,
                });
                if !self.match_kind(TokenKind::Comma) {
                    break;
                }
            }
        }

        self.consume(TokenKind::RightParen, "Expected ')' after the field list")?;
        self.consume(
            TokenKind::FatArrow,
            "Expected '=>' after the type signature",
        )?;
        let (body, indent_closed) = self.block(&[TokenKind::End])?;
        if !indent_closed {
            self.consume(TokenKind::End, "Expected 'end' to close the type")?;
        } else {
            self.match_kind(TokenKind::End);
        }

        Ok(Stmt::new(
            StmtKind::TypeDecl {
                flavor,
                name: name.lexeme,
                fields,
                body,
            },
            name.span,
        ))
    }

    fn if_statement(&mut self) -> Result<Stmt, BoryError> {
        let start = self.previous().span;
        let first_condition = self.expression()?;
        self.consume(TokenKind::FatArrow, "Expected '=>' after the if condition")?;

        let (first_body, first_indent) =
            self.block(&[TokenKind::Elif, TokenKind::Else, TokenKind::End])?;
        let mut branches = vec![(first_condition, first_body)];
        let mut used_indent = first_indent;

        while self.match_kind(TokenKind::Elif) {
            let condition = self.expression()?;
            self.consume(
                TokenKind::FatArrow,
                "Expected '=>' after the elif condition",
            )?;
            let (body, indent_closed) =
                self.block(&[TokenKind::Elif, TokenKind::Else, TokenKind::End])?;
            used_indent |= indent_closed;
            branches.push((condition, body));
        }

        let else_branch = if self.match_kind(TokenKind::Else) {
            self.consume(TokenKind::FatArrow, "Expected '=>' after 'else'")?;
            let (body, indent_closed) = self.block(&[TokenKind::End])?;
            used_indent |= indent_closed;
            Some(body)
        } else {
            None
        };

        if !used_indent {
            self.consume(TokenKind::End, "Expected 'end' to close the if block")?;
        } else {
            self.match_kind(TokenKind::End);
        }

        Ok(Stmt::new(
            StmtKind::If {
                branches,
                else_branch,
            },
            start,
        ))
    }

    fn while_statement(&mut self) -> Result<Stmt, BoryError> {
        let start = self.previous().span;
        let condition = self.expression()?;
        self.consume(
            TokenKind::FatArrow,
            "Expected '=>' after the while condition",
        )?;
        let (body, indent_closed) = self.block(&[TokenKind::End])?;
        if !indent_closed {
            self.consume(TokenKind::End, "Expected 'end' to close the while block")?;
        } else {
            self.match_kind(TokenKind::End);
        }

        Ok(Stmt::new(StmtKind::While { condition, body }, start))
    }

    fn for_statement(&mut self) -> Result<Stmt, BoryError> {
        let start = self.previous().span;
        let name = self.consume(
            TokenKind::Identifier,
            "Expected a loop variable name after 'for'",
        )?;

        if self.match_kind(TokenKind::In) {
            let iterable = self.expression()?;
            self.consume(
                TokenKind::FatArrow,
                "Expected '=>' after the iterable expression",
            )?;
            let (body, indent_closed) = self.block(&[TokenKind::End])?;
            if !indent_closed {
                self.consume(TokenKind::End, "Expected 'end' to close the for block")?;
            } else {
                self.match_kind(TokenKind::End);
            }
            Ok(Stmt::new(
                StmtKind::ForIn {
                    name: name.lexeme,
                    iterable,
                    body,
                },
                start,
            ))
        } else if self.match_kind(TokenKind::From) {
            let start_expr = self.expression()?;
            self.consume(TokenKind::To, "Expected 'to' in the for range syntax")?;
            let end_expr = self.expression()?;
            let step = if self.match_kind(TokenKind::Step) {
                Some(self.expression()?)
            } else {
                None
            };
            self.consume(
                TokenKind::FatArrow,
                "Expected '=>' after the for range expression",
            )?;
            let (body, indent_closed) = self.block(&[TokenKind::End])?;
            if !indent_closed {
                self.consume(TokenKind::End, "Expected 'end' to close the for block")?;
            } else {
                self.match_kind(TokenKind::End);
            }
            Ok(Stmt::new(
                StmtKind::ForRange {
                    name: name.lexeme,
                    start: start_expr,
                    end: end_expr,
                    step,
                    body,
                },
                start,
            ))
        } else {
            Err(BoryError::parse(
                "Expected 'in' or 'from' after the loop variable",
                self.peek().span,
            ))
        }
    }

    fn return_statement(&mut self) -> Result<Stmt, BoryError> {
        let span = self.previous().span;
        if self.at_statement_boundary() {
            Ok(Stmt::new(StmtKind::Return(None), span))
        } else {
            Ok(Stmt::new(StmtKind::Return(Some(self.expression()?)), span))
        }
    }

    fn load_statement(&mut self) -> Result<Stmt, BoryError> {
        let span = self.previous().span;
        let expr = self.expression()?;
        Ok(Stmt::new(StmtKind::Load(expr), span))
    }

    fn expression_or_assignment_statement(&mut self) -> Result<Stmt, BoryError> {
        let expr = self.expression()?;
        if let Some(op) = self.match_assignment_op() {
            let value = self.expression()?;
            let span = expr.span;
            let target = assign_target(expr)?;
            Ok(Stmt::new(StmtKind::Assign { target, op, value }, span))
        } else {
            Ok(Stmt::new(StmtKind::Expr(expr.clone()), expr.span))
        }
    }

    fn block(&mut self, terminators: &[TokenKind]) -> Result<(Vec<Stmt>, bool), BoryError> {
        self.consume_separators();

        if self.match_kind(TokenKind::Indent) {
            let mut statements = Vec::new();
            self.consume_separators();
            while !self.is_at_end() && !self.check(TokenKind::Dedent) {
                statements.push(self.statement()?);
                self.consume_separators();
            }
            self.consume(TokenKind::Dedent, "Expected the indentation block to close")?;
            return Ok((statements, true));
        }

        let mut statements = Vec::new();
        while !self.is_at_end() && !self.check_any(terminators) && !self.check(TokenKind::Dedent) {
            statements.push(self.statement()?);
            self.consume_separators();
        }

        Ok((statements, false))
    }

    fn parse_optional_type_hint(&mut self) -> Result<Option<TypeExpr>, BoryError> {
        if self.match_kind(TokenKind::Colon) {
            Ok(Some(self.type_expr()?))
        } else {
            Ok(None)
        }
    }

    fn type_expr(&mut self) -> Result<TypeExpr, BoryError> {
        let name = self.consume(TokenKind::Identifier, "Expected a type name")?;
        let mut args = Vec::new();
        if self.match_kind(TokenKind::LeftBracket) {
            loop {
                args.push(self.type_expr()?);
                if !self.match_kind(TokenKind::Comma) {
                    break;
                }
            }
            self.consume(
                TokenKind::RightBracket,
                "Expected ']' to close the type arguments",
            )?;
        }
        Ok(TypeExpr {
            name: name.lexeme,
            args,
        })
    }

    fn expression(&mut self) -> Result<Expr, BoryError> {
        self.or()
    }

    fn or(&mut self) -> Result<Expr, BoryError> {
        let mut expr = self.and()?;
        while self.match_kind(TokenKind::Or) {
            let right = self.and()?;
            let span = expr.span;
            expr = Expr::new(
                ExprKind::Binary {
                    left: Box::new(expr),
                    op: BinaryOp::Or,
                    right: Box::new(right),
                },
                span,
            );
        }
        Ok(expr)
    }

    fn and(&mut self) -> Result<Expr, BoryError> {
        let mut expr = self.equality()?;
        while self.match_kind(TokenKind::And) {
            let right = self.equality()?;
            let span = expr.span;
            expr = Expr::new(
                ExprKind::Binary {
                    left: Box::new(expr),
                    op: BinaryOp::And,
                    right: Box::new(right),
                },
                span,
            );
        }
        Ok(expr)
    }

    fn equality(&mut self) -> Result<Expr, BoryError> {
        let mut expr = self.comparison()?;
        while self.match_kinds(&[TokenKind::EqualEqual, TokenKind::BangEqual]) {
            let op = match self.previous().kind {
                TokenKind::EqualEqual => BinaryOp::Equal,
                TokenKind::BangEqual => BinaryOp::NotEqual,
                _ => unreachable!(),
            };
            let right = self.comparison()?;
            let span = expr.span;
            expr = Expr::new(
                ExprKind::Binary {
                    left: Box::new(expr),
                    op,
                    right: Box::new(right),
                },
                span,
            );
        }
        Ok(expr)
    }

    fn comparison(&mut self) -> Result<Expr, BoryError> {
        let mut expr = self.term()?;
        while self.match_kinds(&[
            TokenKind::Greater,
            TokenKind::GreaterEqual,
            TokenKind::Less,
            TokenKind::LessEqual,
            TokenKind::In,
        ]) {
            let op = match self.previous().kind {
                TokenKind::Greater => BinaryOp::Greater,
                TokenKind::GreaterEqual => BinaryOp::GreaterEqual,
                TokenKind::Less => BinaryOp::Less,
                TokenKind::LessEqual => BinaryOp::LessEqual,
                TokenKind::In => BinaryOp::In,
                _ => unreachable!(),
            };
            let right = self.term()?;
            let span = expr.span;
            expr = Expr::new(
                ExprKind::Binary {
                    left: Box::new(expr),
                    op,
                    right: Box::new(right),
                },
                span,
            );
        }
        Ok(expr)
    }

    fn term(&mut self) -> Result<Expr, BoryError> {
        let mut expr = self.factor()?;
        while self.match_kinds(&[TokenKind::Plus, TokenKind::Minus]) {
            let op = match self.previous().kind {
                TokenKind::Plus => BinaryOp::Add,
                TokenKind::Minus => BinaryOp::Subtract,
                _ => unreachable!(),
            };
            let right = self.factor()?;
            let span = expr.span;
            expr = Expr::new(
                ExprKind::Binary {
                    left: Box::new(expr),
                    op,
                    right: Box::new(right),
                },
                span,
            );
        }
        Ok(expr)
    }

    fn factor(&mut self) -> Result<Expr, BoryError> {
        let mut expr = self.power()?;
        while self.match_kinds(&[TokenKind::Star, TokenKind::Slash, TokenKind::Percent]) {
            let op = match self.previous().kind {
                TokenKind::Star => BinaryOp::Multiply,
                TokenKind::Slash => BinaryOp::Divide,
                TokenKind::Percent => BinaryOp::Modulo,
                _ => unreachable!(),
            };
            let right = self.power()?;
            let span = expr.span;
            expr = Expr::new(
                ExprKind::Binary {
                    left: Box::new(expr),
                    op,
                    right: Box::new(right),
                },
                span,
            );
        }
        Ok(expr)
    }

    fn power(&mut self) -> Result<Expr, BoryError> {
        let expr = self.unary()?;
        if self.match_kind(TokenKind::Caret) {
            let right = self.power()?;
            let span = expr.span;
            Ok(Expr::new(
                ExprKind::Binary {
                    left: Box::new(expr),
                    op: BinaryOp::Power,
                    right: Box::new(right),
                },
                span,
            ))
        } else {
            Ok(expr)
        }
    }

    fn unary(&mut self) -> Result<Expr, BoryError> {
        if self.match_kind(TokenKind::Not) {
            let span = self.previous().span;
            let right = self.unary()?;
            return Ok(Expr::new(
                ExprKind::Unary {
                    op: UnaryOp::Not,
                    right: Box::new(right),
                },
                span,
            ));
        }

        if self.match_kind(TokenKind::Minus) {
            let span = self.previous().span;
            let right = self.unary()?;
            return Ok(Expr::new(
                ExprKind::Unary {
                    op: UnaryOp::Negate,
                    right: Box::new(right),
                },
                span,
            ));
        }

        self.call()
    }

    fn call(&mut self) -> Result<Expr, BoryError> {
        let mut expr = self.primary()?;

        loop {
            if self.match_kind(TokenKind::LeftParen) {
                let mut args = Vec::new();
                self.consume_soft_separators();
                if !self.check(TokenKind::RightParen) {
                    loop {
                        args.push(self.expression()?);
                        self.consume_soft_separators();
                        if !self.match_kind(TokenKind::Comma) {
                            break;
                        }
                        self.consume_soft_separators();
                    }
                }
                self.consume(TokenKind::RightParen, "Expected ')' after the arguments")?;
                let span = expr.span;
                expr = Expr::new(
                    ExprKind::Call {
                        callee: Box::new(expr),
                        args,
                    },
                    span,
                );
            } else if self.match_kind(TokenKind::LeftBracket) {
                self.consume_soft_separators();
                let index = self.expression()?;
                self.consume_soft_separators();
                self.consume(TokenKind::RightBracket, "Expected ']' after the index")?;
                let span = expr.span;
                expr = Expr::new(
                    ExprKind::Index {
                        object: Box::new(expr),
                        index: Box::new(index),
                    },
                    span,
                );
            } else if self.match_kind(TokenKind::Dot) {
                let name =
                    self.consume(TokenKind::Identifier, "Expected a member name after '.'")?;
                let span = expr.span;
                expr = Expr::new(
                    ExprKind::Member {
                        object: Box::new(expr),
                        name: name.lexeme,
                    },
                    span,
                );
            } else {
                break;
            }
        }

        Ok(expr)
    }

    fn primary(&mut self) -> Result<Expr, BoryError> {
        let token = self.advance().clone();
        match token.kind {
            TokenKind::Number => {
                let number = token
                    .lexeme
                    .parse::<f64>()
                    .map_err(|_| BoryError::parse("Invalid number literal", token.span))?;
                Ok(Expr::new(
                    ExprKind::Literal(Literal::Number(number)),
                    token.span,
                ))
            }
            TokenKind::String => Ok(Expr::new(
                ExprKind::Literal(Literal::String(token.lexeme)),
                token.span,
            )),
            TokenKind::Yes => Ok(Expr::new(
                ExprKind::Literal(Literal::Bool(true)),
                token.span,
            )),
            TokenKind::No => Ok(Expr::new(
                ExprKind::Literal(Literal::Bool(false)),
                token.span,
            )),
            TokenKind::Nil => Ok(Expr::new(ExprKind::Literal(Literal::Nil), token.span)),
            TokenKind::Identifier => Ok(Expr::new(ExprKind::Variable(token.lexeme), token.span)),
            TokenKind::LeftParen => {
                self.consume_soft_separators();
                let expr = self.expression()?;
                self.consume_soft_separators();
                self.consume(TokenKind::RightParen, "Expected ')' after the expression")?;
                Ok(expr)
            }
            TokenKind::LeftBracket => self.list_literal(token.span),
            TokenKind::LeftBrace => self.object_literal(token.span),
            _ => Err(BoryError::parse(
                format!("Did not expect '{}' here", token.lexeme),
                token.span,
            )),
        }
    }

    fn list_literal(&mut self, span: Span) -> Result<Expr, BoryError> {
        let mut items = Vec::new();
        self.consume_soft_separators();
        if !self.check(TokenKind::RightBracket) {
            loop {
                items.push(self.expression()?);
                self.consume_soft_separators();
                if !self.match_kind(TokenKind::Comma) {
                    break;
                }
                self.consume_soft_separators();
            }
        }
        self.consume(TokenKind::RightBracket, "Expected ']' to close the list")?;
        Ok(Expr::new(ExprKind::List(items), span))
    }

    fn object_literal(&mut self, span: Span) -> Result<Expr, BoryError> {
        let mut items = Vec::new();
        self.consume_soft_separators();
        if !self.check(TokenKind::RightBrace) {
            loop {
                let key = if self.match_kind(TokenKind::Identifier) || self.match_kind(TokenKind::String)
                {
                    self.previous().lexeme.clone()
                } else {
                    return Err(BoryError::parse(
                        "Object keys must be strings or identifiers",
                        self.peek().span,
                    ));
                };
                self.consume(TokenKind::Colon, "Expected ':' after the object key")?;
                let value = self.expression()?;
                items.push((key, value));
                self.consume_soft_separators();
                if !self.match_kind(TokenKind::Comma) {
                    break;
                }
                self.consume_soft_separators();
            }
        }
        self.consume(TokenKind::RightBrace, "Expected '}' to close the object")?;
        Ok(Expr::new(ExprKind::Object(items), span))
    }

    fn consume_separators(&mut self) {
        while self.match_kinds(&[TokenKind::Newline, TokenKind::Semicolon]) {}
    }

    fn consume_soft_separators(&mut self) {
        while self.match_kinds(&[
            TokenKind::Newline,
            TokenKind::Semicolon,
            TokenKind::Indent,
            TokenKind::Dedent,
        ]) {}
    }

    fn consume(&mut self, kind: TokenKind, message: &str) -> Result<Token, BoryError> {
        if self.check(kind) {
            Ok(self.advance().clone())
        } else {
            Err(BoryError::parse(message, self.peek().span))
        }
    }

    fn at_statement_boundary(&self) -> bool {
        self.check_any(&[
            TokenKind::Newline,
            TokenKind::Semicolon,
            TokenKind::Eof,
            TokenKind::End,
            TokenKind::Else,
            TokenKind::Elif,
            TokenKind::Dedent,
        ])
    }

    fn match_assignment_op(&mut self) -> Option<AssignOp> {
        if self.match_kind(TokenKind::Equal) {
            Some(AssignOp::Set)
        } else if self.match_kind(TokenKind::PlusEqual) {
            Some(AssignOp::Add)
        } else if self.match_kind(TokenKind::MinusEqual) {
            Some(AssignOp::Subtract)
        } else if self.match_kind(TokenKind::StarEqual) {
            Some(AssignOp::Multiply)
        } else if self.match_kind(TokenKind::SlashEqual) {
            Some(AssignOp::Divide)
        } else if self.match_kind(TokenKind::PercentEqual) {
            Some(AssignOp::Modulo)
        } else {
            None
        }
    }

    fn match_kind(&mut self, kind: TokenKind) -> bool {
        if self.check(kind) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn match_kinds(&mut self, kinds: &[TokenKind]) -> bool {
        for kind in kinds {
            if self.check(*kind) {
                self.advance();
                return true;
            }
        }
        false
    }

    fn check(&self, kind: TokenKind) -> bool {
        (!self.is_at_end() && self.peek().kind == kind)
            || (kind == TokenKind::Eof && self.peek().kind == TokenKind::Eof)
    }

    fn check_any(&self, kinds: &[TokenKind]) -> bool {
        kinds.iter().any(|kind| self.check(*kind))
    }

    fn advance(&mut self) -> &Token {
        if !self.is_at_end() {
            self.current += 1;
        }
        self.previous()
    }

    fn is_at_end(&self) -> bool {
        self.peek().kind == TokenKind::Eof
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.current]
    }

    fn previous(&self) -> &Token {
        &self.tokens[self.current.saturating_sub(1)]
    }
}

fn assign_target(expr: Expr) -> Result<AssignTarget, BoryError> {
    match expr.kind {
        ExprKind::Variable(name) => Ok(AssignTarget::Variable(name)),
        ExprKind::Index { object, index } => Ok(AssignTarget::Index {
            object: *object,
            index: *index,
        }),
        ExprKind::Member { object, name } => Ok(AssignTarget::Member {
            object: *object,
            name,
        }),
        _ => Err(BoryError::parse(
            "The left side of an assignment must be a variable, index access, or member access",
            expr.span,
        )),
    }
}

fn default_path_alias(spec: &str) -> String {
    let trimmed = spec.trim_end_matches(['/', '\\']);
    let name = trimmed.rsplit(['/', '\\']).next().unwrap_or("module");
    let stem = name.split('.').next().unwrap_or("module");
    stem.to_string()
}
