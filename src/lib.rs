//! Dependency-free validation core for a possible native `lint-md` CLI.
//!
//! This is intentionally a small line-oriented prototype, not a Markdown AST
//! replacement. Its purpose is to validate the diagnostic, fix and CLI model
//! before committing to a full parser rewrite.

use std::fmt;

pub const RULE_NO_FULL_WIDTH_NUMBER: &str = "no-full-width-number";
pub const RULE_NO_EMPTY_INLINE_CODE: &str = "no-empty-inline-code";
pub const RULE_NO_EMPTY_BLOCKQUOTE: &str = "no-empty-blockquote";
pub const RULE_NO_MULTIPLE_SPACE_BLOCKQUOTE: &str = "no-multiple-space-blockquote";
pub const RULE_NO_MULTIPLE_BLANK_LINES: &str = "no-multiple-blank-lines";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Error => f.write_str("error"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub rule_id: &'static str,
    pub message: &'static str,
    pub line: usize,
    pub column: usize,
    pub severity: Severity,
    pub fixable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LintResult {
    pub diagnostics: Vec<Diagnostic>,
    pub fixed: String,
    pub changed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Fence {
    marker: char,
    width: usize,
}

/// Lint Markdown using the prototype rule set.
///
/// Diagnostics always describe the original input. `fixed` contains all safe
/// prototype edits applied in a deterministic order.
pub fn lint_markdown(input: &str) -> LintResult {
    let mut diagnostics = Vec::new();
    let mut fixed = String::with_capacity(input.len());
    let mut active_fence: Option<Fence> = None;
    let mut blank_run = 0usize;

    for (line_index, raw_line) in input.split_inclusive('\n').enumerate() {
        process_line(
            raw_line,
            line_index + 1,
            &mut active_fence,
            &mut blank_run,
            &mut diagnostics,
            &mut fixed,
        );
    }

    let changed = fixed != input;
    LintResult {
        diagnostics,
        fixed,
        changed,
    }
}

fn process_line(
    raw_line: &str,
    line_number: usize,
    active_fence: &mut Option<Fence>,
    blank_run: &mut usize,
    diagnostics: &mut Vec<Diagnostic>,
    fixed: &mut String,
) {
    let (line, ending) = split_line_ending(raw_line);
    let trimmed_start = line.trim_start_matches([' ', '\t']);
    let fence = parse_fence(trimmed_start);

    if let Some(current) = active_fence {
        fixed.push_str(line);
        fixed.push_str(ending);
        if fence.is_some_and(|candidate| {
            candidate.marker == current.marker && candidate.width >= current.width
        }) {
            *active_fence = None;
        }
        *blank_run = 0;
        return;
    }

    if let Some(opening) = fence {
        *active_fence = Some(opening);
        fixed.push_str(line);
        fixed.push_str(ending);
        *blank_run = 0;
        return;
    }

    if line.trim().is_empty() {
        *blank_run += 1;
        if *blank_run > 1 {
            diagnostics.push(Diagnostic {
                rule_id: RULE_NO_MULTIPLE_BLANK_LINES,
                message: "Multiple consecutive blank lines are not allowed.",
                line: line_number,
                column: 1,
                severity: Severity::Error,
                fixable: true,
            });
            return;
        }
        fixed.push_str(line);
        fixed.push_str(ending);
        return;
    }
    *blank_run = 0;

    diagnose_empty_blockquote(line, line_number, diagnostics);
    diagnose_blockquote_spacing(line, line_number, diagnostics);
    diagnose_empty_inline_code(line, line_number, diagnostics);
    diagnose_full_width_numbers(line, line_number, diagnostics);

    let mut transformed = fix_blockquote_spacing(line);
    transformed = fix_empty_inline_code(&transformed);
    transformed = fix_full_width_numbers(&transformed);

    fixed.push_str(&transformed);
    fixed.push_str(ending);
}

fn split_line_ending(raw_line: &str) -> (&str, &str) {
    if let Some(without_lf) = raw_line.strip_suffix('\n') {
        if let Some(without_crlf) = without_lf.strip_suffix('\r') {
            (without_crlf, "\r\n")
        } else {
            (without_lf, "\n")
        }
    } else {
        (raw_line, "")
    }
}

fn parse_fence(line: &str) -> Option<Fence> {
    let marker = line.chars().next()?;
    if marker != '`' && marker != '~' {
        return None;
    }
    let width = line.chars().take_while(|ch| *ch == marker).count();
    (width >= 3).then_some(Fence { marker, width })
}

fn leading_whitespace_bytes(line: &str) -> usize {
    line.char_indices()
        .find_map(|(index, ch)| (!matches!(ch, ' ' | '\t')).then_some(index))
        .unwrap_or(line.len())
}

fn char_column(line: &str, byte_index: usize) -> usize {
    line[..byte_index].chars().count() + 1
}

fn blockquote_parts(line: &str) -> Option<(usize, &str)> {
    let prefix = leading_whitespace_bytes(line);
    let remainder = &line[prefix..];
    remainder
        .strip_prefix('>')
        .map(|after_marker| (prefix, after_marker))
}

fn diagnose_empty_blockquote(line: &str, line_number: usize, diagnostics: &mut Vec<Diagnostic>) {
    if let Some((marker_index, after_marker)) = blockquote_parts(line) {
        if after_marker.trim().is_empty() {
            diagnostics.push(Diagnostic {
                rule_id: RULE_NO_EMPTY_BLOCKQUOTE,
                message: "Blockquote content must not be empty.",
                line: line_number,
                column: char_column(line, marker_index),
                severity: Severity::Error,
                fixable: false,
            });
        }
    }
}

fn diagnose_blockquote_spacing(line: &str, line_number: usize, diagnostics: &mut Vec<Diagnostic>) {
    if let Some((marker_index, after_marker)) = blockquote_parts(line) {
        let spaces = after_marker.chars().take_while(|ch| *ch == ' ').count();
        if spaces > 1 {
            diagnostics.push(Diagnostic {
                rule_id: RULE_NO_MULTIPLE_SPACE_BLOCKQUOTE,
                message: "Use exactly one space after the blockquote marker.",
                line: line_number,
                column: char_column(line, marker_index) + 1,
                severity: Severity::Error,
                fixable: true,
            });
        }
    }
}

fn fix_blockquote_spacing(line: &str) -> String {
    let Some((marker_index, after_marker)) = blockquote_parts(line) else {
        return line.to_owned();
    };
    let spaces = after_marker.chars().take_while(|ch| *ch == ' ').count();
    if spaces <= 1 {
        return line.to_owned();
    }

    let consumed_bytes: usize = after_marker.chars().take(spaces).map(char::len_utf8).sum();
    let mut output = String::with_capacity(line.len() - consumed_bytes + 1);
    output.push_str(&line[..marker_index]);
    output.push('>');
    output.push(' ');
    output.push_str(&after_marker[consumed_bytes..]);
    output
}

fn diagnose_empty_inline_code(line: &str, line_number: usize, diagnostics: &mut Vec<Diagnostic>) {
    for byte_index in exact_double_backticks(line) {
        diagnostics.push(Diagnostic {
            rule_id: RULE_NO_EMPTY_INLINE_CODE,
            message: "Inline code content must not be empty.",
            line: line_number,
            column: char_column(line, byte_index),
            severity: Severity::Error,
            fixable: true,
        });
    }
}

fn exact_double_backticks(line: &str) -> Vec<usize> {
    let bytes = line.as_bytes();
    let mut result = Vec::new();
    let mut index = 0usize;
    while index + 1 < bytes.len() {
        if bytes[index] == b'`' && bytes[index + 1] == b'`' {
            let previous_is_tick = index > 0 && bytes[index - 1] == b'`';
            let next_is_tick = index + 2 < bytes.len() && bytes[index + 2] == b'`';
            let escaped = index > 0 && bytes[index - 1] == b'\\';
            if !previous_is_tick && !next_is_tick && !escaped {
                result.push(index);
                index += 2;
                continue;
            }
        }
        index += 1;
    }
    result
}

fn fix_empty_inline_code(line: &str) -> String {
    let targets = exact_double_backticks(line);
    if targets.is_empty() {
        return line.to_owned();
    }

    let mut output = String::with_capacity(line.len().saturating_sub(targets.len() * 2));
    let mut cursor = 0usize;
    for target in targets {
        output.push_str(&line[cursor..target]);
        cursor = target + 2;
    }
    output.push_str(&line[cursor..]);
    output
}

fn diagnose_full_width_numbers(line: &str, line_number: usize, diagnostics: &mut Vec<Diagnostic>) {
    visit_text_characters(line, |byte_index, ch| {
        if is_full_width_digit(ch) {
            diagnostics.push(Diagnostic {
                rule_id: RULE_NO_FULL_WIDTH_NUMBER,
                message: "Use half-width ASCII digits.",
                line: line_number,
                column: char_column(line, byte_index),
                severity: Severity::Error,
                fixable: true,
            });
        }
    });
}

fn fix_full_width_numbers(line: &str) -> String {
    let mut output = String::with_capacity(line.len());
    let mut cursor = 0usize;
    visit_text_characters(line, |byte_index, ch| {
        if let Some(ascii) = to_ascii_digit(ch) {
            output.push_str(&line[cursor..byte_index]);
            output.push(ascii);
            cursor = byte_index + ch.len_utf8();
        }
    });
    if cursor == 0 {
        return line.to_owned();
    }
    output.push_str(&line[cursor..]);
    output
}

fn visit_text_characters(mut line: &str, mut visitor: impl FnMut(usize, char)) {
    let mut absolute_offset = 0usize;
    let mut active_delimiter: Option<usize> = None;

    while !line.is_empty() {
        let mut chars = line.char_indices();
        let (_, ch) = chars.next().expect("non-empty string has a character");
        if ch == '`' {
            let run = line
                .chars()
                .take_while(|candidate| *candidate == '`')
                .count();
            match active_delimiter {
                None => active_delimiter = Some(run),
                Some(width) if width == run => active_delimiter = None,
                Some(_) => {}
            }
            absolute_offset += run;
            line = &line[run..];
            continue;
        }

        if active_delimiter.is_none() {
            visitor(absolute_offset, ch);
        }
        let width = ch.len_utf8();
        absolute_offset += width;
        line = &line[width..];
    }
}

fn is_full_width_digit(ch: char) -> bool {
    matches!(ch, '０'..='９')
}

fn to_ascii_digit(ch: char) -> Option<char> {
    if !is_full_width_digit(ch) {
        return None;
    }
    let offset = ch as u32 - '０' as u32;
    char::from_u32('0' as u32 + offset)
}

pub fn json_escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 8);
    for ch in value.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            ch if ch <= '\u{1f}' => {
                use std::fmt::Write as _;
                write!(&mut escaped, "\\u{:04x}", ch as u32)
                    .expect("writing to String cannot fail");
            }
            _ => escaped.push(ch),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixes_full_width_numbers_but_skips_code() {
        let input = "版本１２ and `３４`\n```text\n５６\n```\n";
        let result = lint_markdown(input);
        assert_eq!(result.fixed, "版本12 and `３４`\n```text\n５６\n```\n");
        assert_eq!(
            result
                .diagnostics
                .iter()
                .filter(|item| item.rule_id == RULE_NO_FULL_WIDTH_NUMBER)
                .count(),
            2
        );
    }

    #[test]
    fn removes_extra_blank_lines_and_preserves_crlf() {
        let input = "first\r\n\r\n\r\nsecond\r\n";
        let result = lint_markdown(input);
        assert_eq!(result.fixed, "first\r\n\r\nsecond\r\n");
        assert!(result
            .diagnostics
            .iter()
            .any(|item| item.rule_id == RULE_NO_MULTIPLE_BLANK_LINES));
    }

    #[test]
    fn fixes_blockquote_spacing_but_reports_empty_blockquote() {
        let input = ">   text\n>\n";
        let result = lint_markdown(input);
        assert_eq!(result.fixed, "> text\n>\n");
        assert!(result
            .diagnostics
            .iter()
            .any(|item| item.rule_id == RULE_NO_MULTIPLE_SPACE_BLOCKQUOTE));
        assert!(result
            .diagnostics
            .iter()
            .any(|item| item.rule_id == RULE_NO_EMPTY_BLOCKQUOTE && !item.fixable));
    }

    #[test]
    fn removes_exact_empty_inline_code_only() {
        let input = "bad `` but keep ``` and \\`` escaped\n";
        let result = lint_markdown(input);
        assert_eq!(result.fixed, "bad  but keep ``` and \\`` escaped\n");
        assert_eq!(
            result
                .diagnostics
                .iter()
                .filter(|item| item.rule_id == RULE_NO_EMPTY_INLINE_CODE)
                .count(),
            1
        );
    }

    #[test]
    fn leaves_fenced_content_untouched() {
        let input = "~~~md\n>   １２\n\n\n~~~\n";
        let result = lint_markdown(input);
        assert_eq!(result.fixed, input);
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn escapes_json_control_characters() {
        assert_eq!(json_escape("a\n\"b\\"), "a\\n\\\"b\\\\");
    }
}
