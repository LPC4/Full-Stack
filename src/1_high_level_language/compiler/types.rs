use super::*;

impl HighLevelCompiler {
    pub(super) fn lower_return_type(&mut self, return_type: Option<&ReturnType>) -> IrType {
        match return_type {
            Some(ReturnType::Single(ty)) => self.lower_type(ty),
            None => IrType::Void,
        }
    }

    pub(super) fn lower_type(&mut self, ty: &Type) -> IrType {
        match ty {
            Type::Primitive(name) => self.lower_primitive_type(name),
            Type::Pointer(inner) => IrType::Pointer(Box::new(self.lower_type(inner))),
            Type::Array(len, inner) => IrType::Array {
                len: *len,
                element: Box::new(self.lower_type(inner)),
            },
            Type::Struct(fields) => IrType::Aggregate(
                fields
                    .iter()
                    .map(|f| (f.name.clone(), self.lower_type(&f.ty)))
                    .collect(),
            ),
            Type::Named { name, args } => {
                if !args.is_empty() {
                    let lowered_args: Vec<IrType> =
                        args.iter().map(|a| self.lower_type(a)).collect();
                    if self.generic_type_defs.contains_key(name) {
                        let specialized_name = self.create_specialized_name(name, &lowered_args);
                        return IrType::Named(specialized_name);
                    } else {
                        return IrType::Named(self.create_specialized_name(name, &lowered_args));
                    }
                }
                IrType::Named(name.clone())
            }
        }
    }

    pub(super) fn create_specialized_name(&self, name: &str, type_args: &[IrType]) -> String {
        let args = type_args
            .iter()
            .map(|t| t.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        format!("{}<{}>", name, args)
    }

    pub(super) fn lower_type_with_program(
        &mut self,
        ir_program: &mut IrProgram,
        ty: &Type,
    ) -> Result<IrType, CompilerError> {
        match ty {
            Type::Primitive(name) => Ok(self.lower_primitive_type(name)),
            Type::Pointer(inner) => Ok(IrType::Pointer(Box::new(
                self.lower_type_with_program(ir_program, inner)?,
            ))),
            Type::Array(len, inner) => Ok(IrType::Array {
                len: *len,
                element: Box::new(self.lower_type_with_program(ir_program, inner)?),
            }),
            Type::Struct(fields) => Ok(IrType::Aggregate(
                fields
                    .iter()
                    .map(|f| {
                        self.lower_type_with_program(ir_program, &f.ty)
                            .map(|lowered_ty| (f.name.clone(), lowered_ty))
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            )),
            Type::Named { name, args } => {
                if !args.is_empty() {
                    // Lower all type arguments first
                    let lowered_args: Vec<IrType> = args
                        .iter()
                        .map(|a| self.lower_type_with_program(ir_program, a))
                        .collect::<Result<Vec<_>, _>>()?;

                    // Check if this is a known generic type
                    if self.generic_type_defs.contains_key(name.as_str()) {
                        // Specialize the generic type
                        let specialized_name =
                            self.specialize_generic_type(ir_program, name, &lowered_args)?;
                        Ok(IrType::Named(specialized_name))
                    } else {
                        // Not a generic definition, just mangle
                        let mangled = self.create_specialized_name(name, &lowered_args);
                        Ok(IrType::Named(mangled))
                    }
                } else {
                    Ok(IrType::Named(name.clone()))
                }
            }
        }
    }

    pub(super) fn lower_primitive_type(&self, name: &str) -> IrType {
        match name {
            "i8" => IrType::Integer(IntWidth::I8),
            "i16" => IrType::Integer(IntWidth::I16),
            "i32" => IrType::Integer(IntWidth::I32),
            "i64" => IrType::Integer(IntWidth::I64),
            "u8" => IrType::Integer(IntWidth::I8),
            "u16" => IrType::Integer(IntWidth::I16),
            "u32" => IrType::Integer(IntWidth::I32),
            "u64" => IrType::Integer(IntWidth::I64),
            "f32" => IrType::Float(FloatWidth::F32),
            "f64" => IrType::Float(FloatWidth::F64),
            "bool" => IrType::Integer(IntWidth::I1),
            other => IrType::Named(other.to_owned()),
        }
    }

    pub(super) fn substitute_generic_type(
        &self,
        ty: &Type,
        params: &[String],
        args: &[IrType],
    ) -> Type {
        match ty {
            Type::Primitive(_) | Type::Pointer(_) | Type::Array(_, _) | Type::Struct(_) => {
                // Recursively substitute in nested types
                self.substitute_in_type(ty, params, args)
            }
            Type::Named { name, args: _args } => {
                // Check if this is a generic parameter
                if let Some(idx) = params.iter().position(|p| p == name) {
                    if idx < args.len() {
                        // Replace with the concrete type argument
                        // Convert IrType back to Type for substitution
                        self.ir_type_to_type(&args[idx])
                    } else {
                        ty.clone()
                    }
                } else {
                    // Not a generic parameter, recursively substitute
                    self.substitute_in_type(ty, params, args)
                }
            }
        }
    }

    pub(super) fn substitute_in_type(&self, ty: &Type, params: &[String], args: &[IrType]) -> Type {
        match ty {
            Type::Primitive(_) => ty.clone(),
            Type::Pointer(inner) => {
                Type::Pointer(Box::new(self.substitute_generic_type(inner, params, args)))
            }
            Type::Array(len, inner) => Type::Array(
                *len,
                Box::new(self.substitute_generic_type(inner, params, args)),
            ),
            Type::Struct(fields) => Type::Struct(
                fields
                    .iter()
                    .map(|f| crate::high_level_language::ast::FieldDecl {
                        name: f.name.clone(),
                        ty: self.substitute_generic_type(&f.ty, params, args),
                        init: f.init.clone(), // Expressions not substituted yet
                    })
                    .collect(),
            ),
            Type::Named {
                name,
                args: type_args,
            } => {
                // Check if this name is a generic parameter
                if let Some(idx) = params.iter().position(|p| p == name) {
                    if idx < args.len() {
                        return self.ir_type_to_type(&args[idx]);
                    }
                }
                // Otherwise recursively substitute in type arguments
                Type::Named {
                    name: name.clone(),
                    args: type_args
                        .iter()
                        .map(|ta| self.substitute_generic_type(ta, params, args))
                        .collect(),
                }
            }
        }
    }

    pub(super) fn ir_type_to_type(&self, ir_ty: &IrType) -> Type {
        match ir_ty {
            IrType::Integer(width) => {
                let name = match width {
                    IntWidth::I1 => "bool",
                    IntWidth::I8 => "i8",
                    IntWidth::I16 => "i16",
                    IntWidth::I32 => "i32",
                    IntWidth::I64 => "i64",
                };
                Type::Primitive(name.to_string())
            }
            IrType::Float(width) => {
                let name = match width {
                    FloatWidth::F32 => "f32",
                    FloatWidth::F64 => "f64",
                };
                Type::Primitive(name.to_string())
            }
            IrType::Pointer(inner) => Type::Pointer(Box::new(self.ir_type_to_type(inner))),
            IrType::Array { len, element } => {
                Type::Array(*len, Box::new(self.ir_type_to_type(element)))
            }
            IrType::Aggregate(fields) => {
                // Convert to struct type
                Type::Struct(
                    fields
                        .iter()
                        .map(|(name, ty)| crate::high_level_language::ast::FieldDecl {
                            name: name.clone(),
                            ty: self.ir_type_to_type(ty),
                            init: None,
                        })
                        .collect(),
                )
            }
            IrType::Named(name) => Type::Named {
                name: name.clone(),
                args: Vec::new(),
            },
            IrType::Void => Type::Primitive("void".to_string()),
        }
    }

    pub(super) fn specialize_generic_type(
        &mut self,
        ir_program: &mut IrProgram,
        name: &str,
        type_args: &[IrType],
    ) -> Result<String, CompilerError> {
        // Check cache first
        let cache_key = (name.to_string(), type_args.to_vec());
        if let Some(specialized_name) = self.generic_type_cache.get(&cache_key) {
            return Ok(specialized_name.clone());
        }

        // Get the generic type definition
        let generic_def = self.generic_type_defs.get(name).ok_or_else(|| {
            CompilerError::UnsupportedDeclaration(format!("Unknown generic type `{}`", name))
        })?;

        // Validate argument count
        if generic_def.params.len() != type_args.len() {
            return Err(CompilerError::UnsupportedDeclaration(format!(
                "Generic type `{}` expects {} type arguments, got {}",
                name,
                generic_def.params.len(),
                type_args.len()
            )));
        }

        // Use canonical angle-bracket naming so generic instances cannot collide with user identifiers.
        let specialized_name = self.create_specialized_name(name, type_args);

        log::debug!(
            "Specializing generic type `{}` as `{}` with args: {:?}",
            name,
            specialized_name,
            type_args
        );

        // Substitute generic parameters with concrete types
        let specialized_ty =
            self.substitute_generic_type(&generic_def.ty, &generic_def.params, type_args);

        let lowered_ty = self.lower_type(&specialized_ty);

        self.context
            .types
            .register_type(specialized_name.clone(), lowered_ty.clone());
        ir_program.push_type_alias(IrTypeAlias {
            name: specialized_name.clone(),
            ty: lowered_ty,
        });

        // Cache the result
        self.generic_type_cache
            .insert(cache_key, specialized_name.clone());

        Ok(specialized_name)
    }
}
