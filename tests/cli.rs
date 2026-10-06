use std::fs;
use std::io::Write;
use std::process::{Command, Output, Stdio};

use tempfile::TempDir;

fn tokwc(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tokwc"))
        .args(args)
        .output()
        .expect("failed to run tokwc")
}

fn tokwc_with_stdin(args: &[&str], input: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_tokwc"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to run tokwc");

    child
        .stdin
        .as_mut()
        .expect("stdin should be piped")
        .write_all(input)
        .expect("failed to write stdin");

    child.wait_with_output().expect("failed to wait for tokwc")
}

fn text_output(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes).expect("output should be valid UTF-8")
}

#[test]
fn directory_walk_skips_non_utf8_files_and_counts_text_files() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("ok.txt"), "hello world\n").unwrap();
    fs::write(dir.path().join("bin.dat"), [0xff, 0xfe, 0xfd]).unwrap();

    let output = tokwc(&[dir.path().to_str().unwrap()]);

    assert!(output.status.success());

    let stdout = text_output(output.stdout);
    assert!(stdout.contains("input"));
    assert!(stdout.contains("tokens"));
    assert!(stdout.contains("ok.txt"));
    assert!(!stdout.contains("bin.dat"));

    let stderr = text_output(output.stderr);
    assert!(stderr.contains("tokwc: warning: skipped"));
    assert!(stderr.contains("bin.dat: not valid UTF-8"));
}

#[test]
fn explicitly_named_non_utf8_file_is_still_an_error() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("bin.dat");
    fs::write(&path, [0xff, 0xfe, 0xfd]).unwrap();

    let output = tokwc(&[path.to_str().unwrap()]);

    assert!(!output.status.success());
    assert!(text_output(output.stderr).contains("bin.dat: not valid UTF-8"));
}

#[test]
fn directory_walk_with_only_non_utf8_files_fails_after_warning() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("bin.dat"), [0xff, 0xfe, 0xfd]).unwrap();

    let output = tokwc(&[dir.path().to_str().unwrap()]);

    assert!(!output.status.success());
    let stderr = text_output(output.stderr);
    assert!(stderr.contains("tokwc: warning: skipped"));
    assert!(stderr.contains("no files counted (1 skipped)"));
}

#[test]
fn directory_walk_respects_ignore_files_by_default() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join(".ignore"), "ignored.txt\n").unwrap();
    fs::write(dir.path().join("kept.txt"), "count me\n").unwrap();
    fs::write(dir.path().join("ignored.txt"), "do not count me\n").unwrap();

    let dir_arg = dir.path().to_str().unwrap();
    let output = tokwc(&[dir_arg]);

    assert!(output.status.success());
    let stdout = text_output(output.stdout);
    assert!(stdout.contains("kept.txt"));
    assert!(!stdout.contains("ignored.txt"));

    let output = tokwc(&["--no-ignore", dir_arg]);

    assert!(output.status.success());
    let stdout = text_output(output.stdout);
    assert!(stdout.contains("kept.txt"));
    assert!(stdout.contains("ignored.txt"));
}

#[test]
fn encoding_and_model_are_mutually_exclusive() {
    let output = tokwc(&["--encoding", "o200k_base", "--model", "gpt-4o"]);

    assert!(!output.status.success());
    assert!(text_output(output.stderr).contains("cannot be used with"));
}

#[test]
fn stdin_is_labeled_and_counted() {
    let output = tokwc_with_stdin(&[], b"hello world\n");

    assert!(output.status.success());
    let stdout = text_output(output.stdout);
    let row = stdout.lines().last().unwrap();
    assert_eq!(row.split_whitespace().collect::<Vec<_>>(), ["stdin", "3"]);
}

#[test]
fn json_output_includes_model_encoding_and_counts() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("prompt.txt");
    fs::write(&path, "hello world\n").unwrap();

    for model in [
        "gpt-6.1-sol",
        "gpt-6-astra",
        "gpt-6-sol",
        "gpt-6-luna",
        "gpt-4o",
    ] {
        let output = tokwc(&["--json", "--all", "--model", model, path.to_str().unwrap()]);

        assert!(output.status.success(), "{}", text_output(output.stderr));
        let json: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("stdout should be JSON");

        assert_eq!(json["encoding"], "o200k_base");
        assert_eq!(json["model"], model);
        assert_eq!(json["metrics"][0], "lines");
        assert_eq!(json["inputs"][0]["counts"]["lines"], 1);
        assert_eq!(json["inputs"][0]["counts"]["words"], 2);
        assert_eq!(json["inputs"][0]["counts"]["tokens"], 3);
    }
}

#[test]
fn openai_rejects_unknown_gpt6_model_names() {
    for model in [
        "gpt-6.1-sol-typo",
        "gpt-6-astra-unknown",
        "gpt-6.2-sol",
        "gpt-60",
    ] {
        let output = tokwc(&["--model", model]);
        assert!(!output.status.success(), "{model}");
        assert!(text_output(output.stderr).contains("unknown model"));
    }
}

#[test]
fn claude_encoding_counts_each_input_with_the_v5_family() {
    let dir = TempDir::new().unwrap();
    let prompt = dir.path().join("prompt.txt");
    fs::write(&prompt, "hello world\n").unwrap();
    let second = dir.path().join("second.txt");
    fs::write(&second, "second file\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_tokwc"))
        .args(["--json", "--encoding", "claude_v5"])
        .arg(&prompt)
        .arg(&second)
        .output()
        .unwrap();

    assert!(output.status.success(), "{}", text_output(output.stderr));
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["provider"], "anthropic");
    assert!(json.get("model").is_none());
    assert_eq!(json["encoding"], "claude_v5");
    assert_eq!(json["inputs"].as_array().unwrap().len(), 3);
    assert_eq!(json["inputs"][0]["counts"]["tokens"], 9);
    assert_eq!(json["inputs"][1]["counts"]["tokens"], 8);
    assert_eq!(json["inputs"][2]["name"], "total");
    assert_eq!(json["inputs"][2]["counts"]["tokens"], 17);
}

#[test]
fn claude_model_selects_the_v4_7_family() {
    let output = tokwc_with_stdin(&["--json", "--model", "claude-opus-4-7"], b"hello world\n");
    assert!(output.status.success(), "{}", text_output(output.stderr));
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["encoding"], "claude_v4_7");
    assert_eq!(json["inputs"][0]["counts"]["tokens"], 14);
}

#[test]
fn claude_models_and_direct_encodings_select_measured_frames() {
    for (model, encoding, tokens) in [
        ("claude-opus-4-8", "claude_v4_8", 9),
        ("claude-sonnet-5", "claude_v4_8", 9),
        ("claude-fable-5", "claude_v4_8", 9),
        ("claude-opus-5", "claude_v5", 8),
        ("claude-fable-5-1", "claude_fable_v5_1", 10),
        ("claude-opus-5-5", "claude_v5_5", 10),
        ("claude-sonnet-5-5", "claude_v5_5", 10),
    ] {
        for arguments in [
            ["--json", "--model", model],
            ["--json", "--encoding", encoding],
        ] {
            let output = tokwc_with_stdin(&arguments, b"hello ");
            assert!(output.status.success(), "{}", text_output(output.stderr));
            let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(json["encoding"], encoding);
            assert_eq!(json["inputs"][0]["counts"]["tokens"], tokens);
        }
    }
}

#[test]
fn claude_rejects_openai_special_tokens() {
    let output = tokwc(&["--encoding", "claude_v5", "--special"]);
    assert!(!output.status.success());
    assert!(text_output(output.stderr).contains("--special is only supported"));
}

#[test]
fn claude_rejects_pre_v4_7_models() {
    let output = tokwc(&["--model", "claude-opus-4-6"]);
    assert!(!output.status.success());
    assert!(text_output(output.stderr).contains("unsupported Claude model"));
}

#[test]
fn claude_rejects_unknown_families_versions_and_suffixes() {
    for model in [
        "claude-typo-5",
        "claude-haiku-5",
        "claude-opus-99",
        "claude-opus-6",
        "claude-sonnet-6",
        "claude-opus-5-9",
        "claude-opus-4-10",
        "claude-opus-5-latest",
        "claude-opus-5-2026010x",
        "claude-opus-5-20260101-extra",
    ] {
        let output = tokwc(&["--model", model]);
        assert!(!output.status.success(), "{model}");
        assert!(text_output(output.stderr).contains("unsupported Claude model"));
    }
}

#[test]
fn json_distinguishes_a_file_named_total_from_the_total_row() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("total"), "hello").unwrap();
    fs::write(dir.path().join("other"), "world").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_tokwc"))
        .current_dir(dir.path())
        .args(["--json", "-c", "total", "other"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["inputs"][0]["name"], "total");
    assert_eq!(json["inputs"][0]["is_total"], false);
    assert_eq!(json["inputs"][2]["name"], "total");
    assert_eq!(json["inputs"][2]["is_total"], true);
    assert_eq!(json["inputs"][2]["counts"]["bytes"], 10);
}

#[test]
fn bytes_and_lines_accept_invalid_utf8() {
    for model in ["gpt-6.1-sol", "claude-opus-5-5"] {
        let output = tokwc_with_stdin(&["--json", "-cl", "-M", model], b"x\xff\n");
        assert!(output.status.success(), "{}", text_output(output.stderr));
        let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        let counts = &json["inputs"][0]["counts"];
        assert_eq!(counts["bytes"], 3);
        assert_eq!(counts["lines"], 1);
        assert!(counts.get("tokens").is_none());
    }
}

#[test]
fn claude_all_counts_and_lossy_decoding_share_text_metrics() {
    let output = tokwc_with_stdin(
        &["--json", "--all", "--lossy", "--model", "claude-opus-5-5"],
        b"hello\xff world\n",
    );
    assert!(output.status.success(), "{}", text_output(output.stderr));
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let counts = &json["inputs"][0]["counts"];
    assert_eq!(counts["bytes"], 13);
    assert_eq!(counts["chars"], 13);
    assert_eq!(counts["words"], 2);
    assert_eq!(counts["lines"], 1);
    assert_eq!(
        counts["tokens"],
        tokwc::claude::ClaudeTokenizer::new(tokwc::claude::ClaudeVersion::V5_5)
            .token_count("hello\u{fffd} world\n")
    );
}

#[cfg(unix)]
#[test]
fn explicit_stdin_device_is_readable() {
    let output = tokwc_with_stdin(&["--json", "-c", "/dev/stdin"], b"hello");
    assert!(output.status.success(), "{}", text_output(output.stderr));
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["inputs"][0]["counts"]["bytes"], 5);
}

#[cfg(unix)]
#[test]
fn closed_stdout_exits_without_a_panic() {
    use std::os::fd::OwnedFd;
    use std::os::unix::net::UnixStream;

    for args in [vec!["-c"], vec!["--json", "-c"], vec!["--list-encodings"]] {
        let (reader, writer) = UnixStream::pair().unwrap();
        drop(reader);
        let output = Command::new(env!("CARGO_BIN_EXE_tokwc"))
            .args(args)
            .stdin(Stdio::null())
            .stdout(OwnedFd::from(writer))
            .stderr(Stdio::piped())
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", text_output(output.stderr));
        assert_eq!(text_output(output.stderr), "");
    }
}
