use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use bory::{BoryErrorKind, Interpreter, Value, check_source};

#[test]
fn executes_control_flow_and_functions() {
    let mut interpreter = Interpreter::new();
    interpreter
        .run_source(
            r#"
var total = 0
for i from 1 to 6 =>
    total += i
end

task double(x) =>
    give x * 2
end

var result = double(total)
"#,
            "memory",
        )
        .unwrap();

    assert_eq!(interpreter.get_global("total"), Some(Value::Number(15.0)));
    assert_eq!(interpreter.get_global("result"), Some(Value::Number(30.0)));
}

#[test]
fn mutates_lists_and_objects() {
    let mut interpreter = Interpreter::new();
    interpreter
        .run_source(
            r#"
var items = [1, 2, 3]
push(items, 4, 5)
items[0] = 99

var profile = {name: "bory"}
profile.level = 3
"#,
            "memory",
        )
        .unwrap();

    let items = interpreter.get_global("items").unwrap();
    match items {
        Value::List(list) => {
            let borrowed = list.borrow();
            assert_eq!(borrowed.len(), 5);
            assert_eq!(borrowed[0], Value::Number(99.0));
            assert_eq!(borrowed[4], Value::Number(5.0));
        }
        other => panic!("expected list and got {other:?}"),
    }

    let profile = interpreter.get_global("profile").unwrap();
    match profile {
        Value::Object(map) => {
            let borrowed = map.borrow();
            assert_eq!(borrowed.get("name"), Some(&Value::String("bory".to_string())));
            assert_eq!(borrowed.get("level"), Some(&Value::Number(3.0)));
        }
        other => panic!("expected object and got {other:?}"),
    }
}

#[test]
fn loads_other_files_and_uses_modules() {
    let base = unique_temp_dir();
    fs::create_dir_all(&base).unwrap();

    let lib_path = base.join("shared.boy");
    let main_path = base.join("main.boy");

    fs::write(
        &lib_path,
        r#"
var from_lib = 41
task plus_one(x) =>
    give x + 1
end
"#,
    )
    .unwrap();

    fs::write(
        &main_path,
        r#"
load "shared.boy"
var answer = plus_one(from_lib)
var payload = json.parse("{\"ok\": true, \"v\": 3}")
"#,
    )
    .unwrap();

    let mut interpreter = Interpreter::new();
    interpreter.run_file(&main_path).unwrap();

    assert_eq!(interpreter.get_global("answer"), Some(Value::Number(42.0)));

    let payload = interpreter.get_global("payload").unwrap();
    match payload {
        Value::Object(map) => {
            let borrowed = map.borrow();
            assert_eq!(borrowed.get("ok"), Some(&Value::Bool(true)));
            assert_eq!(borrowed.get("v"), Some(&Value::Number(3.0)));
        }
        other => panic!("expected object and got {other:?}"),
    }

    let _ = fs::remove_dir_all(base);
}

#[test]
fn supports_multiline_literals_and_matrix_runtime() {
    let mut interpreter = Interpreter::new();
    interpreter
        .run_source(
            r#"
var grid = [
    [1, 2],
    [3, 4]
]

var profile = {
    name: "BORY",
    version: 1,
    matrix: matrix.transpose(grid)
}
"#,
            "memory",
        )
        .unwrap();

    let profile = interpreter.get_global("profile").unwrap();
    match profile {
        Value::Object(map) => {
            let borrowed = map.borrow();
            assert_eq!(borrowed.get("name"), Some(&Value::String("BORY".to_string())));
            match borrowed.get("matrix").unwrap() {
                Value::List(rows) => {
                    let rows = rows.borrow();
                    assert_eq!(rows.len(), 2);
                }
                other => panic!("expected matrix list and got {other:?}"),
            }
        }
        other => panic!("expected object and got {other:?}"),
    }
}

#[test]
fn imports_formal_modules_with_use() {
    let base = unique_temp_dir();
    let package_dir = base.join("packages").join("statskit");
    fs::create_dir_all(&package_dir).unwrap();

    fs::write(
        package_dir.join("main.boy"),
        r#"
task avg(values) =>
    give mean(values)
end

var label = "statskit"
"#,
    )
    .unwrap();

    fs::write(
        base.join("main.boy"),
        r#"
use statskit
var result = statskit.avg([5, 10, 15])
var label = statskit.label
"#,
    )
    .unwrap();

    let mut interpreter = Interpreter::new();
    interpreter.run_file(&base.join("main.boy")).unwrap();

    assert_eq!(interpreter.get_global("result"), Some(Value::Number(10.0)));
    assert_eq!(
        interpreter.get_global("label"),
        Some(Value::String("statskit".to_string()))
    );
}

#[test]
fn supports_structs_and_classes() {
    let mut interpreter = Interpreter::new();
    interpreter
        .run_source(
            r#"
struct Counter(value) =>
    task inc() =>
        self.value += 1
        give self.value
    end
end

class Label(text) =>
    task shout() =>
        give text.upper(self.text)
    end
end

var counter = Counter(4)
var first = counter.inc()
var second = counter.inc()
var label = Label("bory")
var loud = label.shout()
"#,
            "memory",
        )
        .unwrap();

    assert_eq!(interpreter.get_global("first"), Some(Value::Number(5.0)));
    assert_eq!(interpreter.get_global("second"), Some(Value::Number(6.0)));
    assert_eq!(interpreter.get_global("loud"), Some(Value::String("BORY".to_string())));
}

#[test]
fn supports_http_and_downloads() {
    let base = unique_temp_dir();
    fs::create_dir_all(&base).unwrap();
    let (url, server) = start_test_http_server(2);

    let main_source = format!(
        r#"
var response = http.get("{url}/json")
var status = response.status
var ok = response.ok
var payload = response.json
var saved = http.download("{url}/file", "download.txt")
"#
    );

    let main_path = base.join("main.boy");
    fs::write(&main_path, main_source).unwrap();

    let mut interpreter = Interpreter::new();
    interpreter.run_file(&main_path).unwrap();

    assert_eq!(interpreter.get_global("status"), Some(Value::Number(200.0)));
    assert_eq!(interpreter.get_global("ok"), Some(Value::Bool(true)));

    let payload = interpreter.get_global("payload").unwrap();
    match payload {
        Value::Object(map) => {
            let borrowed = map.borrow();
            assert_eq!(borrowed.get("ok"), Some(&Value::Bool(true)));
            assert_eq!(borrowed.get("name"), Some(&Value::String("bory".to_string())));
        }
        other => panic!("expected object and got {other:?}"),
    }

    let downloaded = fs::read_to_string(base.join("download.txt")).unwrap();
    assert_eq!(downloaded, "asset from server");

    server.join().unwrap();
}

#[test]
fn supports_lightweight_concurrency() {
    let base = unique_temp_dir();
    fs::create_dir_all(&base).unwrap();

    fs::write(
        base.join("worker.boy"),
        r#"
var result = input.base * 6
result
"#,
    )
    .unwrap();

    fs::write(
        base.join("main.boy"),
        r#"
var job = flow.spawn("worker.boy", {base: 7})
var done = flow.join(job)
"#,
    )
    .unwrap();

    let mut interpreter = Interpreter::new();
    interpreter.run_file(&base.join("main.boy")).unwrap();

    let done = interpreter.get_global("done").unwrap();
    match done {
        Value::Object(map) => {
            let borrowed = map.borrow();
            assert_eq!(borrowed.get("ok"), Some(&Value::Bool(true)));
            assert_eq!(borrowed.get("value"), Some(&Value::Number(42.0)));
        }
        other => panic!("expected object and got {other:?}"),
    }
}

#[test]
fn renders_parser_errors_with_code_frames() {
    let error = check_source("3 = 9\n", "memory").unwrap_err();
    assert!(matches!(error.kind, BoryErrorKind::Parse));

    let rendered = error.to_string();
    assert!(rendered.contains("memory 1:1"));
    assert!(rendered.contains("3 = 9"));
    assert!(rendered.contains("^"));
}

#[test]
fn supports_indent_blocks_without_end() {
    let mut interpreter = Interpreter::new();
    interpreter
        .run_source(
            r#"
var total = 0
for i from 1 to 5 =>
    if i > 2 =>
        total += i
"#,
            "memory",
        )
        .unwrap();

    assert_eq!(interpreter.get_global("total"), Some(Value::Number(7.0)));
}

#[test]
fn enforces_typed_variables_and_task_contracts() {
    let mut interpreter = Interpreter::new();
    interpreter
        .run_source(
            r#"
var total: number = 21
task scale(value: number) -> number =>
    give value * 2

var result: number = scale(total)
"#,
            "memory",
        )
        .unwrap();

    assert_eq!(interpreter.get_global("result"), Some(Value::Number(42.0)));

    let error = interpreter
        .run_source(
            r#"
var name: number = "bory"
"#,
            "memory",
        )
        .unwrap_err();
    assert!(error.to_string().contains("TYPECHECK001"));
}

#[test]
fn exposes_gc_runtime_stats() {
    let mut interpreter = Interpreter::new();
    interpreter
        .run_source(
            r#"
var a = [1, 2, 3]
var b = {ok: yes}
var before = gc.stats()
var after = gc.collect()
"#,
            "memory",
        )
        .unwrap();

    let before = interpreter.get_global("before").unwrap();
    let after = interpreter.get_global("after").unwrap();
    assert!(matches!(before, Value::Object(_)));
    assert!(matches!(after, Value::Object(_)));
}

fn unique_temp_dir() -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("bory-test-{stamp}"))
}

fn start_test_http_server(max_requests: usize) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let url = format!("http://{}", address);

    let handle = thread::spawn(move || {
        for _ in 0..max_requests {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0u8; 4096];
            let read = stream.read(&mut buffer).unwrap();
            let request = String::from_utf8_lossy(&buffer[..read]);
            let path = request
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or("/");

            let (body, content_type) = match path {
                "/json" => (r#"{"ok":true,"name":"bory"}"#, "application/json"),
                "/file" => ("asset from server", "text/plain"),
                _ => ("missing", "text/plain"),
            };

            let status = if path == "/missing" { "404 Not Found" } else { "200 OK" };
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).unwrap();
        }
    });

    (url, handle)
}
