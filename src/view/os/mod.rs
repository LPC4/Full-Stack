pub mod interrupt_view;
pub mod page_table_view;
pub mod privilege_view;
pub mod syscall_trace_view;
pub mod trap_view;

pub use interrupt_view::InterruptView;
pub use page_table_view::PageTableView;
pub use privilege_view::PrivilegeView;
pub use syscall_trace_view::SyscallTraceView;
pub use trap_view::TrapView;
