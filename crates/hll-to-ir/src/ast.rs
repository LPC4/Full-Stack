#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub declarations: Vec<Declaration>,
    pub statements: Vec<Statement>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Path {
    pub segments: Vec<String>,
    canonical: String,
}

impl Path {
    pub fn single(name: impl Into<String>) -> Self {
        Self::from_segments(vec![name.into()])
    }

    pub fn from_segments(segments: Vec<String>) -> Self {
        let canonical = segments.join(".");
        Self {
            segments,
            canonical,
        }
    }

    pub fn as_string(&self) -> String {
        self.canonical.clone()
    }
}

impl std::ops::Deref for Path {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.canonical
    }
}

impl std::borrow::Borrow<str> for Path {
    fn borrow(&self) -> &str {
        &self.canonical
    }
}

impl PartialEq<str> for Path {
    fn eq(&self, other: &str) -> bool {
        self.canonical == other
    }
}

impl PartialEq<&str> for Path {
    fn eq(&self, other: &&str) -> bool {
        self.canonical == *other
    }
}

impl From<String> for Path {
    fn from(value: String) -> Self {
        Self::from_segments(value.split('.').map(str::to_owned).collect())
    }
}

impl From<&str> for Path {
    fn from(value: &str) -> Self {
        value.to_owned().into()
    }
}

impl std::fmt::Display for Path {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.as_string())
    }
}

/// An array length in type position: a literal from the parser, or a compile-time
/// constant expression folded to `Fixed` before semantic analysis.
#[derive(Debug, Clone, PartialEq)]
pub enum ArrayLen {
    Fixed(usize),
    Const(Box<Expression>),
}

impl ArrayLen {
    /// The resolved length, or `None` while the constant expression is still folded.
    pub fn fixed(&self) -> Option<usize> {
        match self {
            Self::Fixed(len) => Some(*len),
            Self::Const(_) => None,
        }
    }
}

impl From<usize> for ArrayLen {
    fn from(value: usize) -> Self {
        Self::Fixed(value)
    }
}

impl std::fmt::Display for ArrayLen {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Fixed(len) => write!(formatter, "{len}"),
            Self::Const(_) => formatter.write_str("<const>"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Primitive(String),
    Pointer(Box<Self>),
    Array(ArrayLen, Box<Self>),
    // T[] slice: a {ptr, len} fat pointer, bounds-checked at use.
    Slice(Box<Self>),
    Function {
        params: Vec<Self>,
        return_type: Option<Box<Self>>,
    },
    Struct(Vec<FieldDecl>),
    DynamicReference(Path),
    Named {
        name: Path,
        args: Vec<Self>,
    },
    // `Matrix<i32>.Row`: an associated type alias projected from an impl owner.
    // Resolved to its concrete aliased type before semantic analysis.
    Associated {
        owner: String,
        owner_args: Vec<Self>,
        member: String,
    },
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
    // Marked `export`: visible to importing modules. Unexported declarations are
    // module private. Carried for interface extraction and visibility checks.
    pub exported: bool,
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
        // `(param, interface)` pairs from `<T: Writer>`; checked when the generic
        // is specialized. Empty for unbounded generics and plain functions.
        bounds: Vec<(String, Path)>,
        params: Vec<Parameter>,
        return_type: Option<ReturnType>,
        body: Option<Block>,
        is_extern: bool,
        // Generated import-interface externs are link references, but should not
        // produce user-facing "definition omitted" warnings.
        is_import_interface: bool,
    },
    Type {
        name: String,
        generics: Vec<String>,
        bounds: Vec<(String, Path)>,
        ty: Type,
    },
    Struct {
        name: String,
        generics: Vec<String>,
        bounds: Vec<(String, Path)>,
        fields: Vec<FieldDecl>,
    },
    // A tagged union. Lowers to `{ tag: i32, payload }` where `payload` is
    // sized to the largest variant.
    Enum {
        name: String,
        generics: Vec<String>,
        bounds: Vec<(String, Path)>,
        variants: Vec<Variant>,
    },
    Const {
        name: String,
        init: Expression,
    },
    // `alias := import("path")` / `const alias = import("path")`: a compile-time module
    // binding. Resolved by the host pipeline; exports are reached as `alias.member`.
    ModuleImport {
        alias: String,
        path: String,
    },
    // `interface Name { method: (self: Self*, ...) -> R }`: a compile-time set of
    // required method signatures. Consumed by generic-bound checking; never lowered.
    Interface {
        name: String,
        methods: Vec<InterfaceMethod>,
    },
    // `impl Owner<T> { const LANES = 4  type Row = T[4] }`: compile-time members
    // projected as `Owner<args>.Member`. Function members desugar separately.
    AssociatedItems {
        owner: String,
        generics: Vec<String>,
        consts: Vec<(String, Expression)>,
        aliases: Vec<(String, Type)>,
    },
}

// One required method in an `interface`. Parameter types use `Self` as a
// placeholder for the implementing type (`self: Self*` is the receiver).
#[derive(Debug, Clone, PartialEq)]
pub struct InterfaceMethod {
    pub name: String,
    pub params: Vec<Parameter>,
    pub return_type: Option<ReturnType>,
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

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Block {
    pub statements: Vec<Statement>,
    // 1-based source line per statement (parallel to `statements`), captured at
    // parse time. Consumed by the marker-weave pass; empty on synthesized blocks.
    pub stmt_lines: Vec<u32>,
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
    DestructureBinding {
        fields: Vec<BindingField>,
        init: Expression,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindingField {
    pub source: String,
    pub binding: String,
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

#[derive(Debug, Clone, PartialEq)]
pub enum Pattern {
    // `_` matches anything and binds nothing.
    Wildcard,
    // A bare lowercase name; catch-all that binds the scrutinee.
    Binding(String),
    // `Variant(b0, b1)` or `Enum::Variant(...)`; `bindings` names each payload
    // slot (`_` discards). An empty `bindings` is a unit-variant pattern.
    Variant {
        enum_name: Option<String>,
        variant: String,
        bindings: Vec<String>,
    },
    // An integer or char literal; matches a scalar scrutinee by value equality.
    Literal(Literal),
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
    BitNot,
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
    // A call through an arbitrary callee expression (function-pointer value,
    // struct field, indexed element). Bare-name calls stay `FunctionCall`.
    CallExpr {
        callee: Box<Expression>,
        arguments: Vec<Expression>,
    },
    ArrayLiteral(Vec<Expression>),
    // `rest` records a trailing `..`, which zero-fills every unnamed field.
    // Without it a literal must set every field of its struct.
    StructLiteral {
        fields: Vec<FieldInit>,
        rest: bool,
    },
    NamedStructLiteral {
        name: String,
        fields: Vec<FieldInit>,
        rest: bool,
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
    // `Matrix<i32>.LANES`: an associated constant projected from an impl owner.
    // Folded to its literal value before semantic analysis.
    AssociatedConst {
        owner: String,
        owner_args: Vec<Type>,
        member: String,
    },
    // `alias.format_into(dest, "a{x}b")`: compiler-recognized buffer
    // formatting over the string module runtime (spec 5.5).
    FormatInto {
        alias: String,
        dest: Box<Expression>,
        segments: Vec<FormatSegment>,
    },
}

// One piece of a `format_into` template: literal bytes (escapes resolved,
// `{{`/`}}` folded) or one interpolated hole expression.
#[derive(Debug, Clone, PartialEq)]
pub enum FormatSegment {
    Literal(String),
    Hole(Expression),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    Integer(i64),
    // Radix is kept only so a literal prints back in the base it was written in.
    HexInteger(i64),
    BinInteger(i64),
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
