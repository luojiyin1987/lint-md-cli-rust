use lint_md_cli_rust::{
    json_escape, lint_markdown, Diagnostic, LintResult, RULE_NO_EMPTY_INLINE_CODE,
};
use std::env;
use std::fs;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputFormat {
    Text,
    Json,
}

#[derive(Debug, PartialEq, Eq)]
struct Options {
    source: Source,
    fix: bool,
    format: OutputFormat,
}

#[derive(Debug, PartialEq, Eq)]
enum Source {
    Stdin,
    File(PathBuf),
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code),
        Err(message) => {
            eprintln!("lint-md-rs: {message}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<u8, String> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args
        .iter()
        .any(|arg| matches!(arg.as_str(), "-h" | "--help"))
    {
        print_help();
        return Ok(0);
    }
    if args
        .iter()
        .any(|arg| matches!(arg.as_str(), "-V" | "--version"))
    {
        println!("lint-md-rs {VERSION}");
        return Ok(0);
    }

    let options = parse_args(args)?;
    let (path, input) = read_source(&options.source)?;
    let result = lint_markdown(&input);

    if options.fix {
        write_fixed(&options.source, &result.fixed)?;
    }

    match options.format {
        OutputFormat::Text => emit_text(&path, &input, &result, options.fix, &options.source)?,
        OutputFormat::Json => emit_json(&path, &input, &result, options.fix),
    }

    let remaining = if options.fix {
        lint_markdown(&result.fixed).diagnostics.len()
    } else {
        result.diagnostics.len()
    };

    Ok(u8::from(remaining > 0))
}

fn parse_args(args: Vec<String>) -> Result<Options, String> {
    let mut fix = false;
    let mut stdin = false;
    let mut format = OutputFormat::Text;
    let mut file: Option<PathBuf> = None;
    let mut index = 0usize;

    while index < args.len() {
        match args[index].as_str() {
            "-f" | "--fix" => fix = true,
            "-i" | "--stdin" => stdin = true,
            "--format" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--format requires 'text' or 'json'".to_owned())?;
                format = parse_format(value)?;
            }
            value if value.starts_with("--format=") => {
                let value = value.trim_start_matches("--format=");
                format = parse_format(value)?;
            }
            value if value.starts_with('-') => {
                return Err(format!("unknown option '{value}'"));
            }
            value => {
                if file.replace(PathBuf::from(value)).is_some() {
                    return Err("the prototype accepts exactly one input file".to_owned());
                }
            }
        }
        index += 1;
    }

    let source = match (stdin, file) {
        (true, None) => Source::Stdin,
        (false, Some(path)) => Source::File(path),
        (true, Some(_)) => return Err("--stdin cannot be combined with a file path".to_owned()),
        (false, None) => return Err("provide a file path or use --stdin".to_owned()),
    };

    Ok(Options {
        source,
        fix,
        format,
    })
}

fn parse_format(value: &str) -> Result<OutputFormat, String> {
    match value {
        "text" => Ok(OutputFormat::Text),
        "json" => Ok(OutputFormat::Json),
        _ => Err(format!(
            "unsupported format '{value}'; expected text or json"
        )),
    }
}

fn read_source(source: &Source) -> Result<(String, String), String> {
    match source {
        Source::Stdin => {
            let mut input = String::new();
            io::stdin()
                .read_to_string(&mut input)
                .map_err(|error| format!("failed to read stdin: {error}"))?;
            Ok(("(stdin)".to_owned(), input))
        }
        Source::File(path) => {
            let input = fs::read_to_string(path)
                .map_err(|error| format!("failed to read '{}': {error}", path.display()))?;
            Ok((path.display().to_string(), input))
        }
    }
}

fn write_fixed(source: &Source, fixed: &str) -> Result<(), String> {
    match source {
        Source::Stdin => Ok(()),
        Source::File(path) => fs::write(path, fixed)
            .map_err(|error| format!("failed to write '{}': {error}", path.display())),
    }
}

fn emit_text(
    path: &str,
    input: &str,
    result: &LintResult,
    fix: bool,
    source: &Source,
) -> Result<(), String> {
    if fix && matches!(source, Source::Stdin) {
        io::stdout()
            .write_all(result.fixed.as_bytes())
            .map_err(|error| format!("failed to write stdout: {error}"))?;
        for diagnostic in result
            .diagnostics
            .iter()
            .filter(|diagnostic| !diagnostic.fixable)
        {
            eprintln!("{}", format_diagnostic(path, input, diagnostic));
        }
        return Ok(());
    }

    for diagnostic in &result.diagnostics {
        println!("{}", format_diagnostic(path, input, diagnostic));
    }
    if fix && result.changed {
        println!("fixed {path}");
    }
    if result.diagnostics.is_empty() {
        println!("{path}: no problems found");
    }
    Ok(())
}

fn format_diagnostic(path: &str, input: &str, diagnostic: &Diagnostic) -> String {
    format!(
        "{}:{}:{}: {} [{}] {}{}",
        path,
        diagnostic.line,
        typescript_column(input, diagnostic),
        diagnostic.severity,
        diagnostic.rule_id,
        diagnostic.message,
        if diagnostic.fixable { " (fixable)" } else { "" }
    )
}

fn emit_json(path: &str, input: &str, result: &LintResult, include_fixed: bool) {
    print!(
        "{{\"path\":\"{}\",\"changed\":{},\"diagnostics\":[",
        json_escape(path),
        result.changed
    );
    for (index, diagnostic) in result.diagnostics.iter().enumerate() {
        if index > 0 {
            print!(",");
        }
        print!(
            "{{\"ruleId\":\"{}\",\"message\":\"{}\",\"line\":{},\"column\":{},\"severity\":\"{}\",\"fixable\":{}}}",
            json_escape(diagnostic.rule_id),
            json_escape(diagnostic.message),
            diagnostic.line,
            typescript_column(input, diagnostic),
            diagnostic.severity,
            diagnostic.fixable
        );
    }
    print!("]");
    if include_fixed {
        print!(",\"fixedContent\":\"{}\"", json_escape(&result.fixed));
    }
    println!("}}");
}

fn typescript_column(input: &str, diagnostic: &Diagnostic) -> usize {
    let Some(line) = source_line(input, diagnostic.line) else {
        return diagnostic.column;
    };

    let byte_index = if diagnostic.rule_id == RULE_NO_EMPTY_INLINE_CODE {
        diagnostic.column.saturating_sub(1).min(line.len())
    } else {
        line.char_indices()
            .nth(diagnostic.column.saturating_sub(1))
            .map_or(line.len(), |(index, _)| index)
    };

    if !line.is_char_boundary(byte_index) {
        return diagnostic.column;
    }

    line[..byte_index].encode_utf16().count() + 1
}

fn source_line(input: &str, target_line: usize) -> Option<&str> {
    if target_line == 0 {
        return None;
    }

    let bytes = input.as_bytes();
    let mut start = 0usize;
    let mut line_number = 1usize;

    loop {
        let mut end = start;
        while end < bytes.len() && !matches!(bytes[end], b'\r' | b'\n') {
            end += 1;
        }

        if line_number == target_line {
            return Some(&input[start..end]);
        }
        if end == bytes.len() {
            return None;
        }

        start = if bytes[end] == b'\r' && end + 1 < bytes.len() && bytes[end + 1] == b'\n' {
            end + 2
        } else {
            end + 1
        };
        line_number += 1;
    }
}

fn print_help() {
    println!(
        "lint-md-rs {VERSION}\n\nUSAGE:\n    lint-md-rs [OPTIONS] <FILE>\n    lint-md-rs --stdin [OPTIONS]\n\nOPTIONS:\n    -f, --fix             Apply safe prototype fixes\n    -i, --stdin           Read Markdown from stdin\n        --format <FORMAT> Output text or json [default: text]\n    -h, --help            Print help\n    -V, --version         Print version\n\nEXIT CODES:\n    0  Clean, or all findings were fixed\n    1  Findings remain\n    2  Usage or I/O failure"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_stdin_fix_json() {
        let options = parse_args(vec![
            "--stdin".to_owned(),
            "--fix".to_owned(),
            "--format=json".to_owned(),
        ])
        .expect("valid options");
        assert_eq!(
            options,
            Options {
                source: Source::Stdin,
                fix: true,
                format: OutputFormat::Json,
            }
        );
    }

    #[test]
    fn rejects_file_and_stdin_together() {
        let error = parse_args(vec!["--stdin".to_owned(), "README.md".to_owned()])
            .expect_err("input sources conflict");
        assert!(error.contains("cannot be combined"));
    }

    #[test]
    fn converts_scanner_and_mdast_columns_to_utf16() {
        let cases = [
            ("😀１２\n", "no-full-width-number", 3),
            ("中文 ` ` after\n", RULE_NO_EMPTY_INLINE_CODE, 4),
            ("😀 ` ` after\n", RULE_NO_EMPTY_INLINE_CODE, 4),
            ("e\u{301} ` ` after\n", RULE_NO_EMPTY_INLINE_CODE, 4),
        ];

        for (input, rule_id, expected_column) in cases {
            let result = lint_markdown(input);
            let diagnostic = result
                .diagnostics
                .iter()
                .find(|item| item.rule_id == rule_id)
                .expect("fixture produces the target diagnostic");
            assert_eq!(typescript_column(input, diagnostic), expected_column);
        }
    }

    #[test]
    fn finds_source_lines_across_line_ending_styles() {
        let input = "first\r\nsecond\rthird\nfourth";
        assert_eq!(source_line(input, 1), Some("first"));
        assert_eq!(source_line(input, 2), Some("second"));
        assert_eq!(source_line(input, 3), Some("third"));
        assert_eq!(source_line(input, 4), Some("fourth"));
        assert_eq!(source_line(input, 5), None);
    }
}
