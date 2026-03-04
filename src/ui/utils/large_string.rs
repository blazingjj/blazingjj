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
        Self {
            content,
            line_start,
        }
    }

    /// Number of lines in content
    pub fn lines(&self) -> usize {
        self.line_start.len()
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
