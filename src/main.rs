use lint_md_cli_rust::{json_escape, lint_markdown, Diagnostic, LintResult};
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
        OutputFormat::Text => emit_text(&path, &result, options.fix, &options.source)?,
        OutputFormat::Json => emit_json(&path, &result, options.fix),
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
            eprintln!("{}", format_diagnostic(path, diagnostic));
        }
        return Ok(());
    }

    for diagnostic in &result.diagnostics {
        println!("{}", format_diagnostic(path, diagnostic));
    }
    if fix && result.changed {
        println!("fixed {path}");
    }
    if result.diagnostics.is_empty() {
        println!("{path}: no problems found");
    }
    Ok(())
}

fn format_diagnostic(path: &str, diagnostic: &Diagnostic) -> String {
    format!(
        "{}:{}:{}: {} [{}] {}{}",
        path,
        diagnostic.line,
        diagnostic.column,
        diagnostic.severity,
        diagnostic.rule_id,
        diagnostic.message,
        if diagnostic.fixable {
            " (fixable)"
        } else {
            ""
        }
    )
}

fn emit_json(path: &str, result: &LintResult, include_fixed: bool) {
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
            diagnostic.column,
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
}
