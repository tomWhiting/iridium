//! A resumable LF / CRLF / CR scanner; CR is resolved by the following byte.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct Scan {
    pub offset: u64,
    pub line: u64,
    pub start: u64,
    pub pending_cr: bool,
}
impl Scan {
    pub const fn feed(&mut self, byte: u8) {
        if self.pending_cr {
            self.pending_cr = false;
            if byte == b'\n' {
                self.offset += 1;
                self.new_line();
                return;
            }
            self.new_line();
        }
        self.offset += 1;
        match byte {
            b'\r' => self.pending_cr = true,
            b'\n' => self.new_line(),
            _ => {},
        }
    }
    pub const fn finish(&mut self) {
        if self.pending_cr {
            self.pending_cr = false;
            self.new_line();
        }
    }
    const fn new_line(&mut self) {
        self.line += 1;
        self.start = self.offset;
    }
}
