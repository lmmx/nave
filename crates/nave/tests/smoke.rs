#[test]
fn cli_runs() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_nave"))
        .arg("--help")
        .output()
        .expect("failed to execute nave");

    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Usage") || stdout.contains("USAGE"),
        "unexpected output: {stdout}"
    );
}

#[test]
fn subcommands_listed() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_nave"))
        .arg("--help")
        .output()
        .expect("failed to execute nave");

    let stdout = String::from_utf8_lossy(&output.stdout);
    for sub in ["init", "scan", "pull", "check", "build", "search"] {
        assert!(
            stdout.contains(sub),
            "missing subcommand `{sub}` in help:\n{stdout}"
        );
    }
}

#[test]
fn pull_without_cache_errors_cleanly() {
    let tmp = std::env::temp_dir().join(format!("nave-smoke-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_nave"))
        .arg("pull")
        .env("HOME", &tmp) // force cache lookup into empty dir
        .output()
        .expect("failed to execute nave");

    let _ = std::fs::remove_dir_all(&tmp);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("does not exist") || stderr.contains("run `nave scan`"),
        "unexpected stderr: {stderr}"
    );
}
