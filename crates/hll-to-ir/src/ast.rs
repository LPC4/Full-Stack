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
    Const {
        name: String,
        init: Expression,
    },
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
    Primary(PrimaryExpr),
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
        arguments: Vec<Expression>,
    },
    ArrayLiteral(Vec<Expression>),
    StructLiteral(Vec<FieldInit>),
    FieldAccess {
        expr: Box<Expression>,
        field: String,
    },
    ArrayIndex {
        expr: Box<Expression>,
        index: Box<Expression>,
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
