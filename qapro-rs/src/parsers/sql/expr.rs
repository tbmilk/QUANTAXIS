use std::collections::HashSet;
use std::str::FromStr;

use collect_mac::collect;
use ordered_float::OrderedFloat;

use crate::parsers::parser;
use crate::parsers::sql::Env;
use crate::parsers::sql::Func;
use crate::parsers::sql::Selector;
use crate::parsers::sql::Sql;
use crate::parsers::value::PqlValue;

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Star,
    Selector(Selector),
    Value(PqlValue),
    Func(Box<Func>),
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Div(Box<Expr>, Box<Expr>),
    Rem(Box<Expr>, Box<Expr>),
    Exp(Box<Expr>, Box<Expr>),
    Sql(Sql),
}

impl Default for Expr {
    fn default() -> Self {
        Self::Value(PqlValue::default())
    }
}

impl From<i64> for Expr {
    fn from(i: i64) -> Self {
        Self::Value(PqlValue::Int(i))
    }
}

impl From<f64> for Expr {
    fn from(f: f64) -> Self {
        Self::Value(PqlValue::Float(OrderedFloat(f)))
    }
}

impl From<&str> for Expr {
    fn from(s: &str) -> Self {
        Self::Value(PqlValue::from(s))
    }
}

impl From<Selector> for Expr {
    fn from(selector: Selector) -> Self {
        Self::Selector(selector)
    }
}

impl From<PqlValue> for Expr {
    fn from(value: PqlValue) -> Self {
        Self::Value(value)
    }
}

impl From<Expr> for String {
    fn from(expr: Expr) -> Self {
        match expr {
            Expr::Selector(selector) => selector.to_string(),
            Expr::Value(value) => value.to_json().expect("to json"),
            Expr::Star => "*".to_string(),
            Expr::Sql(_) => "(subquery)".to_string(),
            Expr::Func(f) => match *f {
                crate::parsers::sql::Func::Count(e) => format!("COUNT({})", String::from(e)),
                crate::parsers::sql::Func::Upper(e) => format!("UPPER({})", String::from(e)),
                crate::parsers::sql::Func::Lower(e) => format!("LOWER({})", String::from(e)),
                crate::parsers::sql::Func::Ceil(e) => format!("CEIL({})", String::from(e)),
                crate::parsers::sql::Func::Floor(e) => format!("FLOOR({})", String::from(e)),
                crate::parsers::sql::Func::Round(e) => format!("ROUND({})", String::from(e)),
            },
            Expr::Add(l, r) => format!("({} + {})", String::from(*l), String::from(*r)),
            Expr::Sub(l, r) => format!("({} - {})", String::from(*l), String::from(*r)),
            Expr::Mul(l, r) => format!("({} * {})", String::from(*l), String::from(*r)),
            Expr::Div(l, r) => format!("({} / {})", String::from(*l), String::from(*r)),
            Expr::Rem(l, r) => format!("({} % {})", String::from(*l), String::from(*r)),
            Expr::Exp(l, r) => format!("({} ^ {})", String::from(*l), String::from(*r)),
        }
    }
}

impl FromStr for Expr {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> anyhow::Result<Self> {
        match parser::expressions::parse_expr(s) {
            Ok((_, expr)) => Ok(expr),
            Err(nom::Err::Error(err)) => {
                eprint!("{}", err);
                anyhow::bail!("failed")
            }
            Err(e) => anyhow::bail!("parse error: {:?}", e),
        }
    }
}

impl Expr {
    pub fn to_string(self) -> String {
        String::from(self)
    }

    pub fn as_path(&self) -> Option<Selector> {
        match self {
            Expr::Selector(path) => Some(path.to_owned()),
            _ => None,
        }
    }

    pub fn expand_fullpath(&self, env: &Env) -> Self {
        match self {
            Self::Selector(path) => Self::Selector(path.expand_fullpath2(&env)),
            Expr::Value(_) => self.to_owned(),
            Expr::Star => Expr::Star,
            Expr::Func(_) => self.to_owned(),
            Self::Add(left, right) => Self::Add(
                Box::new((*left).expand_fullpath(&env)),
                Box::new((*right).expand_fullpath(&env)),
            ),
            Self::Sub(left, right) => Self::Sub(
                Box::new((*left).expand_fullpath(&env)),
                Box::new((*right).expand_fullpath(&env)),
            ),
            Self::Mul(left, right) => Self::Mul(
                Box::new((*left).expand_fullpath(&env)),
                Box::new((*right).expand_fullpath(&env)),
            ),
            Self::Div(left, right) => Self::Div(
                Box::new((*left).expand_fullpath(&env)),
                Box::new((*right).expand_fullpath(&env)),
            ),
            Self::Rem(left, right) => Self::Rem(
                Box::new((*left).expand_fullpath(&env)),
                Box::new((*right).expand_fullpath(&env)),
            ),
            Self::Exp(left, right) => Self::Exp(
                Box::new((*left).expand_fullpath(&env)),
                Box::new((*right).expand_fullpath(&env)),
            ),
            Expr::Sql(_) => self.to_owned(),
        }
    }

    pub fn eval(self, env: &Env) -> PqlValue {
        match self.to_owned() {
            Self::Value(value) => value,
            Self::Selector(selector) => selector.evaluate(&env),
            Self::Star => PqlValue::default(),
            Self::Func(f) => {
                use crate::parsers::sql::Func;
                use ordered_float::OrderedFloat;
                match *f {
                    Func::Count(_) => PqlValue::default(),
                    Func::Upper(e) => match e.eval(env) {
                        PqlValue::Str(s) => PqlValue::Str(s.to_uppercase()),
                        v => v,
                    },
                    Func::Lower(e) => match e.eval(env) {
                        PqlValue::Str(s) => PqlValue::Str(s.to_lowercase()),
                        v => v,
                    },
                    Func::Ceil(e) => match e.eval(env) {
                        PqlValue::Float(OrderedFloat(f)) => PqlValue::Int(f.ceil() as i64),
                        PqlValue::Int(i) => PqlValue::Int(i),
                        v => v,
                    },
                    Func::Floor(e) => match e.eval(env) {
                        PqlValue::Float(OrderedFloat(f)) => PqlValue::Int(f.floor() as i64),
                        PqlValue::Int(i) => PqlValue::Int(i),
                        v => v,
                    },
                    Func::Round(e) => match e.eval(env) {
                        PqlValue::Float(OrderedFloat(f)) => PqlValue::Int(f.round() as i64),
                        PqlValue::Int(i) => PqlValue::Int(i),
                        v => v,
                    },
                }
            }
            Self::Sql(_) => PqlValue::default(),
            Self::Add(expr1, expr2) => (*expr1).eval(&env) + (*expr2).eval(&env),
            Self::Sub(expr1, expr2) => (*expr1).eval(&env) - (*expr2).eval(&env),
            Self::Mul(expr1, expr2) => (*expr1).eval(&env) * (*expr2).eval(&env),
            Self::Div(expr1, expr2) => (*expr1).eval(&env) / (*expr2).eval(&env),
            Self::Rem(expr1, expr2) => (*expr1).eval(&env) % (*expr2).eval(&env),
            Self::Exp(expr1, expr2) => (*expr1).eval(&env).powf((*expr2).eval(&env)),
        }
    }

    pub fn source_field_name_set(&self, env: &Env) -> HashSet<String> {
        match self.to_owned() {
            Expr::Selector(selector) => {
                collect! {
                    as HashSet<String>:
                    selector.expand_fullpath2(&env).to_string()
                }
            }
            Expr::Add(expr1, expr2) => {
                let a = expr1.source_field_name_set(&env);
                let b = expr2.source_field_name_set(&env);
                a.union(&b).map(String::from).collect::<HashSet<_>>()
            }
            Expr::Sub(expr1, expr2) => {
                let a = expr1.source_field_name_set(&env);
                let b = expr2.source_field_name_set(&env);
                a.union(&b).map(String::from).collect::<HashSet<_>>()
            }
            Expr::Mul(expr1, expr2) => {
                let a = expr1.source_field_name_set(&env);
                let b = expr2.source_field_name_set(&env);
                a.union(&b).map(String::from).collect::<HashSet<_>>()
            }
            Expr::Div(expr1, expr2) => {
                let a = expr1.source_field_name_set(&env);
                let b = expr2.source_field_name_set(&env);
                a.union(&b).map(String::from).collect::<HashSet<_>>()
            }
            Expr::Rem(expr1, expr2) => {
                let a = expr1.source_field_name_set(&env);
                let b = expr2.source_field_name_set(&env);
                a.union(&b).map(String::from).collect::<HashSet<_>>()
            }
            Expr::Exp(expr1, expr2) => {
                let a = expr1.source_field_name_set(&env);
                let b = expr2.source_field_name_set(&env);
                a.union(&b).map(String::from).collect::<HashSet<_>>()
            }
            _ => HashSet::new(),
        }
    }

    pub fn to_path(&self) -> Option<Selector> {
        match self.to_owned() {
            Self::Value(_value) => None,
            Self::Selector(selector) => Some(selector),
            Self::Star => None,
            Self::Func(_) => None,
            Self::Sql(_) => None,
            Self::Add(expr1, expr2) => match (expr1.to_path(), expr2.to_path()) {
                (Some(s1), Some(s2)) => Some(s1.intersect(&s2)),
                (Some(s1), _) => Some(s1),
                (_, Some(s2)) => Some(s2),
                _ => None,
            },
            Self::Sub(expr1, expr2) => match (expr1.to_path(), expr2.to_path()) {
                (Some(s1), Some(s2)) => Some(s1.intersect(&s2)),
                (Some(s1), _) => Some(s1),
                (_, Some(s2)) => Some(s2),
                _ => None,
            },
            Self::Mul(expr1, expr2) => match (expr1.to_path(), expr2.to_path()) {
                (Some(s1), Some(s2)) => Some(s1.intersect(&s2)),
                (Some(s1), _) => Some(s1),
                (_, Some(s2)) => Some(s2),
                _ => None,
            },
            Self::Div(expr1, expr2) => match (expr1.to_path(), expr2.to_path()) {
                (Some(s1), Some(s2)) => Some(s1.intersect(&s2)),
                (Some(s1), _) => Some(s1),
                (_, Some(s2)) => Some(s2),
                _ => None,
            },
            Self::Rem(expr1, expr2) => match (expr1.to_path(), expr2.to_path()) {
                (Some(s1), Some(s2)) => Some(s1.intersect(&s2)),
                (Some(s1), _) => Some(s1),
                (_, Some(s2)) => Some(s2),
                _ => None,
            },
            Self::Exp(expr1, expr2) => match (expr1.to_path(), expr2.to_path()) {
                (Some(s1), Some(s2)) => Some(s1.intersect(&s2)),
                (Some(s1), _) => Some(s1),
                (_, Some(s2)) => Some(s2),
                _ => None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use crate::parsers::parser;
    use crate::parsers::planner::LogicalPlan;
    use crate::parsers::sql::Env;
    use crate::parsers::sql::Expr;
    use crate::parsers::sql::Selector;
    use crate::parsers::sql::Sql;
    use crate::parsers::value::PqlValue;

    #[test]
    fn test_expr_mul() -> anyhow::Result<()> {
        let mut sql = Sql::default();
        sql.select_clause = parser::clauses::select(r#"SELECT 4 * a AS aa"#)?.1;
        sql.from_clause = parser::clauses::from("FROM 3 as a")?.1;
        let plan = LogicalPlan::from(sql);

        let mut env = Env::default();
        let res = plan.execute(&mut env);

        assert_eq!(res, PqlValue::from_str(r#"[{ "aa": 12 }]"#)?);

        Ok(())
    }
    #[test]
    fn test_expr_calc() -> anyhow::Result<()> {
        let mut sql = Sql::default();
        sql.select_clause = parser::clauses::calc(r#"CALC 4 * a AS aa"#)?.1;
        println!("{:#?}", sql.select_clause);
        sql.from_clause = parser::clauses::from("FROM 3 as a")?.1;
        let plan = LogicalPlan::from(sql);

        let mut env = Env::default();
        let res = plan.execute(&mut env);

        assert_eq!(res, PqlValue::from_str(r#"[{ "aa": 12 }]"#)?);

        Ok(())
    }

    #[test]
    fn test_get_common_path() -> anyhow::Result<()> {
        let expr = Expr::from_str("a.b.c + a.b.d")?;

        let res = expr.to_path();

        assert_eq!(res, Some(Selector::from(r#"a.b"#)));

        Ok(())
    }

    #[test]
    fn test_get_common_math() -> anyhow::Result<()> {
        let expr = Expr::from_str("a.b * 2")?;

        let res = expr.to_path();

        assert_eq!(res, Some(Selector::from(r#"a.b"#)));

        Ok(())
    }
}
