/*! The LargeString structure is optimized for storing large output of jj
in a way that can be quickly rendered. Normally you could convert the
output to a Text but this require more space. Instead, the LargeString
findes all line breaks, and provide methods for converting only the
visible lines into a Text. */

use ansi_to_tui::IntoText;
use ratatui::text::Text;
use tracing::error;

/// Store a large ANSI colour coded string in a way that allows you
/// to quickly extract a small range and convert it into Text
pub struct LargeString {
    /// The stored string
    content: String,
    /// First byte of each line in content
    line_start: Vec<usize>,
    /// Characters in the widest line
    width: usize,
}

/// The characters `line` puts on screen, leaving out its terminator and
/// any ANSI escape sequences.
fn line_width(line: &str) -> usize {
    let mut width = 0;
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match c {
            '\x1b' => {
                // Past the introducer, everything up to and including the
                // first character in 0x40..=0x7e belongs to the sequence.
                chars.next();
                for c in chars.by_ref() {
                    if ('\x40'..='\x7e').contains(&c) {
                        break;
                    }
                }
            }
            '\n' | '\r' => {}
            _ => width += 1,
        }
    }
    width
}

impl LargeString {
    /// Find line start of all lines
    /// to enable quick rendering of a small range of lines.
    pub fn new(content: String) -> Self {
        // Index content
        let bytes = content.as_bytes();
        let mut line_start = vec![];
        let mut i = 0;
        while i < bytes.len() {
            // Found new line start
            line_start.push(i);
            // Skip all non-EOL chars
            fn is_eol_char(c: u8) -> bool {
                c == b'\n' || c == b'\r'
            }
            while i < bytes.len() && !is_eol_char(bytes[i]) {
                i += 1;
            }
            // If at a pair of CR LF, then skip the first of those
            if i + 1 < bytes.len() && is_eol_char(bytes[i + 1]) && bytes[i] != bytes[i + 1] {
                i += 1;
            }
            // Include the last EOL char in this line
            i += 1;
        }
        // Create object
        let width = line_start
            .iter()
            .zip(line_start.iter().skip(1).chain([&content.len()]))
            .map(|(&start, &end)| line_width(&content[start..end]))
            .max()
            .unwrap_or(0);
        Self {
            content,
            line_start,
            width,
        }
    }

    /// Number of lines in content
    pub fn lines(&self) -> usize {
        self.line_start.len()
    }

    /// Characters in the widest line
    pub fn width(&self) -> usize {
        self.width
    }

    /// Render a range of lines of the content as Text
    pub fn render(&self, top_line: usize, line_count: usize) -> Text<'_> {
        let end_of_content = self.content.len();
        let get_line_start = |line| self.line_start.get(line).copied().unwrap_or(end_of_content);
        let start = get_line_start(top_line);
        let end = get_line_start(top_line + line_count);
        let content_str: &str = &self.content[start..end];
        match content_str.into_text() {
            Ok(text) => text,
            Err(err) => {
                error!("Error converting \"{}\" into ratatui::Text", content_str);
                Text::from(format!("{}", err))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn width_is_that_of_the_widest_line() {
        assert_eq!(LargeString::new("ab\nabcd\nabc".to_owned()).width(), 4);
        assert_eq!(LargeString::new("abcd\r\nab".to_owned()).width(), 4);
        assert_eq!(LargeString::new(String::new()).width(), 0);
    }

    #[test]
    fn width_leaves_out_ansi_escape_sequences() {
        let colored = "\x1b[1;31mError\x1b[0m: nope".to_owned();
        assert_eq!(LargeString::new(colored).width(), "Error: nope".len());
    }
}
