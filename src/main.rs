use crossterm::{
    cursor::{MoveTo, Show},
    event::{Event, KeyCode, KeyEventKind, read},
    execute,
    terminal::{
        Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode,
        enable_raw_mode,
    },
};
use std::fmt;
use std::{
    io::{self, Write, stdout},
    vec,
};
struct GapBuffer {
    buffer: Vec<u8>,
    gap_start: usize,
    gap_end: usize,
}

trait Editor {
    fn new(capacity: usize) -> Self;
}

impl Editor for GapBuffer {
    fn new(capacity: usize) -> Self {
        GapBuffer {
            buffer: vec![0; capacity],
            gap_start: (0),
            gap_end: (capacity),
        }
    }
}

impl GapBuffer {
    fn grow(&mut self, min_extra: usize) {
        let old_len = self.buffer.len();
        let needed = (old_len - (self.gap_end - self.gap_start)) + min_extra.max(8);
        let new_len = needed.next_power_of_two().max(old_len * 2).max(8);
        let extra = new_len - old_len;

        self.buffer.resize(new_len, 0xAA);
        self.buffer
            .copy_within(self.gap_end..old_len, self.gap_end + extra);
        self.gap_end += extra;
    }
    fn store(&mut self, c: char) {
        let mut buffer = [0u8; 4];
        let bytes = c.encode_utf8(&mut buffer).as_bytes();

        if self.gap_end - self.gap_start < bytes.len() {
            self.grow(bytes.len());
        }

        for &b in bytes {
            self.buffer[self.gap_start] = b;
            self.gap_start += 1;
        }
    }

    fn visible_len(&self) -> usize {
        self.gap_start + (self.buffer.len() - self.gap_end)
    }

    // read a byte at a logical position (0..visible_len), skipping over the gap
    fn byte_at(&self, pos: usize) -> u8 {
        if pos < self.gap_start {
            self.buffer[pos]
        } else {
            self.buffer[pos + (self.gap_end - self.gap_start)]
        }
    }

    fn line_start(&self, pos: usize) -> usize {
        let mut i = pos;
        while i > 0 {
            if self.byte_at(i - 1) == b'\n' {
                return i;
            }
            i -= 1;
        }
        0
    }

    fn line_end(&self, pos: usize) -> usize {
        let len = self.visible_len();
        let mut i = pos;
        while i < len {
            if self.byte_at(i) == b'\n' {
                return i;
            }
            i += 1;
        }
        len
    }

    fn next_line(&mut self) {
        let next = '\n';
        self.store(next);
    }

    fn move_gap_to(&mut self, position: usize) {
        debug_assert!(
            self.is_char_boundary(position),
            "pos must land on a char boundary"
        );

        if position < self.gap_start {
            let count = self.gap_start - position;
            self.buffer
                .copy_within(position..self.gap_start, self.gap_end - count);
            self.gap_start -= count;
            self.gap_end -= count;
        } else if position > self.gap_start {
            let count = position - self.gap_start;
            self.buffer
                .copy_within(self.gap_end..self.gap_end + count, self.gap_start);
            self.gap_start += count;
            self.gap_end += count;
        }
    }

    fn is_char_boundary(&self, pos: usize) -> bool {
        // byte_at each side and check it's not a UTF-8 continuation byte (0b10xxxxxx)
        if pos == 0 || pos == self.visible_len() {
            return true;
        }
        (self.byte_at(pos) & 0b1100_0000) != 0b1000_0000
    }

    fn backspace(&mut self) {
        if self.gap_start == 0 {
            return;
        }

        // find the previous char boundary (could be 1-4 bytes back)
        let mut new_start = self.gap_start - 1;
        while new_start > 0 && !self.is_char_boundary(new_start) {
            new_start -= 1;
        }

        self.gap_start = new_start; // gap just grew backward — deleted bytes are now inside the gap
    }

    fn move_up(&mut self) {
        let cur = self.gap_start;
        let cur_line_start = self.line_start(cur);
        let col = cur - cur_line_start;

        if cur_line_start == 0 {
            return; // already on first line
        }

        let prev_line_end = cur_line_start - 1; // the '\n' before this line
        let prev_line_start = self.line_start(prev_line_end);
        let prev_line_len = prev_line_end - prev_line_start;

        self.move_gap_to(prev_line_start + col.min(prev_line_len));
    }

    fn move_down(&mut self) {
        let cur = self.gap_start;
        let cur_line_start = self.line_start(cur);
        let col = cur - cur_line_start;

        let cur_line_end = self.line_end(cur);
        let len = self.visible_len();
        if cur_line_end >= len {
            return; // already on last line
        }

        let next_line_start = cur_line_end + 1; // skip past '\n'
        let next_line_end = self.line_end(next_line_start);
        let next_line_len = next_line_end - next_line_start;

        self.move_gap_to(next_line_start + col.min(next_line_len));
    }
}

fn render(gb: &GapBuffer) -> io::Result<()> {
    let mut out = stdout();
    execute!(out, Clear(ClearType::All), MoveTo(0, 0))?;

    write!(out, "{}", gb)?;

    // compute (row, col) for the cursor from gap_start
    let cur_line_start = gb.line_start(gb.gap_start);
    let col = gb.gap_start - cur_line_start;

    // count how many '\n' occur before gap_start to get the row number
    let row = (0..gb.gap_start)
        .filter(|&i| gb.byte_at(i) == b'\n')
        .count();

    execute!(out, MoveTo(col as u16, row as u16))?;
    out.flush()
}

impl fmt::Display for GapBuffer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            std::str::from_utf8(&self.buffer[..self.gap_start]).unwrap()
        )?;
        write!(
            f,
            "{}",
            std::str::from_utf8(&self.buffer[self.gap_end..]).unwrap()
        )
    }
}

fn main() -> io::Result<()> {
    let mut gb = GapBuffer::new(8);

    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen, Show)?;
    render(&gb)?;
    loop {
        match read()? {
            Event::Key(key_event) => {
                if key_event.kind != KeyEventKind::Press {
                    continue;
                }

                match key_event.code {
                    KeyCode::Char(c) => {
                        gb.store(c);
                    }
                    KeyCode::Left => {
                        let cur = gb.gap_start;
                        if cur > 0 {
                            let mut new_pos = cur - 1;
                            while new_pos > 0 && !gb.is_char_boundary(new_pos) {
                                new_pos -= 1;
                            }
                            gb.move_gap_to(new_pos);
                        }
                    }
                    KeyCode::Right => {
                        let cur = gb.gap_start;
                        let len = gb.visible_len();
                        if cur < len {
                            let mut new_pos = cur + 1;
                            while new_pos < len && !gb.is_char_boundary(new_pos) {
                                new_pos += 1;
                            }
                            gb.move_gap_to(new_pos);
                        }
                    }
                    KeyCode::Backspace => {
                        gb.backspace();
                    }
                    KeyCode::Enter => gb.next_line(),
                    KeyCode::Up => gb.move_up(),
                    KeyCode::Down => gb.move_down(),

                    KeyCode::Esc => break,
                    _ => {}
                }
                render(&gb)?;
            }
            _ => {}
        }
    }
    execute!(stdout(), Show, LeaveAlternateScreen)?;
    disable_raw_mode()?;
    Ok(())
}
