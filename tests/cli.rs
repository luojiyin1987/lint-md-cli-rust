use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

const BIN: &str = env!("CARGO_BIN_EXE_lint-md-rs");

fn temporary_file(name: &str, content: &str) -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock is after Unix epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("lint-md-rs-{unique}-{name}"));
    fs::write(&path, content).expect("write fixture");
    path
}

#[test]
fn reports_findings_and_exits_one() {
    let path = temporary_file("lint.md", "版本１２\n");
    let output = Command::new(BIN).arg(&path).output().expect("run binary");

    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8(output.stdout).expect("UTF-8 stdout");
    assert!(stdout.contains("no-full-width-number"));
    fs::remove_file(path).expect("remove fixture");
}

#[test]
fn fixes_file_and_exits_zero_when_nothing_remains() {
    let path = temporary_file("fix.md", "版本１２\n\n\nend\n");
    let output = Command::new(BIN)
        .args(["--fix"])
        .arg(&path)
        .output()
        .expect("run binary");

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        fs::read_to_string(&path).expect("read fixed file"),
        "版本12\n\nend\n"
    );
    fs::remove_file(path).expect("remove fixture");
}

#[test]
fn fixes_stdin_without_polluting_stdout() {
    let mut child = Command::new(BIN)
        .args(["--stdin", "--fix"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn binary");
    child
        .stdin
        .as_mut()
        .expect("stdin pipe")
        .write_all("版本１２\n".as_bytes())
        .expect("write stdin");
    let output = child.wait_with_output().expect("wait for binary");

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "版本12\n");
}

#[test]
fn emits_machine_readable_json() {
    let path = temporary_file("json.md", ">\n");
    let output = Command::new(BIN)
        .args(["--format", "json"])
        .arg(&path)
        .output()
        .expect("run binary");

    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8(output.stdout).expect("UTF-8 stdout");
    assert!(stdout.starts_with("{\"path\":"));
    assert!(stdout.contains("\"ruleId\":\"no-empty-blockquote\""));
    fs::remove_file(path).expect("remove fixture");
}
