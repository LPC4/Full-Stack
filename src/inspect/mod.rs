//! Read-only inspectors that decode kernel/OS state from guest memory into plain
//! data structs. No UI: a future IDE renders these; tests assert on them directly.

pub mod elf;
pub mod fs_view;
pub mod os_view;
