#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub declarations: Vec<Declaration>,
    pub statements: Vec<Statement>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Primitive(String),
    Pointer(Box<Self>),
    Array(usize, Box<Self>),
    // T[] slice: a {ptr, len} fat pointer, bounds-checked at use.
    Slice(Box<Self>),
    Struct(Vec<FieldDecl>),
    Named { name: String, args: Vec<Self> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct FieldDecl {
    pub name: String,
    pub ty: Type,
    pub init: Option<Expression>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Declaration {
    pub decl: DeclNode,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DeclNode {
    Variable {
        name: String,
        ty: Type,
        init: Option<Expression>,
        // External globals are defined in another module; storage is not emitted
        // here, only the name + type are recorded so references resolve at link.
        is_extern: bool,
    },
    InferredVariable {
        name: String,
        init: Expression,
    },
    Function {
        name: String,
        generics: Vec<String>,
        params: Vec<Parameter>,
        return_type: Option<ReturnType>,
        body: Option<Block>,
        is_extern: bool,
    },
    Type {
        name: String,
        generics: Vec<String>,
        ty: Type,
    },
    Struct {
        name: String,
        generics: Vec<String>,
        fields: Vec<FieldDecl>,
    },
    // A tagged union. Lowers to `{ tag: i32, payload }` where `payload` is
    // sized to the largest variant.
    Enum {
        name: String,
        generics: Vec<String>,
        variants: Vec<Variant>,
    },
    Const {
        name: String,
        init: Expression,
    },
    Import {
        path: String,
    },
}

// One arm of an `enum`. An empty `payload` is a unit variant (`None`); a
// non-empty one is a tuple-like variant (`Circle(f64)`, `Rect(f64, f64)`).
#[derive(Debug, Clone, PartialEq)]
pub struct Variant {
    pub name: String,
    pub payload: Vec<Type>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Parameter {
    pub name: String,
    pub ty: Type,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ReturnType {
    Single(Type),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub statements: Vec<Statement>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Statement {
    Expression(Expression),
    Block(Block),
    If {
        cond: Expression,
        then_block: Block,
        else_branch: Option<Box<Self>>,
    },
    While {
        cond: Expression,
        body: Block,
    },
    For {
        var: String,
        iter: ForIter,
        body: Block,
    },
    Return(Option<Expression>),
    Defer(Expression),
    AsmBlock {
        lines: Vec<String>,
    },
    Break,
    Continue,
    VariableDecl {
        name: String,
        ty: Type,
        init: Option<Expression>,
    },
    InferredVariableDecl {
        name: String,
        init: Expression,
    },
}

// The iterable in a `for` loop.
#[derive(Debug, Clone, PartialEq)]
pub enum ForIter {
    // `start..end`, half-open unless `inclusive` (`..=`).
    Range {
        start: Expression,
        end: Expression,
        inclusive: bool,
    },
    // `arr` -- iterate a fixed array's elements by value.
    Each(Expression),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expression {
    Assignment {
        target: Box<AssignTarget>,
        rvalue: Box<Self>,
    },
    Binary {
        op: BinaryOp,
        left: Box<Self>,
        right: Box<Self>,
    },
    Unary {
        op: UnaryOp,
        expr: Box<Self>,
    },
    Cast {
        target_ty: Type,
        expr: Box<Self>,
    },
    // `match scrutinee { pattern -> block ... }`. Exhaustive over the
    // scrutinee enum's variants.
    Match {
        scrutinee: Box<Self>,
        arms: Vec<MatchArm>,
    },
    // `expr?`: propagate failure with a visible early return.
    Try(Box<Self>),
    Primary(PrimaryExpr),
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchArm {
    pub pattern: Pattern,
    // Statements executed before the arm yields. A `Pattern -> { ... }` arm fills
    // this; a `Pattern -> expr` value arm leaves it empty.
    pub body: Block,
    // Some(expr) for a value arm (`Pattern -> expr`); None for a statement arm.
    // A `match` whose arms all carry a value is value-producing.
    pub value: Option<Expression>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pattern {
    // `_` -- matches anything, binds nothing.
    Wildcard,
    // A bare lowercase name -- catch-all that binds the scrutinee.
    Binding(String),
    // `Variant(b0, b1)` or `Enum::Variant(...)`; `bindings` names each payload
    // slot (`_` discards). An empty `bindings` is a unit-variant pattern.
    Variant {
        enum_name: Option<String>,
        variant: String,
        bindings: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum AssignTarget {
    Identifier(String),
    Dereference(Box<Self>),
    FieldAccess {
        expr: Box<Self>,
        field: String,
    },
    ArrayIndex {
        expr: Box<Self>,
        index: Box<Expression>,
    },
    StructDestructure(Vec<StructDestructureField>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructDestructureField {
    pub name: Option<String>, // None for discard (_)
    pub ty: Option<Type>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Shl,
    Shr,
    Eq,
    Neq,
    Lt,
    Lte,
    Gt,
    Gte,
    And,
    Or,
    BitwiseAnd,
    BitwiseXor,
    BitwiseOr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnaryOp {
    Negate,
    Not,
    AddressOf,
    Dereference,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PrimaryExpr {
    Identifier(String),
    Literal(Literal),
    Grouped(Box<Expression>),
    FunctionCall {
        name: String,
        type_arguments: Vec<Type>,
        arguments: Vec<Expression>,
    },
    ArrayLiteral(Vec<Expression>),
    StructLiteral(Vec<FieldInit>),
    NamedStructLiteral {
        name: String,
        fields: Vec<FieldInit>,
    },
    FieldAccess {
        expr: Box<Expression>,
        field: String,
    },
    ArrayIndex {
        expr: Box<Expression>,
        index: Box<Expression>,
    },
    // `arr[a..b]` / `arr[a..=b]` -- a sub-slice. Open endpoints default to
    // 0 (start) and the source length (end).
    Slice {
        expr: Box<Expression>,
        start: Option<Box<Expression>>,
        end: Option<Box<Expression>>,
        inclusive: bool,
    },
    New {
        ty: Type,
        args: Vec<Expression>, // Optional size/capacity args
    },
    AsmReg {
        reg: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    Integer(i64),
    HexInteger(i64),
    Float(f64),
    Boolean(bool),
    Null,
    String(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct FieldInit {
    pub name: String,
    pub ty: Option<Type>,
    pub expr: Expression,
}
