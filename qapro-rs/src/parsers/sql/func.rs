use crate::parsers::sql::Expr;

#[derive(Debug, Clone, PartialEq)]
pub enum Func {
    Count(Expr),
    Upper(Expr),
    Lower(Expr),
    Ceil(Expr),
    Floor(Expr),
    Round(Expr),
}
