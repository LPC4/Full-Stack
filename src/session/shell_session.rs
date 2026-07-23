#[derive(Clone, Debug, Default)]
pub struct ShellSession {
    input_buffer: String,
    commands: Vec<ShellCommandRecord>,
    next_id: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShellCommandRecord {
    pub id: u64,
    pub raw: String,
    pub parsed: ParsedCommand,
    pub exit_code: Option<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParsedCommand {
    pub program: String,
    pub argv: Vec<String>,
    pub background: bool,
    pub stdout: Option<Redirect>,
    pub stdin: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Redirect {
    pub path: String,
    pub append: bool,
}

impl ShellSession {
    fn observe_input_byte(&mut self, byte: u8) {
        match byte {
            b'\r' | b'\n' => self.commit_input(),
            0x08 => {
                self.input_buffer.pop();
            }
            0x03 => {
                self.input_buffer.clear();
            }
            b if b.is_ascii_graphic() || b == b' ' || b == b'\t' => {
                self.input_buffer.push(b as char);
            }
            _ => {}
        }
    }

    pub fn observe_input_bytes(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.observe_input_byte(byte);
        }
    }

    fn record_command(&mut self, raw: &str) {
        let raw = raw.trim();
        if raw.is_empty() {
            return;
        }
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.commands.push(ShellCommandRecord {
            id,
            raw: raw.to_owned(),
            parsed: ParsedCommand::parse(raw),
            exit_code: None,
        });
    }

    pub fn commands(&self) -> &[ShellCommandRecord] {
        &self.commands
    }

    pub fn clear(&mut self) {
        self.input_buffer.clear();
        self.commands.clear();
        self.next_id = 0;
    }

    fn commit_input(&mut self) {
        let raw = std::mem::take(&mut self.input_buffer);
        self.record_command(&raw);
    }
}

impl ParsedCommand {
    pub fn parse(raw: &str) -> Self {
        let mut tokens = tokenize(raw);
        let background = matches!(tokens.last().map(String::as_str), Some("&"));
        if background {
            tokens.pop();
        }

        let mut argv = Vec::new();
        let mut stdout = None;
        let mut stdin = None;
        let mut iter = tokens.into_iter();
        while let Some(token) = iter.next() {
            match token.as_str() {
                ">" | ">>" => {
                    if let Some(path) = iter.next() {
                        stdout = Some(Redirect {
                            path,
                            append: token == ">>",
                        });
                    }
                }
                "<" => {
                    stdin = iter.next();
                }
                _ => argv.push(token),
            }
        }

        let program = argv.first().cloned().unwrap_or_default();
        Self {
            program,
            argv,
            background,
            stdout,
            stdin,
        }
    }
}

fn tokenize(raw: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    for ch in raw.chars() {
        match (ch, quote) {
            ('\'' | '"', None) => quote = Some(ch),
            (q, Some(active)) if q == active => quote = None,
            (' ' | '\t', None) => {
                if !current.is_empty() {
                    out.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(ch),
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_argv_redirects_and_background_marker() {
        let parsed = ParsedCommand::parse("cat 'a b.txt' >> out.txt &");
        assert_eq!(parsed.program, "cat");
        assert_eq!(parsed.argv, vec!["cat", "a b.txt"]);
        assert_eq!(
            parsed.stdout,
            Some(Redirect {
                path: "out.txt".to_owned(),
                append: true
            })
        );
        assert!(parsed.background);
    }

    #[test]
    fn session_commits_terminal_input_on_enter() {
        let mut session = ShellSession::default();
        session.observe_input_bytes(b"ls /bin\r");
        assert_eq!(session.commands().len(), 1);
        assert_eq!(session.commands()[0].parsed.argv, vec!["ls", "/bin"]);
    }

    #[test]
    fn backspace_updates_pending_command() {
        let mut session = ShellSession::default();
        session.observe_input_bytes(b"echx\x08o hi\r");
        assert_eq!(session.commands()[0].raw, "echo hi");
    }
}
