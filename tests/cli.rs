use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn repl_keeps_state_across_lines() {
    let exe = env!("CARGO_BIN_EXE_bory");
    let mut child = Command::new(exe)
        .arg("repl")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    {
        let stdin = child.stdin.as_mut().unwrap();
        stdin.write_all(b"var x = 2\nx + 3\n:quit\n").unwrap();
    }

    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("BORY REPL 0.4.0"));
    assert!(stdout.contains("5"));
}

#[test]
fn package_manager_creates_and_lists_packages() {
    let exe = env!("CARGO_BIN_EXE_bory");
    let root = unique_temp_dir();
    fs::create_dir_all(&root).unwrap();

    let init = Command::new(exe)
        .args(["pkg", "init", "demo_pkg"])
        .current_dir(&root)
        .output()
        .unwrap();
    assert!(init.status.success());
    assert!(root.join("packages").join("demo_pkg").join("main.boy").exists());

    let list = Command::new(exe)
        .args(["pkg", "list"])
        .current_dir(&root)
        .output()
        .unwrap();
    assert!(list.status.success());
    let stdout = String::from_utf8_lossy(&list.stdout);
    assert!(stdout.contains("demo_pkg"));
}

#[test]
fn package_manager_installs_local_files() {
    let exe = env!("CARGO_BIN_EXE_bory");
    let root = unique_temp_dir();
    fs::create_dir_all(&root).unwrap();

    let source = root.join("mathkit.boy");
    fs::write(
        &source,
        "task triple(x) =>\n    give x * 3\nend\n",
    )
    .unwrap();

    let install = Command::new(exe)
        .args([
            "pkg",
            "install",
            source.to_string_lossy().as_ref(),
            "mathkit",
        ])
        .current_dir(&root)
        .output()
        .unwrap();
    assert!(install.status.success());

    let main_path = root.join("packages").join("mathkit").join("main.boy");
    assert!(main_path.exists());
}

#[test]
fn formatter_rewrites_file() {
    let exe = env!("CARGO_BIN_EXE_bory");
    let root = unique_temp_dir();
    fs::create_dir_all(&root).unwrap();

    let source = root.join("main.boy");
    fs::write(&source, "task demo(x)=>\n give x+1\nend\n").unwrap();

    let format = Command::new(exe)
        .args(["fmt", source.to_string_lossy().as_ref()])
        .current_dir(&root)
        .output()
        .unwrap();
    assert!(format.status.success());

    let formatted = fs::read_to_string(&source).unwrap();
    assert!(formatted.contains("task demo(x) =>"));
    assert!(formatted.contains("    give (x + 1)"));
}

fn unique_temp_dir() -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("bory-cli-test-{stamp}"))
}
