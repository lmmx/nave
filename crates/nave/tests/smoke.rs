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
    for sub in ["init", "discover", "fetch", "validate"] {
        assert!(
            stdout.contains(sub),
            "missing subcommand `{sub}` in help:\n{stdout}"
        );
    }
}
