use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use bory::{Interpreter, Value, check_source, format_source};
use reqwest::blocking::get;

fn print_help() {
    println!("BORY 0.4.0");
    println!();
    println!("Usage:");
    println!("  bory                 Start the interactive REPL");
    println!("  bory <file.boy>      Run a source file");
    println!("  bory run <file.boy>  Run a source file");
    println!("  bory check <file.boy>");
    println!("  bory fmt <file.boy>");
    println!("  bory repl");
    println!("  bory pkg init <name>");
    println!("  bory pkg install <source> [name]");
    println!("  bory pkg list");
    println!("  bory help");
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();

    match args.as_slice() {
        [_bin] => run_repl(),
        [_bin, command] if command == "help" || command == "--help" || command == "-h" => {
            print_help();
            ExitCode::SUCCESS
        }
        [_bin, command] if command == "--version" || command == "-V" => {
            println!("BORY 0.4.0");
            ExitCode::SUCCESS
        }
        [_bin, command] if command == "repl" => run_repl(),
        [_bin, command, file] if command == "run" => run_file(file),
        [_bin, command, file] if command == "check" => check_file(file),
        [_bin, command, file] if command == "fmt" => format_file(file),
        [_bin, command, action, name] if command == "pkg" && action == "init" => pkg_init(name),
        [_bin, command, action, source] if command == "pkg" && action == "install" => {
            pkg_install(source, None)
        }
        [_bin, command, action, source, name]
            if command == "pkg" && action == "install" =>
        {
            pkg_install(source, Some(name))
        }
        [_bin, command, action] if command == "pkg" && action == "list" => pkg_list(),
        [_bin, file] => run_file(file),
        _ => {
            print_help();
            ExitCode::from(1)
        }
    }
}

fn run_file(file: &str) -> ExitCode {
    let mut interpreter = Interpreter::new();
    match interpreter.run_file(Path::new(file)) {
        Ok(_) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}

fn check_file(file: &str) -> ExitCode {
    match std::fs::read_to_string(file) {
        Ok(source) => match check_source(&source, file) {
            Ok(()) => {
                println!("Syntax OK: {file}");
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("{error}");
                ExitCode::from(1)
            }
        },
        Err(error) => {
            eprintln!("Could not read {file}: {error}");
            ExitCode::from(1)
        }
    }
}

fn format_file(file: &str) -> ExitCode {
    match std::fs::read_to_string(file) {
        Ok(source) => match format_source(&source, file) {
            Ok(formatted) => match std::fs::write(file, formatted) {
                Ok(()) => {
                    println!("Formatted: {file}");
                    ExitCode::SUCCESS
                }
                Err(error) => {
                    eprintln!("Could not write {file}: {error}");
                    ExitCode::from(1)
                }
            },
            Err(error) => {
                eprintln!("{error}");
                ExitCode::from(1)
            }
        },
        Err(error) => {
            eprintln!("Could not read {file}: {error}");
            ExitCode::from(1)
        }
    }
}

fn run_repl() -> ExitCode {
    let mut interpreter = Interpreter::new();
    let mut buffer = String::new();
    let mut line = String::new();

    println!("BORY REPL 0.4.0");
    println!("Type :help for commands.");

    loop {
        let prompt = if buffer.is_empty() { "bory> " } else { "....> " };
        print!("{prompt}");
        if io::stdout().flush().is_err() {
            return ExitCode::from(1);
        }

        line.clear();
        match io::stdin().read_line(&mut line) {
            Ok(0) => {
                println!();
                return ExitCode::SUCCESS;
            }
            Ok(_) => {}
            Err(error) => {
                eprintln!("Could not read input: {error}");
                return ExitCode::from(1);
            }
        }

        let trimmed = line.trim_end_matches(&['\r', '\n'][..]);
        if buffer.is_empty() && trimmed.starts_with(':') {
            match trimmed {
                ":quit" | ":exit" => return ExitCode::SUCCESS,
                ":help" => {
                    println!("Commands:");
                    println!("  :help  Show this help");
                    println!("  :quit  Exit the REPL");
                    println!("  :reset Clear the current interpreter state");
                    println!("  :clear Clear the pending multiline buffer");
                    continue;
                }
                ":reset" => {
                    interpreter = Interpreter::new();
                    println!("Interpreter reset.");
                    continue;
                }
                ":clear" => {
                    buffer.clear();
                    continue;
                }
                _ => {
                    println!("Unknown command. Type :help.");
                    continue;
                }
            }
        }

        buffer.push_str(trimmed);
        buffer.push('\n');

        if needs_more_input(&buffer) {
            continue;
        }

        match interpreter.run_source(&buffer, "<repl>") {
            Ok(value) => {
                if value != Value::Nil {
                    println!("{value}");
                }
            }
            Err(error) => eprintln!("{error}"),
        }

        buffer.clear();
    }
}

fn needs_more_input(source: &str) -> bool {
    let mut arrows = 0usize;
    let mut ends = 0usize;
    let mut parens = 0isize;
    let mut braces = 0isize;
    let mut brackets = 0isize;

    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            continue;
        }

        arrows += trimmed.matches("=>").count();
        if trimmed == "end" {
            ends += 1;
        }

        for ch in trimmed.chars() {
            match ch {
                '(' => parens += 1,
                ')' => parens -= 1,
                '{' => braces += 1,
                '}' => braces -= 1,
                '[' => brackets += 1,
                ']' => brackets -= 1,
                _ => {}
            }
        }
    }

    arrows > ends || parens > 0 || braces > 0 || brackets > 0
}

fn pkg_init(name: &str) -> ExitCode {
    let root = packages_dir().join(name);
    if root.exists() {
        eprintln!("Package {name} already exists at {}", root.display());
        return ExitCode::from(1);
    }

    if let Err(error) = fs::create_dir_all(&root) {
        eprintln!("Could not create {}: {error}", root.display());
        return ExitCode::from(1);
    }

    let manifest_path = root.join("bory.pkg.json");
    let entry_path = root.join("main.boy");
    let manifest = format!(
        "{{\n  \"name\": \"{name}\",\n  \"version\": \"0.1.0\",\n  \"entry\": \"main.boy\"\n}}\n"
    );
    let starter = format!(
        "var package_name = \"{name}\"\n\n\
task about() =>\n    give \"package: \" + package_name\nend\n"
    );

    if let Err(error) = fs::write(&manifest_path, manifest) {
        eprintln!("Could not write {}: {error}", manifest_path.display());
        return ExitCode::from(1);
    }

    if let Err(error) = fs::write(&entry_path, starter) {
        eprintln!("Could not write {}: {error}", entry_path.display());
        return ExitCode::from(1);
    }

    println!("Created package {}", root.display());
    ExitCode::SUCCESS
}

fn pkg_install(source: &str, explicit_name: Option<&str>) -> ExitCode {
    let install_root = packages_dir();
    if let Err(error) = fs::create_dir_all(&install_root) {
        eprintln!("Could not create {}: {error}", install_root.display());
        return ExitCode::from(1);
    }

    let is_url = source.starts_with("http://") || source.starts_with("https://");
    let package_name = explicit_name
        .map(ToString::to_string)
        .unwrap_or_else(|| infer_package_name(source));
    let target_dir = install_root.join(&package_name);

    if target_dir.exists() {
        eprintln!("Package {package_name} already exists at {}", target_dir.display());
        return ExitCode::from(1);
    }

    let result = if is_url {
        install_remote_package(source, &target_dir, &package_name)
    } else {
        install_local_package(Path::new(source), &target_dir, &package_name)
    };

    match result {
        Ok(()) => {
            println!("Installed package {package_name} at {}", target_dir.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}

fn pkg_list() -> ExitCode {
    let root = packages_dir();
    if !root.exists() {
        println!("No packages installed.");
        return ExitCode::SUCCESS;
    }

    let mut found = false;
    match fs::read_dir(&root) {
        Ok(entries) => {
            for entry in entries.filter_map(Result::ok) {
                if entry.path().is_dir() {
                    found = true;
                    println!("{}", entry.file_name().to_string_lossy());
                }
            }
        }
        Err(error) => {
            eprintln!("Could not read {}: {error}", root.display());
            return ExitCode::from(1);
        }
    }

    if !found {
        println!("No packages installed.");
    }

    ExitCode::SUCCESS
}

fn packages_dir() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("packages")
}

fn infer_package_name(source: &str) -> String {
    let normalized = source.trim_end_matches(['/', '\\']);
    let last = normalized.rsplit(['/', '\\']).next().unwrap_or("package");
    last.split('.').next().unwrap_or("package").to_string()
}

fn install_remote_package(source: &str, target_dir: &Path, package_name: &str) -> Result<(), String> {
    let response = get(source).map_err(|error| format!("Could not download {source}: {error}"))?;
    if !response.status().is_success() {
        return Err(format!("Download failed with status {}", response.status()));
    }
    let body = response
        .text()
        .map_err(|error| format!("Could not read the response body: {error}"))?;

    fs::create_dir_all(target_dir)
        .map_err(|error| format!("Could not create {}: {error}", target_dir.display()))?;
    fs::write(target_dir.join("main.boy"), body)
        .map_err(|error| format!("Could not write package entrypoint: {error}"))?;
    write_manifest(target_dir, package_name)
}

fn install_local_package(source: &Path, target_dir: &Path, package_name: &str) -> Result<(), String> {
    if !source.exists() {
        return Err(format!("Source {} does not exist", source.display()));
    }

    if source.is_file() {
        fs::create_dir_all(target_dir)
            .map_err(|error| format!("Could not create {}: {error}", target_dir.display()))?;
        fs::copy(source, target_dir.join("main.boy"))
            .map_err(|error| format!("Could not copy {}: {error}", source.display()))?;
        write_manifest(target_dir, package_name)?;
        return Ok(());
    }

    copy_dir_recursive(source, target_dir)?;
    if !target_dir.join("bory.pkg.json").exists() {
        write_manifest(target_dir, package_name)?;
    }
    Ok(())
}

fn write_manifest(target_dir: &Path, package_name: &str) -> Result<(), String> {
    let manifest = format!(
        "{{\n  \"name\": \"{package_name}\",\n  \"version\": \"0.1.0\",\n  \"entry\": \"main.boy\"\n}}\n"
    );
    fs::write(target_dir.join("bory.pkg.json"), manifest)
        .map_err(|error| format!("Could not write manifest: {error}"))
}

fn copy_dir_recursive(source: &Path, target: &Path) -> Result<(), String> {
    fs::create_dir_all(target)
        .map_err(|error| format!("Could not create {}: {error}", target.display()))?;

    for entry in fs::read_dir(source)
        .map_err(|error| format!("Could not read {}: {error}", source.display()))?
    {
        let entry = entry.map_err(|error| format!("Could not read directory entry: {error}"))?;
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());
        if source_path.is_dir() {
            copy_dir_recursive(&source_path, &target_path)?;
        } else {
            fs::copy(&source_path, &target_path).map_err(|error| {
                format!("Could not copy {}: {error}", source_path.display())
            })?;
        }
    }

    Ok(())
}
