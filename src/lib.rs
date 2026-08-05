//! Rust validation core for a possible native `lint-md` CLI.
//!
//! The prototype combines lightweight scanners for text-oriented rules with
//! mdast positions for rules that depend on Markdown parsing semantics.

use markdown::{
    mdast::{InlineCode, Node},
    to_mdast, ParseOptions,
};
use std::{fmt, ops::Range};

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

#[derive(Debug, Clone, Copy)]
struct SourceLine<'a> {
    content: &'a str,
    ending: &'a str,
    number: usize,
    protected: bool,
}

/// Lint Markdown using the prototype rule set.
///
/// Diagnostics describe the original input. Empty inline-code fixes use mdast
/// source ranges; scanner fixes then repeat until stable so interacting rules
/// converge on the TypeScript reference output.
pub fn lint_markdown(input: &str) -> LintResult {
    let mut diagnostics = Vec::new();

    diagnose_blank_lines(input, &mut diagnostics);
    diagnose_line_rules(input, &mut diagnostics);
    let inline_code_removals = diagnose_empty_inline_code(input, &mut diagnostics);

    let mut fixed = apply_removals(input, &inline_code_removals);
    fixed = fix_line_rules(&fixed);
    fixed = fix_blank_lines(&fixed);

    if fixed != input {
        for _ in 0..3 {
            let mut next = fix_line_rules(&fixed);
            next = fix_blank_lines(&next);
            if next == fixed {
                break;
            }
            fixed = next;
        }
    }

    LintResult {
        diagnostics,
        changed: fixed != input,
        fixed,
    }
}

fn parse_lines(input: &str) -> Vec<SourceLine<'_>> {
    let mut lines = Vec::new();
    let bytes = input.as_bytes();
    let mut start = 0usize;
    let mut number = 1usize;

    while start < bytes.len() {
        let mut end = start;
        while end < bytes.len() && !matches!(bytes[end], b'\r' | b'\n') {
            end += 1;
        }

        let ending_end = if end == bytes.len() {
            end
        } else if bytes[end] == b'\r' && end + 1 < bytes.len() && bytes[end + 1] == b'\n' {
            end + 2
        } else {
            end + 1
        };

        lines.push(SourceLine {
            content: &input[start..end],
            ending: &input[end..ending_end],
            number,
            protected: false,
        });

        start = ending_end;
        number += 1;
    }

    lines
}

fn mark_protected_lines(lines: &mut [SourceLine<'_>]) {
    let mut active_fence: Option<Fence> = None;

    for line in lines {
        let candidate = parse_fence(line.content.trim_start_matches([' ', '\t']));
        if let Some(current) = active_fence {
            line.protected = true;
            if candidate
                .is_some_and(|fence| fence.marker == current.marker && fence.width >= current.width)
            {
                active_fence = None;
            }
            continue;
        }

        if let Some(opening) = candidate {
            line.protected = true;
            active_fence = Some(opening);
        }
    }
}

fn is_blank_line(line: SourceLine<'_>) -> bool {
    !line.protected && line.content.trim_matches([' ', '\t']).is_empty()
}

fn diagnose_blank_lines(input: &str, diagnostics: &mut Vec<Diagnostic>) {
    if input.is_empty() {
        return;
    }

    if input.chars().all(|ch| matches!(ch, ' ' | '\t')) {
        diagnostics.push(blank_line_diagnostic(
            1,
            1,
            "Whitespace-only documents should be empty.",
        ));
        return;
    }

    let mut lines = parse_lines(input);
    mark_protected_lines(&mut lines);
    if lines.is_empty() {
        return;
    }

    let mut index = 0usize;
    while index < lines.len() && is_blank_line(lines[index]) {
        index += 1;
    }
    if index > 0 {
        diagnostics.push(blank_line_diagnostic(
            1,
            1,
            "Documents must not start with blank lines.",
        ));
    }

    while index < lines.len() {
        if is_blank_line(lines[index]) {
            index += 1;
            continue;
        }

        let previous = lines[index];
        index += 1;
        let blank_start = index;
        while index < lines.len() && is_blank_line(lines[index]) {
            index += 1;
        }
        let run = index - blank_start;
        if run == 0 {
            continue;
        }

        let column = previous.content.chars().count() + 1;
        if index == lines.len() {
            diagnostics.push(blank_line_diagnostic(
                previous.number,
                column,
                "Documents must end with at most one newline.",
            ));
        } else if run > 1 {
            diagnostics.push(blank_line_diagnostic(
                previous.number,
                column,
                "Consecutive blank lines are not allowed.",
            ));
        }
    }
}

fn blank_line_diagnostic(line: usize, column: usize, message: &'static str) -> Diagnostic {
    Diagnostic {
        rule_id: RULE_NO_MULTIPLE_BLANK_LINES,
        message,
        line,
        column,
        severity: Severity::Error,
        fixable: true,
    }
}

fn fix_blank_lines(input: &str) -> String {
    if input.is_empty() {
        return String::new();
    }
    if input.chars().all(|ch| matches!(ch, ' ' | '\t')) {
        return String::new();
    }

    let mut lines = parse_lines(input);
    mark_protected_lines(&mut lines);
    if lines.is_empty() {
        return input.to_owned();
    }

    let mut output = String::with_capacity(input.len());
    let mut index = 0usize;
    while index < lines.len() && is_blank_line(lines[index]) {
        index += 1;
    }

    while index < lines.len() {
        let line = lines[index];
        if is_blank_line(line) {
            index += 1;
            continue;
        }

        output.push_str(line.content);
        output.push_str(line.ending);
        index += 1;

        let blank_start = index;
        while index < lines.len() && is_blank_line(lines[index]) {
            index += 1;
        }
        let run = index - blank_start;
        if run == 0 || index == lines.len() {
            continue;
        }

        if run == 1 {
            output.push_str(lines[blank_start].content);
            output.push_str(lines[blank_start].ending);
        } else {
            output.push_str(line.ending);
        }
    }

    output
}

fn diagnose_line_rules(input: &str, diagnostics: &mut Vec<Diagnostic>) {
    let mut active_fence: Option<Fence> = None;

    for line in parse_lines(input) {
        let candidate = parse_fence(line.content.trim_start_matches([' ', '\t']));
        if let Some(current) = active_fence {
            if candidate
                .is_some_and(|fence| fence.marker == current.marker && fence.width >= current.width)
            {
                active_fence = None;
            }
            continue;
        }

        if let Some(opening) = candidate {
            active_fence = Some(opening);
            continue;
        }

        if line.content.trim_matches([' ', '\t']).is_empty() {
            continue;
        }

        diagnose_empty_blockquote(line.content, line.number, diagnostics);
        diagnose_blockquote_spacing(line.content, line.number, diagnostics);
        diagnose_full_width_numbers(line.content, line.number, diagnostics);
    }
}

fn fix_line_rules(input: &str) -> String {
    let mut fixed = String::with_capacity(input.len());
    let mut active_fence: Option<Fence> = None;

    for line in parse_lines(input) {
        let candidate = parse_fence(line.content.trim_start_matches([' ', '\t']));
        if let Some(current) = active_fence {
            fixed.push_str(line.content);
            fixed.push_str(line.ending);
            if candidate
                .is_some_and(|fence| fence.marker == current.marker && fence.width >= current.width)
            {
                active_fence = None;
            }
            continue;
        }

        if let Some(opening) = candidate {
            active_fence = Some(opening);
            fixed.push_str(line.content);
            fixed.push_str(line.ending);
            continue;
        }

        if line.content.trim_matches([' ', '\t']).is_empty() {
            fixed.push_str(line.content);
            fixed.push_str(line.ending);
            continue;
        }

        let mut transformed = fix_empty_blockquote(line.content);
        if !transformed.is_empty() {
            transformed = fix_blockquote_spacing(&transformed);
            transformed = fix_full_width_numbers(&transformed);
        }

        fixed.push_str(&transformed);
        fixed.push_str(line.ending);
    }

    fixed
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
    line[prefix..]
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
                fixable: true,
            });
        }
    }
}

fn fix_empty_blockquote(line: &str) -> String {
    if blockquote_parts(line).is_some_and(|(_, after_marker)| after_marker.trim().is_empty()) {
        String::new()
    } else {
        line.to_owned()
    }
}

fn diagnose_blockquote_spacing(line: &str, line_number: usize, diagnostics: &mut Vec<Diagnostic>) {
    if let Some((marker_index, after_marker)) = blockquote_parts(line) {
        if after_marker.trim().is_empty() {
            return;
        }
        let spaces = after_marker.chars().take_while(|ch| *ch == ' ').count();
        if spaces != 1 {
            diagnostics.push(Diagnostic {
                rule_id: RULE_NO_MULTIPLE_SPACE_BLOCKQUOTE,
                message: "Use exactly one space after the blockquote marker.",
                line: line_number,
                column: char_column(line, marker_index),
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
    if after_marker.trim().is_empty() {
        return line.to_owned();
    }

    let spaces = after_marker.chars().take_while(|ch| *ch == ' ').count();
    if spaces == 1 {
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

fn diagnose_empty_inline_code(input: &str, diagnostics: &mut Vec<Diagnostic>) -> Vec<Range<usize>> {
    if !input.as_bytes().contains(&b'`') {
        return Vec::new();
    }

    let tree = to_mdast(input, &ParseOptions::default())
        .expect("CommonMark parsing without MDX extensions should not fail");
    let mut removals = Vec::new();
    collect_empty_inline_code(&tree, diagnostics, &mut removals);
    removals.sort_unstable_by_key(|range| range.start);
    removals
}

fn collect_empty_inline_code(
    node: &Node,
    diagnostics: &mut Vec<Diagnostic>,
    removals: &mut Vec<Range<usize>>,
) {
    if let Node::InlineCode(inline_code) = node {
        collect_empty_inline_code_node(inline_code, diagnostics, removals);
    }

    if let Some(children) = node.children() {
        for child in children {
            collect_empty_inline_code(child, diagnostics, removals);
        }
    }
}

fn collect_empty_inline_code_node(
    inline_code: &InlineCode,
    diagnostics: &mut Vec<Diagnostic>,
    removals: &mut Vec<Range<usize>>,
) {
    if !inline_code.value.trim().is_empty() {
        return;
    }
    let Some(position) = &inline_code.position else {
        return;
    };

    diagnostics.push(Diagnostic {
        rule_id: RULE_NO_EMPTY_INLINE_CODE,
        message: "Inline code content must not be empty.",
        line: position.start.line,
        column: position.start.column,
        severity: Severity::Error,
        fixable: true,
    });
    removals.push(position.start.offset..position.end.offset);
}

fn apply_removals(input: &str, removals: &[Range<usize>]) -> String {
    if removals.is_empty() {
        return input.to_owned();
    }

    let removed_bytes: usize = removals.iter().map(|range| range.len()).sum();
    let mut output = String::with_capacity(input.len().saturating_sub(removed_bytes));
    let mut cursor = 0usize;

    for range in removals {
        debug_assert!(range.start >= cursor);
        debug_assert!(range.end <= input.len());
        output.push_str(&input[cursor..range.start]);
        cursor = range.end;
    }

    output.push_str(&input[cursor..]);
    output
}

fn diagnose_full_width_numbers(line: &str, line_number: usize, diagnostics: &mut Vec<Diagnostic>) {
    for byte_index in full_width_digit_run_starts(line) {
        diagnostics.push(Diagnostic {
            rule_id: RULE_NO_FULL_WIDTH_NUMBER,
            message: "Use half-width ASCII digits.",
            line: line_number,
            column: char_column(line, byte_index),
            severity: Severity::Error,
            fixable: true,
        });
    }
}

fn full_width_digit_run_starts(line: &str) -> Vec<usize> {
    let mut starts = Vec::new();
    let mut active_delimiter: Option<usize> = None;
    let mut digit_run: Option<usize> = None;
    let mut byte_index = 0usize;

    while byte_index < line.len() {
        let ch = line[byte_index..]
            .chars()
            .next()
            .expect("byte index is on a character boundary");

        if ch == '`' {
            if let Some(start) = digit_run.take() {
                starts.push(start);
            }
            let run = line[byte_index..]
                .chars()
                .take_while(|candidate| *candidate == '`')
                .count();
            match active_delimiter {
                None => active_delimiter = Some(run),
                Some(width) if width == run => active_delimiter = None,
                Some(_) => {}
            }
            byte_index += run;
            continue;
        }

        if active_delimiter.is_none() && is_full_width_digit(ch) {
            if digit_run.is_none() {
                digit_run = Some(byte_index);
            }
        } else if let Some(start) = digit_run.take() {
            starts.push(start);
        }
        byte_index += ch.len_utf8();
    }

    if let Some(start) = digit_run {
        starts.push(start);
    }
    starts
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
        let ch = line
            .chars()
            .next()
            .expect("non-empty string has a character");
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
    fn groups_full_width_numbers_and_skips_code() {
        let input = "版本１２ and `３４`\n```text\n５６\n```\n";
        let result = lint_markdown(input);
        assert_eq!(result.fixed, "版本12 and `３４`\n```text\n５６\n```\n");
        assert_eq!(
            result
                .diagnostics
                .iter()
                .filter(|item| item.rule_id == RULE_NO_FULL_WIDTH_NUMBER)
                .count(),
            1
        );
    }

    #[test]
    fn normalizes_blank_line_runs_and_preserves_crlf() {
        let input = "\r\nfirst\r\n\r\n\r\nsecond\r\n\r\n";
        let result = lint_markdown(input);
        assert_eq!(result.fixed, "first\r\n\r\nsecond\r\n");
        assert_eq!(
            result
                .diagnostics
                .iter()
                .filter(|item| item.rule_id == RULE_NO_MULTIPLE_BLANK_LINES)
                .count(),
            3
        );
    }

    #[test]
    fn fixes_blockquote_spacing_and_removes_empty_blockquote() {
        let input = ">   text\n>missing\n>\n";
        let result = lint_markdown(input);
        assert_eq!(result.fixed, "> text\n> missing\n");
        assert_eq!(
            result
                .diagnostics
                .iter()
                .filter(|item| item.rule_id == RULE_NO_MULTIPLE_SPACE_BLOCKQUOTE)
                .count(),
            2
        );
        assert!(result
            .diagnostics
            .iter()
            .any(|item| item.rule_id == RULE_NO_EMPTY_BLOCKQUOTE && item.fixable));
    }

    #[test]
    fn ignores_unmatched_empty_backtick_run() {
        let input = "before `` after\n";
        let result = lint_markdown(input);
        assert_eq!(result.fixed, input);
        assert!(!result
            .diagnostics
            .iter()
            .any(|item| item.rule_id == RULE_NO_EMPTY_INLINE_CODE));
    }

    #[test]
    fn removes_whitespace_only_inline_code_by_mdast_range() {
        let input = "`        `\n";
        let result = lint_markdown(input);
        assert_eq!(result.fixed, "");
        let diagnostic = result
            .diagnostics
            .iter()
            .find(|item| item.rule_id == RULE_NO_EMPTY_INLINE_CODE)
            .expect("empty inline code is diagnosed");
        assert_eq!((diagnostic.line, diagnostic.column), (1, 1));
        assert!(diagnostic.fixable);
    }

    #[test]
    fn respects_matching_backtick_delimiter_widths() {
        let input = "keep `` ` `` and remove ` `\n";
        let result = lint_markdown(input);
        assert_eq!(result.fixed, "keep `` ` `` and remove \n");
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
    fn uses_mdast_line_and_column_after_crlf() {
        let input = "first\r\ntext ` `\r\n";
        let result = lint_markdown(input);
        let diagnostic = result
            .diagnostics
            .iter()
            .find(|item| item.rule_id == RULE_NO_EMPTY_INLINE_CODE)
            .expect("empty inline code is diagnosed");
        assert_eq!((diagnostic.line, diagnostic.column), (2, 6));
        assert_eq!(result.fixed, "first\r\ntext \r\n");
    }

    #[test]
    fn leaves_fenced_content_untouched() {
        let input = "~~~md\n>   １２\n\n\n~~~\n";
        let result = lint_markdown(input);
        assert_eq!(result.fixed, input);
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn removes_whitespace_only_documents() {
        let result = lint_markdown("  \t");
        assert_eq!(result.fixed, "");
        assert_eq!(result.diagnostics.len(), 1);
        assert_eq!(result.diagnostics[0].line, 1);
        assert_eq!(result.diagnostics[0].column, 1);
    }

    #[test]
    fn escapes_json_control_characters() {
        assert_eq!(json_escape("a\n\"b\\"), "a\\n\\\"b\\\\");
    }
}
