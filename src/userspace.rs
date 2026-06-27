//! Embedded userspace program catalog generated from `programs/user/**/*.build`.

/// Role of a bundled user program.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UserProgramKind {
    /// HLL source compiled to an ELF and installed under `/bin`.
    Tool,
    /// HLL source compiled to an ELF and installed under `/home/demo`.
    Demo,
    /// Verbatim source installed under `/home/src` for the in-VM toolchain.
    Example,
    /// Frozen test input; not installed into the boot image.
    Fixture,
}

/// One bundled userspace program or source artifact.
#[derive(Clone, Copy, Debug)]
pub struct UserProgram {
    pub name: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub kind: UserProgramKind,
    pub install_path: Option<&'static str>,
    pub source: &'static str,
    pub source_path: &'static str,
    pub build_path: &'static str,
    pub import_closure: bool,
    pub mangle_symbols: bool,
    pub layout: &'static str,
}

impl UserProgram {
    /// HLL programs the host compiles to an ELF.
    pub fn is_compiled(&self) -> bool {
        matches!(self.kind, UserProgramKind::Tool | UserProgramKind::Demo)
    }
}

include!(concat!(env!("OUT_DIR"), "/userspace.rs"));
