use crate::intermediate_language::IrType;
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct TypeContext {
    named_types: HashMap<String, IrType>,
}

impl TypeContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.named_types.clear();
    }

    pub fn register_type(&mut self, name: impl Into<String>, ty: IrType) {
        self.named_types.insert(name.into(), ty);
    }

    pub fn resolve(&self, name: &str) -> Option<&IrType> {
        self.named_types.get(name)
    }
}
