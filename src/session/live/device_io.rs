use super::LiveMachine;

impl LiveMachine {
    /// Feed one byte to the guest UART receive queue.
    pub fn push_uart(&mut self, byte: u8) {
        self.vm.push_uart_rx(byte);
    }

    /// Feed a whole line to the guest UART, terminated with a newline.
    pub fn push_uart_line(&mut self, text: &str) {
        for byte in text.bytes() {
            self.vm.push_uart_rx(byte);
        }
        self.vm.push_uart_rx(b'\n');
    }

    pub fn push_key_event(&mut self, scancode: u16, pressed: bool) {
        self.vm.keyboard_push(scancode, pressed);
    }

    pub fn framebuffer(&self) -> &[u8] {
        self.vm.peek_framebuffer()
    }

    pub fn framebuffer_fill_count(&self) -> u64 {
        self.vm.framebuffer_fill_count()
    }

    /// Visible-pixel change counter; the GUI re-uploads its texture when it moves.
    pub fn framebuffer_version(&self) -> u64 {
        self.vm.framebuffer_content_version()
    }
}

// Minimal terminal discipline: `\b` erases the previous character (guests echo
// "\b \b") and `\r` is dropped.
pub(super) fn append_terminal_bytes(log: &mut String, bytes: &[u8]) {
    if bytes.is_empty() {
        return;
    }
    for ch in String::from_utf8_lossy(bytes).chars() {
        match ch {
            '\u{8}' => {
                if log.chars().next_back().is_some_and(|last| last != '\n') {
                    log.pop();
                }
            }
            '\r' => {}
            _ => log.push(ch),
        }
    }
}
