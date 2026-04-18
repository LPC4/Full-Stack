#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub declarations: Vec<Declaration>,
    pub statements: Vec<Statement>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Primitive(String),
    Pointer(Box<Type>),
    Array(usize, Box<Type>),
    Struct(Vec<FieldDecl>),
    Named { name: String, args: Vec<Type> },
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
pub struct ReturnField {
    pub name: Option<String>,
    pub ty: Type,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ReturnType {
    Single(Type),
    Tuple(Vec<ReturnField>),
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
        else_branch: Option<Box<Statement>>,
    },
    While {
        cond: Expression,
        body: Block,
    },
    Return(Option<Expression>),
    Defer(Expression),
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
        rvalue: Box<Expression>,
    },
    Tuple(Vec<Expression>),
    Binary {
        op: BinaryOp,
        left: Box<Expression>,
        right: Box<Expression>,
    },
    Unary {
        op: UnaryOp,
        expr: Box<Expression>,
    },
    Primary(PrimaryExpr),
}

#[derive(Debug, Clone, PartialEq)]
pub enum AssignTarget {
    Identifier(String),
    Dereference(Box<AssignTarget>),
    FieldAccess {
        expr: Box<AssignTarget>,
        field: String,
    },
    ArrayIndex {
        expr: Box<AssignTarget>,
        index: Box<Expression>,
    },
    Tuple(Vec<AssignTarget>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Neq,
    Lt,
    Lte,
    Gt,
    Gte,
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq)]
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
    FunctionCall {
        name: String,
        arguments: Vec<Expression>,
    },
    ArrayLiteral(Vec<Expression>),
    TupleLiteral(Vec<Expression>),
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
}

#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    Integer(i64),
    HexInteger(i64),
    Float(f64),
    StringLit(String),
    Boolean(bool),
    Null,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FieldInit {
    pub name: Option<String>,
    pub expr: Expression,
}
