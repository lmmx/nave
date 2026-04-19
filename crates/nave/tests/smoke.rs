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
