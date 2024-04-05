use std::sync::OnceLock;

use super::*;
use crate::type_::{bool, HasType, Type};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypedExpr {
    Int {
        location: SrcSpan,
        typ: Arc<Type>,
        value: EcoString,
    },

    Float {
        location: SrcSpan,
        typ: Arc<Type>,
        value: EcoString,
    },

    String {
        location: SrcSpan,
        typ: Arc<Type>,
        value: EcoString,
    },

    Block {
        location: SrcSpan,
        statements: Vec1<TypedStatement>,
    },

    /// A chain of pipe expressions.
    /// By this point the type checker has expanded it into a series of
    /// assignments and function calls, but we still have a Pipeline AST node as
    /// even though it is identical to `Block` we want to use different
    /// locations when showing it in error messages, etc.
    Pipeline {
        location: SrcSpan,
        assignments: Vec<TypedAssignment>,
        finally: Box<Self>,
    },

    Var {
        location: SrcSpan,
        constructor: ValueConstructor,
        name: EcoString,
    },

    Fn {
        location: SrcSpan,
        typ: Arc<Type>,
        is_capture: bool,
        args: Vec<Arg<Arc<Type>>>,
        body: Vec1<Statement<Arc<Type>, Self>>,
        return_annotation: Option<TypeAst>,
    },

    List {
        location: SrcSpan,
        typ: Arc<Type>,
        elements: Vec<Self>,
        tail: Option<Box<Self>>,
    },

    Call {
        location: SrcSpan,
        typ: Arc<Type>,
        fun: Box<Self>,
        args: Vec<CallArg<Self>>,
    },

    BinOp {
        location: SrcSpan,
        typ: Arc<Type>,
        name: BinOp,
        left: Box<Self>,
        right: Box<Self>,
    },

    Case {
        location: SrcSpan,
        typ: Arc<Type>,
        subjects: Vec<Self>,
        clauses: Vec<Clause<Self, Arc<Type>, EcoString>>,
    },

    RecordAccess {
        location: SrcSpan,
        typ: Arc<Type>,
        label: EcoString,
        index: u64,
        record: Box<Self>,
    },

    ModuleSelect {
        location: SrcSpan,
        typ: Arc<Type>,
        label: EcoString,
        module_name: EcoString,
        module_alias: EcoString,
        constructor: ModuleValueConstructor,
    },

    Tuple {
        location: SrcSpan,
        typ: Arc<Type>,
        elems: Vec<Self>,
    },

    TupleIndex {
        location: SrcSpan,
        typ: Arc<Type>,
        index: u64,
        tuple: Box<Self>,
    },

    Todo {
        location: SrcSpan,
        message: Option<Box<Self>>,
        type_: Arc<Type>,
    },

    Panic {
        location: SrcSpan,
        message: Option<Box<Self>>,
        type_: Arc<Type>,
    },

    BitArray {
        location: SrcSpan,
        typ: Arc<Type>,
        segments: Vec<TypedExprBitArraySegment>,
    },

    RecordUpdate {
        location: SrcSpan,
        typ: Arc<Type>,
        spread: Box<Self>,
        args: Vec<TypedRecordUpdateArg>,
    },

    NegateBool {
        location: SrcSpan,
        value: Box<Self>,
    },

    NegateInt {
        location: SrcSpan,
        value: Box<Self>,
    },
}

impl TypedExpr {
    pub fn find_node(&self, byte_index: u32) -> Option<Located<'_>> {
        match self {
            Self::Var { .. }
            | Self::Int { .. }
            | Self::Todo { .. }
            | Self::Panic { .. }
            | Self::Float { .. }
            | Self::String { .. }
            | Self::ModuleSelect { .. } => self.self_if_contains_location(byte_index),

            Self::Pipeline {
                assignments,
                finally,
                ..
            } => {
                if assignments
                    .first()
                    .is_some_and(|a| byte_index >= a.location.start)
                    && assignments
                        .last()
                        .is_some_and(|a| byte_index < a.location.end)
                {
                    if let Some(located) = assignments.iter().find_map(|a| a.find_node(byte_index))
                    {
                        return Some(located);
                    }

                    return None;
                }

                finally.find_node(byte_index)
            }

            Self::Block { statements, .. } => {
                for statement in statements {
                    if statement.location().start > byte_index {
                        cov_mark::hit!(early_exit_block);
                        break;
                    }

                    if let Some(located) = statement.find_node(byte_index) {
                        return Some(located);
                    }
                }

                None
            }

            Self::Tuple {
                elems: expressions, ..
            }
            | Self::List {
                elements: expressions,
                ..
            } => {
                for expression in expressions {
                    if expression.location().start > byte_index {
                        cov_mark::hit!(early_exit_tuple_list);
                        break;
                    }

                    if let Some(located) = expression.find_node(byte_index) {
                        return Some(located);
                    }
                }

                self.self_if_contains_location(byte_index)
            }

            Self::NegateBool { value, .. } | Self::NegateInt { value, .. } => value
                .find_node(byte_index)
                .or_else(|| self.self_if_contains_location(byte_index)),

            Self::Fn { body, args, .. } => {
                if args.first().is_some_and(|a| byte_index >= a.location.start)
                    && args.last().is_some_and(|a| byte_index < a.location.end)
                {
                    if let Some(located) = args.iter().find_map(|arg| arg.find_node(byte_index)) {
                        return Some(located);
                    }

                    return None;
                }

                if byte_index >= body.first().location().start
                    && byte_index < body.last().location().end
                {
                    if let Some(located) = body.iter().find_map(|s| s.find_node(byte_index)) {
                        return Some(located);
                    }
                    return None;
                }

                self.self_if_contains_location(byte_index)
            }

            Self::Call { fun, args, .. } => args
                .iter()
                .find_map(|arg| arg.find_node(byte_index))
                .or_else(|| fun.find_node(byte_index))
                .or_else(|| self.self_if_contains_location(byte_index)),

            Self::BinOp { left, right, .. } => left
                .find_node(byte_index)
                .or_else(|| right.find_node(byte_index)),

            Self::Case {
                subjects, clauses, ..
            } => {
                if subjects
                    .last()
                    .is_some_and(|s| byte_index < s.location().end)
                {
                    return subjects
                        .iter()
                        .find_map(|subject| subject.find_node(byte_index))
                        .or_else(|| self.self_if_contains_location(byte_index));
                }

                clauses
                    .iter()
                    .find_map(|c| c.find_node(byte_index))
                    .or_else(|| self.self_if_contains_location(byte_index))
            }

            Self::RecordAccess {
                record: expression, ..
            }
            | Self::TupleIndex {
                tuple: expression, ..
            } => expression
                .find_node(byte_index)
                .or_else(|| self.self_if_contains_location(byte_index)),

            Self::BitArray { segments, .. } => segments
                .iter()
                .find_map(|arg| arg.find_node(byte_index))
                .or_else(|| self.self_if_contains_location(byte_index)),

            Self::RecordUpdate { spread, args, .. } => args
                .iter()
                .find_map(|arg| arg.find_node(byte_index))
                .or_else(|| spread.find_node(byte_index))
                .or_else(|| self.self_if_contains_location(byte_index)),
        }
    }

    fn self_if_contains_location(&self, byte_index: u32) -> Option<Located<'_>> {
        if self.location().contains(byte_index) {
            Some(self.into())
        } else {
            None
        }
    }

    pub fn non_zero_compile_time_number(&self) -> bool {
        use regex::Regex;
        static NON_ZERO: OnceLock<Regex> = OnceLock::new();

        matches!(
            self,
            Self::Int{ value, .. } | Self::Float { value, .. } if NON_ZERO.get_or_init(||


                Regex::new(r"[1-9]").expect("NON_ZERO regex")).is_match(value)
        )
    }

    pub fn location(&self) -> SrcSpan {
        match self {
            Self::Fn { location, .. }
            | Self::Int { location, .. }
            | Self::Var { location, .. }
            | Self::Todo { location, .. }
            | Self::Case { location, .. }
            | Self::Call { location, .. }
            | Self::List { location, .. }
            | Self::Float { location, .. }
            | Self::BinOp { location, .. }
            | Self::Tuple { location, .. }
            | Self::Panic { location, .. }
            | Self::Block { location, .. }
            | Self::String { location, .. }
            | Self::NegateBool { location, .. }
            | Self::NegateInt { location, .. }
            | Self::Pipeline { location, .. }
            | Self::BitArray { location, .. }
            | Self::TupleIndex { location, .. }
            | Self::ModuleSelect { location, .. }
            | Self::RecordAccess { location, .. }
            | Self::RecordUpdate { location, .. } => *location,
        }
    }

    pub fn type_defining_location(&self) -> SrcSpan {
        match self {
            Self::Fn { location, .. }
            | Self::Int { location, .. }
            | Self::Var { location, .. }
            | Self::Todo { location, .. }
            | Self::Case { location, .. }
            | Self::Call { location, .. }
            | Self::List { location, .. }
            | Self::Float { location, .. }
            | Self::BinOp { location, .. }
            | Self::Tuple { location, .. }
            | Self::String { location, .. }
            | Self::Panic { location, .. }
            | Self::NegateBool { location, .. }
            | Self::NegateInt { location, .. }
            | Self::Pipeline { location, .. }
            | Self::BitArray { location, .. }
            | Self::TupleIndex { location, .. }
            | Self::ModuleSelect { location, .. }
            | Self::RecordAccess { location, .. }
            | Self::RecordUpdate { location, .. } => *location,
            Self::Block { statements, .. } => statements.last().location(),
        }
    }

    pub fn definition_location(&self) -> Option<DefinitionLocation<'_>> {
        match self {
            TypedExpr::Fn { .. }
            | TypedExpr::Int { .. }
            | TypedExpr::List { .. }
            | TypedExpr::Call { .. }
            | TypedExpr::Case { .. }
            | TypedExpr::Todo { .. }
            | TypedExpr::Panic { .. }
            | TypedExpr::BinOp { .. }
            | TypedExpr::Float { .. }
            | TypedExpr::Tuple { .. }
            | TypedExpr::NegateBool { .. }
            | TypedExpr::NegateInt { .. }
            | TypedExpr::String { .. }
            | TypedExpr::Block { .. }
            | TypedExpr::Pipeline { .. }
            | TypedExpr::BitArray { .. }
            | TypedExpr::TupleIndex { .. }
            | TypedExpr::RecordAccess { .. } => None,

            // TODO: test
            // TODO: definition
            TypedExpr::RecordUpdate { .. } => None,

            // TODO: test
            TypedExpr::ModuleSelect {
                module_name,
                constructor,
                ..
            } => Some(DefinitionLocation {
                module: Some(module_name.as_str()),
                span: constructor.location(),
            }),

            // TODO: test
            TypedExpr::Var { constructor, .. } => Some(constructor.definition_location()),
        }
    }

    pub fn type_(&self) -> Arc<Type> {
        match self {
            Self::NegateBool { .. } => bool(),
            Self::NegateInt { value, .. } => value.type_(),
            Self::Var { constructor, .. } => constructor.type_.clone(),
            Self::Fn { typ, .. }
            | Self::Int { typ, .. }
            | Self::Todo { type_: typ, .. }
            | Self::Case { typ, .. }
            | Self::List { typ, .. }
            | Self::Call { typ, .. }
            | Self::Float { typ, .. }
            | Self::Panic { type_: typ, .. }
            | Self::BinOp { typ, .. }
            | Self::Tuple { typ, .. }
            | Self::String { typ, .. }
            | Self::BitArray { typ, .. }
            | Self::TupleIndex { typ, .. }
            | Self::ModuleSelect { typ, .. }
            | Self::RecordAccess { typ, .. }
            | Self::RecordUpdate { typ, .. } => typ.clone(),
            Self::Pipeline { finally, .. } => finally.type_(),
            Self::Block { statements, .. } => statements.last().type_(),
        }
    }

    pub fn is_literal(&self) -> bool {
        match self {
            Self::Int { .. }
            | Self::List { .. }
            | Self::Float { .. }
            | Self::Tuple { .. }
            | Self::String { .. }
            | Self::BitArray { .. } => true,
            _ => false,
        }
    }

    /// Returns `true` if the typed expr is [`Var`].
    ///
    /// [`Var`]: TypedExpr::Var
    #[must_use]
    pub fn is_var(&self) -> bool {
        match self {
            Self::Var { .. } => true,
            _ => false,
        }
    }

    pub(crate) fn get_documentation(&self) -> Option<&str> {
        match self {
            TypedExpr::Var { constructor, .. } => constructor.get_documentation(),
            TypedExpr::ModuleSelect { constructor, .. } => constructor.get_documentation(),

            TypedExpr::Int { .. }
            | TypedExpr::Float { .. }
            | TypedExpr::String { .. }
            | TypedExpr::Block { .. }
            | TypedExpr::Pipeline { .. }
            | TypedExpr::Fn { .. }
            | TypedExpr::List { .. }
            | TypedExpr::Call { .. }
            | TypedExpr::BinOp { .. }
            | TypedExpr::Case { .. }
            | TypedExpr::Tuple { .. }
            | TypedExpr::TupleIndex { .. }
            | TypedExpr::Todo { .. }
            | TypedExpr::Panic { .. }
            | TypedExpr::BitArray { .. }
            | TypedExpr::RecordUpdate { .. }
            | TypedExpr::RecordAccess { .. }
            | TypedExpr::NegateBool { .. }
            | TypedExpr::NegateInt { .. } => None,
        }
    }

    /// Returns `true` if the typed expr is [`Case`].
    ///
    /// [`Case`]: TypedExpr::Case
    #[must_use]
    pub fn is_case(&self) -> bool {
        match self {
            Self::Case { .. } => true,
            _ => false,
        }
    }

    /// Returns `true` if the typed expr is [`Pipeline`].
    ///
    /// [`Pipeline`]: TypedExpr::Pipeline
    #[must_use]
    pub fn is_pipeline(&self) -> bool {
        match self {
            Self::Pipeline { .. } => true,
            _ => false,
        }
    }

    pub fn is_pure_value_constructor(&self) -> bool {
        match self {
            TypedExpr::Int { .. }
            | TypedExpr::Float { .. }
            | TypedExpr::String { .. }
            | TypedExpr::List { .. }
            | TypedExpr::Tuple { .. }
            | TypedExpr::BitArray { .. }
            | TypedExpr::Var { .. }
            | TypedExpr::BinOp { .. }
            | TypedExpr::RecordAccess { .. }
            | TypedExpr::TupleIndex { .. }
            | TypedExpr::RecordUpdate { .. }
            | TypedExpr::ModuleSelect { .. }
            | TypedExpr::Fn { .. } => true,

            TypedExpr::NegateBool { value, .. } | TypedExpr::NegateInt { value, .. } => {
                value.is_pure_value_constructor()
            }

            // A pipeline is a pure value constructor if its last step is a record builder.
            // For example `wibble() |> wobble() |> Ok`
            TypedExpr::Pipeline { finally, .. } => finally.is_record_builder(),

            // A function call is a pure record constructor if it is a record builder.
            // For example `Ok(wobble(wibble()))`
            TypedExpr::Call { fun, .. } => fun.as_ref().is_record_builder(),

            // Blocks and Cases are not considered pure value constructors for now,
            // in the future we might want to do something a bit smarter and inspect
            // their content to see if they end up returning something that is a
            // pure value constructor and raise a warning for those as well.
            TypedExpr::Block { .. } | TypedExpr::Case { .. } => false,

            // `panic` and `todo` are never considered pure value constructors,
            // we don't want to raise a warning for an unused value if it's one
            // of those two.
            TypedExpr::Todo { .. } | TypedExpr::Panic { .. } => false,
        }
    }

    pub fn is_record_builder(&self) -> bool {
        match self {
            TypedExpr::Call { fun, .. } => fun.is_record_builder(),
            TypedExpr::Var { constructor, .. } => constructor.variant.is_record(),
            _ => false,
        }
    }

    pub fn to_string(&self) -> Option<EcoString> {
        match self {
            TypedExpr::Int { value, .. } => Some(value.clone()),
            TypedExpr::Float { value, .. } => Some(value.clone()),
            TypedExpr::String { value, .. } => Some(value.clone()),
            TypedExpr::Var { name, .. } => Some(name.clone()),
            TypedExpr::List {
                location: _,
                typ: _,
                elements,
                tail,
            } => {
                let mut result = EcoString::from("[");

                for (i, element) in elements.iter().enumerate() {
                    if i > 0 {
                        result.push_str(", ");
                    }
                    result.push_str(&element.to_string()?);
                }

                if let Some(tail_expr) = tail {
                    result.push_str(", ");
                    result.push_str(&tail_expr.to_string()?);
                }

                result.push_str("]");
                Some(result)
            }
            TypedExpr::Call {
                location: _,
                typ: _,
                fun,
                args,
            } => {
                let mut fun_str = fun.to_string()?;

                //Init with opening parenthesis
                let mut args_str = EcoString::from("(");

                for (i, arg) in args.iter().enumerate() {
                    if let Some(arg_string) = arg.value.to_string() {
                        if i > 0 {
                            args_str.push_str(", ");
                        }
                        args_str.push_str(&arg_string);
                    } else {
                        return None;
                    }
                }

                //Append closing parenthesis
                args_str.push(')');

                fun_str.push_str(&args_str);
                Some(fun_str)
            }
            TypedExpr::BinOp {
                location: _,
                typ: _,
                name,
                left,
                right,
            } => match (left.to_string(), right.to_string()) {
                (Some(left_str), Some(right_str)) => Some(EcoString::from(format!(
                    "{} {} {}",
                    left_str,
                    name.name(),
                    right_str
                ))),
                _ => None,
            },
            TypedExpr::ModuleSelect {
                location: _,
                typ: _,
                label,
                module_name: _,
                module_alias,
                constructor: _,
            } => Some(EcoString::from(format!("{}.{}", module_alias, label))),
            TypedExpr::Tuple { elems, .. } => {
                //Opening parenthesis tuple
                let mut res = EcoString::from("#(");

                for (i, elem) in elems.iter().enumerate() {
                    if i > 0 {
                        res.push_str(", ");
                    }

                    if let Some(elem_str) = elem.to_string() {
                        res.push_str(&elem_str);
                    } else {
                        return None;
                    }
                }

                //Closing parenthesis tuple
                res.push(')');
                Some(res)
            }
            TypedExpr::NegateBool { value, .. } => value.to_string(),
            TypedExpr::NegateInt { value, .. } => value.to_string(),
            _ => None,
        }
    }
}

impl<'a> From<&'a TypedExpr> for Located<'a> {
    fn from(value: &'a TypedExpr) -> Self {
        Located::Expression(value)
    }
}

impl HasLocation for TypedExpr {
    fn location(&self) -> SrcSpan {
        self.location()
    }
}

impl HasType for TypedExpr {
    fn type_(&self) -> Arc<Type> {
        self.type_()
    }
}

impl crate::bit_array::GetLiteralValue for TypedExpr {
    fn as_int_literal(&self) -> Option<i64> {
        if let TypedExpr::Int { value: val, .. } = self {
            if let Ok(val) = val.parse::<i64>() {
                return Some(val);
            }
        }
        None
    }
}
