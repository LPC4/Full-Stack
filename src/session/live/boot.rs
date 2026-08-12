use super::{
    AssembledOutput, BTreeSet, BuildArtifacts, FsSymbols, LinkLayout, LiveMachine, MachineState,
    OsSymbols, RunKind, TimeMachine, VirtualMachine,
};

impl LiveMachine {
    /// Boot a hosted program from its linked artifacts; the session starts running.
    pub fn new_hosted(artifacts: &BuildArtifacts) -> Result<Self, String> {
        let elf = artifacts
            .linked
            .to_elf_with_entry(LinkLayout::hosted().load_base, "_start");
        let vm = VirtualMachine::from_elf(&elf).map_err(|err| format!("ELF load failed: {err}"))?;
        Ok(Self {
            vm,
            state: MachineState::Running,
            kind: RunKind::Hosted,
            steps: 0,
            breakpoints: BTreeSet::new(),
            uart_log: String::new(),
            steps_per_sec: 0.0,
            symbol_table: runtime_symbol_table(&artifacts.linked),
            symbols: artifacts.linked.clone(),
            os_sym: None,
            fs_sym: None,
            resumed_from: None,
            temp_breakpoint: None,
            prev_step_regs: None,
            timeline: TimeMachine::new(50_000, 8, 64),
        })
    }

    /// Boot a bare-metal VM: no hosted runtime, execution starts at the config's
    /// entry symbol (falling back to `_start`).
    pub fn new_freestanding(artifacts: &BuildArtifacts) -> Result<Self, String> {
        let entry = artifacts.plan.entry.as_deref().unwrap_or("_start");
        let elf = artifacts
            .linked
            .to_elf_with_entry(virtual_machine::bus::ELF_LOAD_BASE, entry);
        let vm = VirtualMachine::from_elf(&elf).map_err(|err| format!("ELF load failed: {err}"))?;
        Ok(Self {
            vm,
            state: MachineState::Running,
            kind: RunKind::Freestanding,
            steps: 0,
            breakpoints: BTreeSet::new(),
            uart_log: String::new(),
            steps_per_sec: 0.0,
            symbol_table: runtime_symbol_table(&artifacts.linked),
            symbols: artifacts.linked.clone(),
            os_sym: None,
            fs_sym: None,
            resumed_from: None,
            temp_breakpoint: None,
            prev_step_regs: None,
            timeline: TimeMachine::new(50_000, 8, 64),
        })
    }

    /// Boot a kernel VM from the prepared kernel image and optional boot assets.
    pub fn new_kernel(
        kernel: &BuildArtifacts,
        user_init: Option<&AssembledOutput>,
        fs_image: Option<&[u8]>,
        autorun: &str,
    ) -> Result<Self, String> {
        let mut vm = VirtualMachine::new_kernel(&kernel.linked);
        write_kernel_boot_assets(&mut vm, user_init, fs_image)?;
        for byte in autorun.bytes() {
            vm.push_uart_rx(byte);
        }

        Ok(Self {
            vm,
            state: MachineState::Running,
            kind: RunKind::KernelBoot,
            steps: 0,
            breakpoints: BTreeSet::new(),
            uart_log: String::new(),
            steps_per_sec: 0.0,
            symbol_table: runtime_symbol_table(&kernel.linked),
            symbols: kernel.linked.clone(),
            os_sym: OsSymbols::from_kernel(&kernel.linked),
            fs_sym: FsSymbols::from_kernel(&kernel.linked),
            resumed_from: None,
            temp_breakpoint: None,
            prev_step_regs: None,
            timeline: TimeMachine::new(50_000, 8, 64),
        })
    }

    /// Apply an explicitly requested timing model to this session.
    pub fn set_memory_latency_config(&mut self, config: virtual_machine::bus::MemoryLatencyConfig) {
        self.vm.set_memory_latency_config(config);
    }
}

fn runtime_symbol_table(linked: &AssembledOutput) -> Vec<(u64, String)> {
    let mut table: Vec<(u64, String)> = linked
        .symbols_iter()
        .map(|(name, offset)| (LiveMachine::runtime_address(offset), name.to_owned()))
        .collect();
    table.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    table
}

const FS_META_PA: u64 = 0x87BFF000;
const FS_IMAGE_PA: u64 = 0x87C00000;
const USER_BINARY_PA: u64 = 0x87F00000;
const USER_META_PA: u64 = 0x87EFF000;
pub(super) const USER_CODE_VA: u64 = 0x4000_0000;

fn write_kernel_boot_assets(
    vm: &mut VirtualMachine,
    user: Option<&AssembledOutput>,
    fs_image: Option<&[u8]>,
) -> Result<(), String> {
    if let Some(image) = fs_image {
        vm.write_ram(FS_META_PA, &FS_IMAGE_PA.to_le_bytes())
            .map_err(|err| format!("write FS image address failed: {err:?}"))?;
        vm.write_ram(FS_META_PA + 8, &(image.len() as u64).to_le_bytes())
            .map_err(|err| format!("write FS image size failed: {err:?}"))?;
        vm.write_ram(FS_IMAGE_PA, image)
            .map_err(|err| format!("write FS image failed: {err:?}"))?;
    }

    if let Some(user) = user {
        let mut flat = user.to_flat_binary();
        let page = 4096usize;
        flat.resize(flat.len().div_ceil(page) * page, 0u8);
        let entry_off = user
            .symbol_address("_start")
            .ok_or_else(|| "user init binary is missing `_start`".to_owned())?;
        let entry_va = USER_CODE_VA + entry_off;
        vm.write_ram(USER_META_PA, &entry_va.to_le_bytes())
            .map_err(|err| format!("write user entry failed: {err:?}"))?;
        vm.write_ram(USER_META_PA + 8, &(flat.len() as u64).to_le_bytes())
            .map_err(|err| format!("write user size failed: {err:?}"))?;
        vm.write_ram(USER_BINARY_PA, &flat)
            .map_err(|err| format!("write user binary failed: {err:?}"))?;
    }

    Ok(())
}
