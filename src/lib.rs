//! Rust validation core for a possible native `lint-md` CLI.
//!
//! The prototype combines lightweight scanners for text-oriented rules with
//! mdast positions for rules that depend on Markdown parsing semantics.

use markdown::{
    mdast::{InlineCode, Node},
    to_mdast, ParseOptions,
};
use std::{collections::HashMap, fmt, ops::Range};

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
    start: usize,
    number: usize,
    protected: bool,
}

#[derive(Debug, Clone)]
struct EmptyInlineCodeFinding {
    line: usize,
    column: usize,
    range: Range<usize>,
}

#[derive(Debug, Clone)]
struct BlockquoteFinding {
    line: usize,
    column: usize,
    range: Range<usize>,
    marker_offset: usize,
    spacing_delta: Option<usize>,
}

#[derive(Debug, Default)]
struct MdastAnalysis {
    inline_code_ranges: Vec<Range<usize>>,
    empty_inline_code: Vec<EmptyInlineCodeFinding>,
    blockquotes: Vec<BlockquoteFinding>,
}

#[derive(Debug, Clone)]
struct TextEdit {
    range: Range<usize>,
    replacement: &'static str,
}

/// Lint Markdown using the prototype rule set.
///
/// Diagnostics describe the original input. Rules that depend on Markdown
/// structure share one mdast parse, while ordinary clean documents stay on the
/// scanner fast path. Fixes repeat until interacting rules become stable.
pub fn lint_markdown(input: &str) -> LintResult {
    let analysis = analyze_mdast(input);
    let mut diagnostics = Vec::new();

    diagnose_blank_lines(input, &mut diagnostics);
    diagnose_line_rules(input, &analysis, &mut diagnostics);
    diagnose_empty_inline_code(&analysis, &mut diagnostics);

    let mut fixed = fix_once(input, &analysis);
    if fixed != input {
        for _ in 0..3 {
            let next_analysis = analyze_mdast(&fixed);
            let next = fix_once(&fixed, &next_analysis);
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

fn analyze_mdast(input: &str) -> MdastAnalysis {
    let inspect_empty_inline = may_contain_whitespace_only_inline_code(input);
    let inspect_inline_ranges = input.contains('`') && input.chars().any(is_full_width_digit);
    let inspect_blockquotes = may_contain_problematic_blockquote(input);

    if !inspect_empty_inline && !inspect_inline_ranges && !inspect_blockquotes {
        return MdastAnalysis::default();
    }

    let tree = to_mdast(input, &ParseOptions::default())
        .expect("CommonMark parsing without MDX extensions should not fail");
    let mut analysis = MdastAnalysis::default();
    collect_mdast_findings(
        input,
        &tree,
        inspect_empty_inline,
        inspect_inline_ranges,
        inspect_blockquotes,
        &mut analysis,
    );
    analysis
        .inline_code_ranges
        .sort_unstable_by_key(|range| range.start);
    analysis
        .empty_inline_code
        .sort_unstable_by_key(|finding| finding.range.start);
    analysis
        .blockquotes
        .sort_unstable_by_key(|finding| finding.range.start);
    analysis
}

fn collect_mdast_findings(
    input: &str,
    node: &Node,
    inspect_empty_inline: bool,
    inspect_inline_ranges: bool,
    inspect_blockquotes: bool,
    analysis: &mut MdastAnalysis,
) {
    match node {
        Node::InlineCode(inline_code) => {
            collect_inline_code_finding(
                inline_code,
                inspect_empty_inline,
                inspect_inline_ranges,
                analysis,
            );
        }
        Node::Blockquote(blockquote) if inspect_blockquotes => {
            if let Some(position) = &blockquote.position {
                let marker_offset =
                    blockquote_marker_offset(input, position.start.offset, position.end.offset);
                let spacing_delta = blockquote
                    .children
                    .first()
                    .map(|_| blockquote_spacing_delta(input, marker_offset));
                analysis.blockquotes.push(BlockquoteFinding {
                    line: position.start.line,
                    column: source_column(input, marker_offset),
                    range: position.start.offset..position.end.offset,
                    marker_offset,
                    spacing_delta,
                });
            }
        }
        _ => {}
    }

    if let Some(children) = node.children() {
        for child in children {
            collect_mdast_findings(
                input,
                child,
                inspect_empty_inline,
                inspect_inline_ranges,
                inspect_blockquotes,
                analysis,
            );
        }
    }
}

fn blockquote_marker_offset(input: &str, start: usize, end: usize) -> usize {
    input[start..end]
        .char_indices()
        .take_while(|(_, ch)| !matches!(ch, '\r' | '\n'))
        .find_map(|(offset, ch)| (ch == '>').then_some(start + offset))
        .unwrap_or(start)
}

fn source_column(input: &str, offset: usize) -> usize {
    let line_start = input[..offset]
        .char_indices()
        .rev()
        .find_map(|(index, ch)| matches!(ch, '\r' | '\n').then_some(index + ch.len_utf8()))
        .unwrap_or(0);
    input[line_start..offset].chars().count() + 1
}

fn blockquote_spacing_delta(input: &str, marker_offset: usize) -> usize {
    let bytes = input.as_bytes();
    let mut cursor = marker_offset.saturating_add(1).min(bytes.len());

    while cursor < bytes.len() && bytes[cursor] == b' ' {
        cursor += 1;
    }

    cursor.saturating_sub(marker_offset)
}

fn collect_inline_code_finding(
    inline_code: &InlineCode,
    inspect_empty_inline: bool,
    inspect_inline_ranges: bool,
    analysis: &mut MdastAnalysis,
) {
    let Some(position) = &inline_code.position else {
        return;
    };
    let range = position.start.offset..position.end.offset;

    if inspect_inline_ranges {
        analysis.inline_code_ranges.push(range.clone());
    }
    if inspect_empty_inline && inline_code.value.trim().is_empty() {
        analysis.empty_inline_code.push(EmptyInlineCodeFinding {
            line: position.start.line,
            column: position.start.column,
            range,
        });
    }
}

fn may_contain_problematic_blockquote(input: &str) -> bool {
    let bytes = input.as_bytes();
    let mut index = 0usize;

    while index < bytes.len() {
        if bytes[index] != b'>' {
            index += 1;
            continue;
        }

        let after = index + 1;
        if after >= bytes.len() || matches!(bytes[after], b'\r' | b'\n' | b'\t') {
            return true;
        }
        if bytes[after] != b' ' {
            return true;
        }

        let content = after + 1;
        if content >= bytes.len() || matches!(bytes[content], b' ' | b'\t' | b'\r' | b'\n') {
            return true;
        }

        index += 1;
    }

    false
}

fn fix_once(input: &str, analysis: &MdastAnalysis) -> String {
    let edits = mdast_edits(input, analysis);
    let after_mdast = apply_text_edits(input, &edits);

    let owned_ranges;
    let inline_code_ranges = if after_mdast == input {
        &analysis.inline_code_ranges
    } else {
        owned_ranges = full_width_inline_code_ranges(&after_mdast);
        &owned_ranges
    };

    let mut fixed = fix_full_width_numbers_document(&after_mdast, inline_code_ranges);
    fixed = fix_blank_lines(&fixed);
    fixed
}

fn mdast_edits(input: &str, analysis: &MdastAnalysis) -> Vec<TextEdit> {
    let mut edits = Vec::new();

    for finding in &analysis.empty_inline_code {
        edits.push(TextEdit {
            range: finding.range.clone(),
            replacement: "",
        });
    }

    for finding in &analysis.blockquotes {
        match finding.spacing_delta {
            None => edits.push(TextEdit {
                range: finding.range.clone(),
                replacement: "",
            }),
            Some(delta) => {
                if delta == 2 {
                    continue;
                }

                let start = finding.marker_offset.saturating_add(1).min(input.len());
                let end = finding.marker_offset.saturating_add(delta).min(input.len());
                edits.push(TextEdit {
                    range: start..end.max(start),
                    replacement: " ",
                });
            }
        }
    }

    edits
}

fn apply_text_edits(input: &str, edits: &[TextEdit]) -> String {
    if edits.is_empty() {
        return input.to_owned();
    }

    let mut edits = edits.to_vec();
    edits.sort_unstable_by(|left, right| {
        left.range
            .start
            .cmp(&right.range.start)
            .then_with(|| right.range.end.cmp(&left.range.end))
    });

    let mut output = String::with_capacity(input.len());
    let mut cursor = 0usize;

    for edit in edits {
        if edit.range.start < cursor
            || edit.range.start > edit.range.end
            || edit.range.end > input.len()
            || !input.is_char_boundary(edit.range.start)
            || !input.is_char_boundary(edit.range.end)
        {
            continue;
        }

        output.push_str(&input[cursor..edit.range.start]);
        output.push_str(edit.replacement);
        cursor = edit.range.end;
    }

    output.push_str(&input[cursor..]);
    output
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
            start,
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

fn diagnose_line_rules(input: &str, analysis: &MdastAnalysis, diagnostics: &mut Vec<Diagnostic>) {
    let mut active_fence: Option<Fence> = None;
    let mut blockquotes_by_line = blockquote_diagnostics_by_line(analysis);

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

        if let Some(mut blockquote_diagnostics) = blockquotes_by_line.remove(&line.number) {
            diagnostics.append(&mut blockquote_diagnostics);
        }

        if line.content.trim_matches([' ', '\t']).is_empty() {
            continue;
        }

        diagnose_full_width_numbers(
            line.content,
            line.start,
            line.number,
            &analysis.inline_code_ranges,
            diagnostics,
        );
    }
}

fn blockquote_diagnostics_by_line(analysis: &MdastAnalysis) -> HashMap<usize, Vec<Diagnostic>> {
    let mut by_line = HashMap::<usize, Vec<Diagnostic>>::new();

    for finding in &analysis.blockquotes {
        let diagnostic = match finding.spacing_delta {
            None => Some(Diagnostic {
                rule_id: RULE_NO_EMPTY_BLOCKQUOTE,
                message: "Blockquote content must not be empty.",
                line: finding.line,
                column: finding.column,
                severity: Severity::Error,
                fixable: true,
            }),
            Some(delta) if delta != 2 => Some(Diagnostic {
                rule_id: RULE_NO_MULTIPLE_SPACE_BLOCKQUOTE,
                message: "Use exactly one space after the blockquote marker.",
                line: finding.line,
                column: finding.column,
                severity: Severity::Error,
                fixable: true,
            }),
            _ => None,
        };

        if let Some(diagnostic) = diagnostic {
            by_line.entry(finding.line).or_default().push(diagnostic);
        }
    }

    by_line
}

fn diagnose_empty_inline_code(analysis: &MdastAnalysis, diagnostics: &mut Vec<Diagnostic>) {
    for finding in &analysis.empty_inline_code {
        diagnostics.push(Diagnostic {
            rule_id: RULE_NO_EMPTY_INLINE_CODE,
            message: "Inline code content must not be empty.",
            line: finding.line,
            column: finding.column,
            severity: Severity::Error,
            fixable: true,
        });
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

fn char_column(line: &str, byte_index: usize) -> usize {
    line[..byte_index].chars().count() + 1
}

fn may_contain_whitespace_only_inline_code(input: &str) -> bool {
    let bytes = input.as_bytes();
    let mut previous_runs = HashMap::<usize, usize>::new();
    let mut index = 0usize;

    while index < bytes.len() {
        if bytes[index] != b'`' {
            index += 1;
            continue;
        }

        let start = index;
        while index < bytes.len() && bytes[index] == b'`' {
            index += 1;
        }
        let width = index - start;

        if let Some(previous_end) = previous_runs.insert(width, index) {
            if input[previous_end..start].chars().all(char::is_whitespace) {
                return true;
            }
        }
    }

    false
}

fn full_width_inline_code_ranges(input: &str) -> Vec<Range<usize>> {
    if !input.contains('`') || !input.chars().any(is_full_width_digit) {
        return Vec::new();
    }

    let tree = to_mdast(input, &ParseOptions::default())
        .expect("CommonMark parsing without MDX extensions should not fail");
    let mut ranges = Vec::new();
    collect_inline_code_ranges(&tree, &mut ranges);
    ranges.sort_unstable_by_key(|range| range.start);
    ranges
}

fn collect_inline_code_ranges(node: &Node, ranges: &mut Vec<Range<usize>>) {
    if let Node::InlineCode(inline_code) = node {
        if let Some(position) = &inline_code.position {
            ranges.push(position.start.offset..position.end.offset);
        }
    }

    if let Some(children) = node.children() {
        for child in children {
            collect_inline_code_ranges(child, ranges);
        }
    }
}

fn offset_is_in_ranges(offset: usize, ranges: &[Range<usize>]) -> bool {
    let index = ranges.partition_point(|range| range.end <= offset);
    ranges
        .get(index)
        .is_some_and(|range| range.contains(&offset))
}

fn diagnose_full_width_numbers(
    line: &str,
    line_start: usize,
    line_number: usize,
    inline_code_ranges: &[Range<usize>],
    diagnostics: &mut Vec<Diagnostic>,
) {
    for byte_index in full_width_digit_run_starts(line, line_start, inline_code_ranges) {
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

fn full_width_digit_run_starts(
    line: &str,
    line_start: usize,
    inline_code_ranges: &[Range<usize>],
) -> Vec<usize> {
    let mut starts = Vec::new();
    let mut digit_run: Option<usize> = None;

    for (byte_index, ch) in line.char_indices() {
        let protected = offset_is_in_ranges(line_start + byte_index, inline_code_ranges);
        if !protected && is_full_width_digit(ch) {
            if digit_run.is_none() {
                digit_run = Some(byte_index);
            }
        } else if let Some(start) = digit_run.take() {
            starts.push(start);
        }
    }

    if let Some(start) = digit_run {
        starts.push(start);
    }
    starts
}

fn fix_full_width_numbers_document(input: &str, inline_code_ranges: &[Range<usize>]) -> String {
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

        fixed.push_str(&fix_full_width_numbers(
            line.content,
            line.start,
            inline_code_ranges,
        ));
        fixed.push_str(line.ending);
    }

    fixed
}

fn fix_full_width_numbers(
    line: &str,
    line_start: usize,
    inline_code_ranges: &[Range<usize>],
) -> String {
    let mut output = String::with_capacity(line.len());
    let mut cursor = 0usize;

    for (byte_index, ch) in line.char_indices() {
        if offset_is_in_ranges(line_start + byte_index, inline_code_ranges) {
            continue;
        }
        if let Some(ascii) = to_ascii_digit(ch) {
            output.push_str(&line[cursor..byte_index]);
            output.push(ascii);
            cursor = byte_index + ch.len_utf8();
        }
    }

    if cursor == 0 {
        return line.to_owned();
    }
    output.push_str(&line[cursor..]);
    output
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
    fn fixes_full_width_numbers_after_unmatched_backticks() {
        let input = "before `１２ after ３４\n";
        let result = lint_markdown(input);
        assert_eq!(result.fixed, "before `12 after 34\n");
        let columns = result
            .diagnostics
            .iter()
            .filter(|item| item.rule_id == RULE_NO_FULL_WIDTH_NUMBER)
            .map(|item| item.column)
            .collect::<Vec<_>>();
        assert_eq!(columns, vec![9, 18]);
    }

    #[test]
    fn protects_only_valid_inline_code_ranges() {
        let input = "`１２` and ３４\n";
        let result = lint_markdown(input);
        assert_eq!(result.fixed, "`１２` and 34\n");
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
    fn treats_mismatched_backtick_runs_as_text() {
        let input = "before ``１２` after ３４\n";
        let result = lint_markdown(input);
        assert_eq!(result.fixed, "before ``12` after 34\n");
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
    fn protects_full_width_numbers_in_multiline_code_spans() {
        let input = "before `１２\ncontinued ３４` outside ５６\n";
        let result = lint_markdown(input);
        assert_eq!(result.fixed, "before `１２\ncontinued ３４` outside 56\n");
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
    fn fixes_separate_blockquote_nodes() {
        let input = ">   text\n\n>missing\n\n>\n";
        let result = lint_markdown(input);
        assert_eq!(result.fixed, "> text\n\n> missing\n");
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
    fn treats_contiguous_crlf_lines_as_one_blockquote_node() {
        let input = ">missing\r\n>   text\r\n";
        let result = lint_markdown(input);
        assert_eq!(result.fixed, "> missing\r\n>   text\r\n");
        let diagnostics = result
            .diagnostics
            .iter()
            .filter(|item| item.rule_id == RULE_NO_MULTIPLE_SPACE_BLOCKQUOTE)
            .collect::<Vec<_>>();
        assert_eq!(diagnostics.len(), 1);
        assert_eq!((diagnostics[0].line, diagnostics[0].column), (1, 1));
    }

    #[test]
    fn fixes_blockquote_nested_in_list_by_node_position() {
        let input = "- >missing\n";
        let result = lint_markdown(input);
        assert_eq!(result.fixed, "- > missing\n");
        let diagnostic = result
            .diagnostics
            .iter()
            .find(|item| item.rule_id == RULE_NO_MULTIPLE_SPACE_BLOCKQUOTE)
            .expect("list-contained blockquote is diagnosed");
        assert_eq!((diagnostic.line, diagnostic.column), (1, 3));
    }

    #[test]
    fn fixes_indented_blockquote_at_marker_column() {
        let input = "  >   text\n";
        let result = lint_markdown(input);
        assert_eq!(result.fixed, "  > text\n");
        let diagnostic = result
            .diagnostics
            .iter()
            .find(|item| item.rule_id == RULE_NO_MULTIPLE_SPACE_BLOCKQUOTE)
            .expect("indented blockquote is diagnosed");
        assert_eq!((diagnostic.line, diagnostic.column), (1, 3));
    }

    #[test]
    fn fixes_nested_blockquote_marker_once() {
        let input = ">> text\n";
        let result = lint_markdown(input);
        assert_eq!(result.fixed, "> > text\n");
        assert_eq!(
            result
                .diagnostics
                .iter()
                .filter(|item| item.rule_id == RULE_NO_MULTIPLE_SPACE_BLOCKQUOTE)
                .count(),
            1
        );
    }

    #[test]
    fn removes_empty_blockquote_by_mdast_range() {
        let result = lint_markdown(">\n");
        assert_eq!(result.fixed, "");
        let diagnostic = result
            .diagnostics
            .iter()
            .find(|item| item.rule_id == RULE_NO_EMPTY_BLOCKQUOTE)
            .expect("empty blockquote is diagnosed");
        assert_eq!((diagnostic.line, diagnostic.column), (1, 1));
        assert!(diagnostic.fixable);
    }

    #[test]
    fn locates_indented_empty_blockquote_at_marker() {
        let result = lint_markdown("  >   \n");
        assert_eq!(result.fixed, "");
        let diagnostic = result
            .diagnostics
            .iter()
            .find(|item| item.rule_id == RULE_NO_EMPTY_BLOCKQUOTE)
            .expect("indented empty blockquote is diagnosed");
        assert_eq!((diagnostic.line, diagnostic.column), (1, 3));
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
    fn prefilters_non_empty_code_without_parsing_the_document() {
        assert!(!may_contain_whitespace_only_inline_code(
            "Inline `code` remains non-empty.\n```text\nsample\n```\n"
        ));
        assert!(!may_contain_whitespace_only_inline_code(
            "before `` after\n"
        ));
        assert!(may_contain_whitespace_only_inline_code("` \t `\n"));
    }

    #[test]
    fn prefilters_full_width_inline_code_parsing() {
        assert!(full_width_inline_code_ranges("Inline `code` only.\n").is_empty());
        assert!(full_width_inline_code_ranges("版本１２ only.\n").is_empty());
        assert_eq!(full_width_inline_code_ranges("`１２`\n").len(), 1);
    }

    #[test]
    fn prefilters_correct_blockquotes_without_parsing() {
        assert!(!may_contain_problematic_blockquote("> quoted text\n"));
        assert!(may_contain_problematic_blockquote(">missing\n"));
        assert!(may_contain_problematic_blockquote(">   text\n"));
        assert!(may_contain_problematic_blockquote("- >missing\n"));
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
