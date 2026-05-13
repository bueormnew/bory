use std::cell::RefCell;
use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::Command;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chrono::Local;
use minifb::{Key, MouseButton, MouseMode, Window, WindowOptions};
use reqwest::Method;
use reqwest::blocking::Client;
use serde_json::Value as JsonValue;

use crate::error::BoryError;
use crate::runtime::Interpreter;
use crate::value::{
    JobRef, ListRef, NativeFunction, ObjectRef, Value, values_equal,
};

thread_local! {
    static SCREEN_RUNTIME: RefCell<ScreenRuntime> = RefCell::new(ScreenRuntime::default());
}

#[derive(Default)]
struct ScreenRuntime {
    next_id: u64,
    windows: BTreeMap<u64, ScreenWindow>,
}

struct ScreenWindow {
    window: Window,
    buffer: Vec<u32>,
    width: usize,
    height: usize,
}

pub fn install(interpreter: &mut Interpreter) {
    let globals = [
        ("echo", native("echo", 0, None, builtin_echo)),
        ("ask", native("ask", 0, Some(1), builtin_ask)),
        ("fail", native("fail", 1, Some(2), builtin_fail)),
        ("type_of", native("type_of", 1, Some(1), builtin_type_of)),
        ("size", native("size", 1, Some(1), builtin_size)),
        ("range", native("range", 1, Some(3), builtin_range)),
        ("sum", native("sum", 1, Some(1), builtin_sum)),
        ("mean", native("mean", 1, Some(1), builtin_mean)),
        ("min", native("min", 1, Some(1), builtin_min)),
        ("max", native("max", 1, Some(1), builtin_max)),
        ("abs", native("abs", 1, Some(1), builtin_abs)),
        ("round", native("round", 1, Some(2), builtin_round)),
        ("pow", native("pow", 2, Some(2), builtin_pow)),
        ("push", native("push", 2, None, builtin_push)),
        ("pop", native("pop", 1, Some(2), builtin_pop)),
        ("sort", native("sort", 1, Some(1), builtin_sort)),
        ("reverse", native("reverse", 1, Some(1), builtin_reverse)),
        ("keys", native("keys", 1, Some(1), builtin_keys)),
        ("values", native("values", 1, Some(1), builtin_values)),
        ("has", native("has", 2, Some(2), builtin_has)),
        ("copy", native("copy", 1, Some(1), builtin_copy)),
        ("to_num", native("to_num", 1, Some(1), builtin_to_num)),
        ("to_text", native("to_text", 1, Some(1), builtin_to_text)),
        ("to_bool", native("to_bool", 1, Some(1), builtin_to_bool)),
    ];

    for (name, value) in globals {
        interpreter.define_global(name, value);
    }

    let math = math_module();
    let rand = rand_module();
    let sys = sys_module();
    let json = json_module();
    let text = text_module();
    let matrix = matrix_module();
    let clock = clock_module();
    let net = net_module();
    let flow = flow_module();
    let gc = gc_module();
    let screen = screen_module();

    interpreter.define_global("math", math);
    interpreter.define_global("rand", rand);
    interpreter.define_global("sys", sys);
    interpreter.define_global("json", json);
    interpreter.define_global("text", text);
    interpreter.define_global("matrix", matrix);
    interpreter.define_global("clock", clock);
    interpreter.define_global("net", net.clone());
    interpreter.define_global("http", net);
    interpreter.define_global("flow", flow);
    interpreter.define_global("gc", gc);
    interpreter.define_global("screen", screen);
}

fn native(
    name: &str,
    min_arity: usize,
    max_arity: Option<usize>,
    func: fn(&mut Interpreter, Vec<Value>) -> Result<Value, BoryError>,
) -> Value {
    Value::NativeFunction(Rc::new(NativeFunction::new(name, min_arity, max_arity, func)))
}

fn module(entries: Vec<(&str, Value)>) -> Value {
    let mut data = BTreeMap::new();
    for (key, value) in entries {
        data.insert(key.to_string(), value);
    }
    Value::object(data)
}

fn math_module() -> Value {
    module(vec![
        ("pi", Value::Number(std::f64::consts::PI)),
        ("tau", Value::Number(std::f64::consts::TAU)),
        ("e", Value::Number(std::f64::consts::E)),
        ("sqrt", native("math.sqrt", 1, Some(1), math_sqrt)),
        ("sin", native("math.sin", 1, Some(1), math_sin)),
        ("cos", native("math.cos", 1, Some(1), math_cos)),
        ("tan", native("math.tan", 1, Some(1), math_tan)),
        ("log", native("math.log", 1, Some(2), math_log)),
        ("ln", native("math.ln", 1, Some(1), math_ln)),
        ("floor", native("math.floor", 1, Some(1), math_floor)),
        ("ceil", native("math.ceil", 1, Some(1), math_ceil)),
        ("round", native("math.round", 1, Some(2), math_round)),
        ("abs", native("math.abs", 1, Some(1), math_abs)),
        ("clamp", native("math.clamp", 3, Some(3), math_clamp)),
        ("pow", native("math.pow", 2, Some(2), math_pow)),
    ])
}

fn rand_module() -> Value {
    module(vec![
        ("seed", native("rand.seed", 1, Some(1), rand_seed)),
        ("int", native("rand.int", 2, Some(2), rand_int)),
        ("float", native("rand.float", 2, Some(2), rand_float)),
        ("pick", native("rand.pick", 1, Some(1), rand_pick)),
        ("shuffle", native("rand.shuffle", 1, Some(1), rand_shuffle)),
    ])
}

fn sys_module() -> Value {
    module(vec![
        ("cwd", native("sys.cwd", 0, Some(0), sys_cwd)),
        ("read_text", native("sys.read_text", 1, Some(1), sys_read_text)),
        ("write_text", native("sys.write_text", 2, Some(2), sys_write_text)),
        ("append_text", native("sys.append_text", 2, Some(2), sys_append_text)),
        ("list_dir", native("sys.list_dir", 0, Some(1), sys_list_dir)),
        ("exists", native("sys.exists", 1, Some(1), sys_exists)),
        ("make_dir", native("sys.make_dir", 1, Some(1), sys_make_dir)),
        ("remove", native("sys.remove", 1, Some(1), sys_remove)),
        ("join_path", native("sys.join_path", 0, None, sys_join_path)),
        ("run", native("sys.run", 1, Some(1), sys_run)),
        ("env", native("sys.env", 1, Some(1), sys_env)),
    ])
}

fn json_module() -> Value {
    module(vec![
        ("parse", native("json.parse", 1, Some(1), json_parse)),
        ("stringify", native("json.stringify", 1, Some(2), json_stringify)),
    ])
}

fn text_module() -> Value {
    module(vec![
        ("upper", native("text.upper", 1, Some(1), text_upper)),
        ("lower", native("text.lower", 1, Some(1), text_lower)),
        ("split", native("text.split", 1, Some(2), text_split)),
        ("join", native("text.join", 1, Some(2), text_join)),
        ("replace", native("text.replace", 3, Some(3), text_replace)),
        ("trim", native("text.trim", 1, Some(1), text_trim)),
        ("contains", native("text.contains", 2, Some(2), text_contains)),
    ])
}

fn matrix_module() -> Value {
    module(vec![
        ("zeros", native("matrix.zeros", 1, None, matrix_zeros)),
        ("ones", native("matrix.ones", 1, None, matrix_ones)),
        ("shape", native("matrix.shape", 1, Some(1), matrix_shape)),
        ("flatten", native("matrix.flatten", 1, Some(1), matrix_flatten)),
        ("transpose", native("matrix.transpose", 1, Some(1), matrix_transpose)),
        ("dot", native("matrix.dot", 2, Some(2), matrix_dot)),
        ("matmul", native("matrix.matmul", 2, Some(2), matrix_matmul)),
    ])
}

fn clock_module() -> Value {
    module(vec![
        ("now", native("clock.now", 0, Some(0), clock_now)),
        ("timestamp", native("clock.timestamp", 0, Some(0), clock_timestamp)),
        ("sleep", native("clock.sleep", 1, Some(1), clock_sleep)),
    ])
}

fn net_module() -> Value {
    module(vec![
        ("request", native("net.request", 2, Some(4), net_request)),
        ("get", native("net.get", 1, Some(2), net_get)),
        ("post", native("net.post", 2, Some(3), net_post)),
        ("put", native("net.put", 2, Some(3), net_put)),
        ("delete", native("net.delete", 1, Some(2), net_delete)),
        ("download", native("net.download", 2, Some(3), net_download)),
    ])
}

fn flow_module() -> Value {
    module(vec![
        ("spawn", native("flow.spawn", 1, Some(2), flow_spawn)),
        ("join", native("flow.join", 1, Some(1), flow_join)),
    ])
}

fn gc_module() -> Value {
    module(vec![
        ("stats", native("gc.stats", 0, Some(0), gc_stats)),
        ("collect", native("gc.collect", 0, Some(0), gc_collect)),
    ])
}

fn screen_module() -> Value {
    module(vec![
        ("open", native("screen.open", 2, Some(3), screen_open)),
        ("clear", native("screen.clear", 1, Some(2), screen_clear)),
        ("set", native("screen.set", 4, Some(4), screen_set)),
        ("rect", native("screen.rect", 6, Some(6), screen_rect)),
        ("present", native("screen.present", 1, Some(1), screen_present)),
        ("poll", native("screen.poll", 1, Some(1), screen_poll)),
        ("close", native("screen.close", 1, Some(1), screen_close)),
        ("is_open", native("screen.is_open", 1, Some(1), screen_is_open)),
        ("size", native("screen.size", 1, Some(1), screen_size)),
    ])
}

fn builtin_echo(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    let line = args
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(" ");
    println!("{line}");
    Ok(Value::Nil)
}

fn builtin_ask(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    if let Some(prompt) = args.first() {
        print!("{prompt}");
        io::stdout().flush().map_err(|error| {
            BoryError::runtime(format!("Could not display the prompt: {error}"), None)
        })?;
    }
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|error| BoryError::runtime(format!("Could not read the input: {error}"), None))?;
    Ok(Value::String(
        input.trim_end_matches(&['\r', '\n'][..]).to_string(),
    ))
}

fn builtin_fail(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    let message = args[0].to_string();
    let mut error = BoryError::runtime(message, None);
    if let Some(hint) = args.get(1) {
        error = error.with_hint(hint.to_string());
    }
    Err(error)
}

fn builtin_type_of(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    Ok(Value::String(args[0].type_name().to_string()))
}

fn builtin_size(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    Ok(Value::Number(size_of_value(&args[0])? as f64))
}

fn builtin_range(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    let (start, end, step) = match args.as_slice() {
        [end] => {
            let end = expect_number(end, "range requires numeric arguments")?;
            (0.0, end, if end >= 0.0 { 1.0 } else { -1.0 })
        }
        [start, end] => {
            let start = expect_number(start, "range requires numeric arguments")?;
            let end = expect_number(end, "range requires numeric arguments")?;
            (start, end, if start <= end { 1.0 } else { -1.0 })
        }
        [start, end, step] => (
            expect_number(start, "range requires numeric arguments")?,
            expect_number(end, "range requires numeric arguments")?,
            expect_number(step, "range requires numeric arguments")?,
        ),
        _ => unreachable!(),
    };

    if step == 0.0 {
        return Err(BoryError::runtime("range does not accept step = 0", None));
    }

    let mut current = start;
    let mut values = Vec::new();
    while if step > 0.0 {
        current < end
    } else {
        current > end
    } {
        values.push(Value::Number(current));
        current += step;
    }
    Ok(Value::list(values))
}

fn builtin_sum(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    Ok(Value::Number(number_list(&args[0], "sum")?.iter().sum()))
}

fn builtin_mean(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    let values = number_list(&args[0], "mean")?;
    if values.is_empty() {
        return Err(BoryError::runtime("mean cannot work on an empty list", None));
    }
    Ok(Value::Number(
        values.iter().sum::<f64>() / values.len() as f64,
    ))
}

fn builtin_min(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    min_or_max(&args[0], true)
}

fn builtin_max(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    min_or_max(&args[0], false)
}

fn builtin_abs(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    Ok(Value::Number(
        expect_number(&args[0], "abs expects a number")?.abs(),
    ))
}

fn builtin_round(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    round_impl(&args)
}

fn builtin_pow(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    Ok(Value::Number(
        expect_number(&args[0], "pow expects numbers")?
            .powf(expect_number(&args[1], "pow expects numbers")?),
    ))
}

fn builtin_push(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    let list = expect_list_ref(&args[0], "push")?;
    {
        let mut borrowed = list.borrow_mut();
        for value in args.iter().skip(1) {
            borrowed.push(value.clone());
        }
    }
    Ok(Value::List(list))
}

fn builtin_pop(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    let list = expect_list_ref(&args[0], "pop")?;
    let mut borrowed = list.borrow_mut();
    if borrowed.is_empty() {
        return Err(BoryError::runtime("pop cannot work on an empty list", None));
    }

    if args.len() == 1 {
        borrowed
            .pop()
            .ok_or_else(|| BoryError::runtime("pop failed unexpectedly", None))
    } else {
        let index = normalize_index(
            expect_integer(&args[1], "pop needs an integer index")?,
            borrowed.len(),
        )?;
        Ok(borrowed.remove(index))
    }
}

fn builtin_sort(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    let list = expect_list_ref(&args[0], "sort")?;
    {
        let mut borrowed = list.borrow_mut();
        if borrowed.iter().all(|value| matches!(value, Value::Number(_))) {
            borrowed.sort_by(|a, b| {
                let a = expect_number(a, "sort expects numeric values").unwrap_or(0.0);
                let b = expect_number(b, "sort expects numeric values").unwrap_or(0.0);
                a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal)
            });
        } else if borrowed.iter().all(|value| matches!(value, Value::String(_))) {
            borrowed.sort_by_key(|value| value.to_string());
        } else {
            borrowed.sort_by_key(|value| value.to_string());
        }
    }
    Ok(Value::List(list))
}

fn builtin_reverse(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    match &args[0] {
        Value::List(list) => {
            list.borrow_mut().reverse();
            Ok(Value::List(list.clone()))
        }
        Value::String(text) => Ok(Value::String(text.chars().rev().collect())),
        _ => Err(BoryError::runtime("reverse needs a list or text value", None)),
    }
}

fn builtin_keys(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    let object = expect_object_ref(&args[0], "keys")?;
    Ok(Value::list(
        object
            .borrow()
            .keys()
            .map(|key| Value::String(key.clone()))
            .collect(),
    ))
}

fn builtin_values(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    let object = expect_object_ref(&args[0], "values")?;
    Ok(Value::list(object.borrow().values().cloned().collect()))
}

fn builtin_has(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    let container = &args[0];
    let needle = &args[1];
    let result = match container {
        Value::List(list) => list.borrow().iter().any(|value| values_equal(value, needle)),
        Value::Object(object) => object.borrow().contains_key(&needle.to_string()),
        Value::String(text) => text.contains(&needle.to_string()),
        _ => return Err(BoryError::runtime("has needs a list, object, or text value", None)),
    };
    Ok(Value::Bool(result))
}

fn builtin_copy(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    Ok(args[0].deep_copy())
}

fn builtin_to_num(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    let value = match &args[0] {
        Value::Number(number) => *number,
        Value::Bool(true) => 1.0,
        Value::Bool(false) => 0.0,
        Value::Nil => 0.0,
        Value::String(text) => text
            .trim()
            .parse::<f64>()
            .map_err(|_| BoryError::runtime("Could not convert the text to a number", None))?,
        _ => return Err(BoryError::runtime("Cannot convert that value to a number", None)),
    };
    Ok(Value::Number(value))
}

fn builtin_to_text(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    Ok(Value::String(args[0].to_string()))
}

fn builtin_to_bool(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    Ok(Value::Bool(args[0].is_truthy()))
}

fn math_sqrt(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    Ok(Value::Number(
        expect_number(&args[0], "math.sqrt expects a number")?.sqrt(),
    ))
}

fn math_sin(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    Ok(Value::Number(
        expect_number(&args[0], "math.sin expects a number")?.sin(),
    ))
}

fn math_cos(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    Ok(Value::Number(
        expect_number(&args[0], "math.cos expects a number")?.cos(),
    ))
}

fn math_tan(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    Ok(Value::Number(
        expect_number(&args[0], "math.tan expects a number")?.tan(),
    ))
}

fn math_log(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    let value = expect_number(&args[0], "math.log expects numbers")?;
    let result = if args.len() == 2 {
        let base = expect_number(&args[1], "math.log expects numbers")?;
        value.log(base)
    } else {
        value.log10()
    };
    Ok(Value::Number(result))
}

fn math_ln(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    Ok(Value::Number(
        expect_number(&args[0], "math.ln expects a number")?.ln(),
    ))
}

fn math_floor(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    Ok(Value::Number(
        expect_number(&args[0], "math.floor expects a number")?.floor(),
    ))
}

fn math_ceil(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    Ok(Value::Number(
        expect_number(&args[0], "math.ceil expects a number")?.ceil(),
    ))
}

fn math_round(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    round_impl(&args)
}

fn math_abs(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    builtin_abs(_interpreter, args)
}

fn math_clamp(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    let value = expect_number(&args[0], "math.clamp expects numbers")?;
    let low = expect_number(&args[1], "math.clamp expects numbers")?;
    let high = expect_number(&args[2], "math.clamp expects numbers")?;
    Ok(Value::Number(value.clamp(low, high)))
}

fn math_pow(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    builtin_pow(_interpreter, args)
}

fn rand_seed(interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    interpreter.seed_random(expect_integer(&args[0], "rand.seed needs an integer")? as u64);
    Ok(Value::Nil)
}

fn rand_int(interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    let min = expect_integer(&args[0], "rand.int needs integer arguments")?;
    let max = expect_integer(&args[1], "rand.int needs integer arguments")?;
    if min > max {
        return Err(BoryError::runtime("rand.int needs min <= max", None));
    }
    let span = max - min + 1;
    let value = min + (interpreter.next_random_u64() % span as u64) as i64;
    Ok(Value::Number(value as f64))
}

fn rand_float(interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    let min = expect_number(&args[0], "rand.float needs numeric arguments")?;
    let max = expect_number(&args[1], "rand.float needs numeric arguments")?;
    if min > max {
        return Err(BoryError::runtime("rand.float needs min <= max", None));
    }
    Ok(Value::Number(
        min + interpreter.next_random_f64() * (max - min),
    ))
}

fn rand_pick(interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    match &args[0] {
        Value::List(list) => {
            let borrowed = list.borrow();
            if borrowed.is_empty() {
                return Err(BoryError::runtime("rand.pick does not accept empty lists", None));
            }
            let index = (interpreter.next_random_u64() as usize) % borrowed.len();
            Ok(borrowed[index].clone())
        }
        Value::String(text) => {
            let chars = text.chars().collect::<Vec<_>>();
            if chars.is_empty() {
                return Err(BoryError::runtime("rand.pick does not accept empty text", None));
            }
            let index = (interpreter.next_random_u64() as usize) % chars.len();
            Ok(Value::String(chars[index].to_string()))
        }
        _ => Err(BoryError::runtime("rand.pick needs a list or text value", None)),
    }
}

fn rand_shuffle(interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    let list = expect_list_ref(&args[0], "rand.shuffle")?;
    let len = list.borrow().len();
    {
        let mut borrowed = list.borrow_mut();
        for i in (1..len).rev() {
            let j = (interpreter.next_random_u64() as usize) % (i + 1);
            borrowed.swap(i, j);
        }
    }
    Ok(Value::List(list))
}

fn sys_cwd(interpreter: &mut Interpreter, _args: Vec<Value>) -> Result<Value, BoryError> {
    Ok(Value::String(interpreter.current_base_dir().display().to_string()))
}

fn sys_read_text(interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    let path = interpreter.resolve_path(&to_path_text(&args[0]));
    let content = std::fs::read_to_string(&path)
        .map_err(|error| BoryError::runtime(format!("Could not read {}: {error}", path.display()), None))?;
    Ok(Value::String(content))
}

fn sys_write_text(interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    let path = interpreter.resolve_path(&to_path_text(&args[0]));
    std::fs::write(&path, args[1].to_string())
        .map_err(|error| BoryError::runtime(format!("Could not write {}: {error}", path.display()), None))?;
    Ok(Value::String(path.display().to_string()))
}

fn sys_append_text(interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    let path = interpreter.resolve_path(&to_path_text(&args[0]));
    use std::fs::OpenOptions;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|error| BoryError::runtime(format!("Could not open {}: {error}", path.display()), None))?;
    file.write_all(args[1].to_string().as_bytes())
        .map_err(|error| BoryError::runtime(format!("Could not write {}: {error}", path.display()), None))?;
    Ok(Value::String(path.display().to_string()))
}

fn sys_list_dir(interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    let path = if let Some(first) = args.first() {
        interpreter.resolve_path(&to_path_text(first))
    } else {
        interpreter.current_base_dir()
    };
    let mut entries = std::fs::read_dir(&path)
        .map_err(|error| BoryError::runtime(format!("Could not open {}: {error}", path.display()), None))?
        .filter_map(Result::ok)
        .map(|entry| Value::String(entry.file_name().to_string_lossy().to_string()))
        .collect::<Vec<_>>();
    entries.sort_by_key(|value| value.to_string());
    Ok(Value::list(entries))
}

fn sys_exists(interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    Ok(Value::Bool(
        interpreter.resolve_path(&to_path_text(&args[0])).exists(),
    ))
}

fn sys_make_dir(interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    let path = interpreter.resolve_path(&to_path_text(&args[0]));
    std::fs::create_dir_all(&path)
        .map_err(|error| BoryError::runtime(format!("Could not create {}: {error}", path.display()), None))?;
    Ok(Value::String(path.display().to_string()))
}

fn sys_remove(interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    let path = interpreter.resolve_path(&to_path_text(&args[0]));
    if path.is_dir() {
        std::fs::remove_dir_all(&path)
            .map_err(|error| BoryError::runtime(format!("Could not remove {}: {error}", path.display()), None))?;
    } else if path.exists() {
        std::fs::remove_file(&path)
            .map_err(|error| BoryError::runtime(format!("Could not remove {}: {error}", path.display()), None))?;
    }
    Ok(Value::Bool(true))
}

fn sys_join_path(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    let mut path = PathBuf::new();
    for part in args {
        path.push(to_path_text(&part));
    }
    Ok(Value::String(path.display().to_string()))
}

fn sys_run(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    let command = args[0].to_string();
    let output = if cfg!(windows) {
        Command::new("powershell")
            .args(["-NoProfile", "-Command", &command])
            .output()
    } else {
        Command::new("sh").args(["-lc", &command]).output()
    }
    .map_err(|error| BoryError::runtime(format!("Could not execute the command: {error}"), None))?;

    let mut response = BTreeMap::new();
    response.insert(
        "code".to_string(),
        Value::Number(output.status.code().unwrap_or(-1) as f64),
    );
    response.insert(
        "out".to_string(),
        Value::String(String::from_utf8_lossy(&output.stdout).to_string()),
    );
    response.insert(
        "err".to_string(),
        Value::String(String::from_utf8_lossy(&output.stderr).to_string()),
    );
    Ok(Value::object(response))
}

fn sys_env(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    match std::env::var(args[0].to_string()) {
        Ok(value) => Ok(Value::String(value)),
        Err(_) => Ok(Value::Nil),
    }
}

fn json_parse(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    let parsed: JsonValue = serde_json::from_str(&args[0].to_string())
        .map_err(|error| BoryError::runtime(format!("json.parse failed: {error}"), None))?;
    from_json(parsed)
}

fn json_stringify(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    let json = to_json(&args[0])?;
    let pretty = args.get(1).is_some_and(Value::is_truthy);
    let rendered = if pretty {
        serde_json::to_string_pretty(&json)
    } else {
        serde_json::to_string(&json)
    }
    .map_err(|error| BoryError::runtime(format!("json.stringify failed: {error}"), None))?;
    Ok(Value::String(rendered))
}

fn text_upper(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    Ok(Value::String(args[0].to_string().to_uppercase()))
}

fn text_lower(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    Ok(Value::String(args[0].to_string().to_lowercase()))
}

fn text_split(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    let text = args[0].to_string();
    let parts = if let Some(separator) = args.get(1) {
        text.split(&separator.to_string())
            .map(|part| Value::String(part.to_string()))
            .collect()
    } else {
        text.split_whitespace()
            .map(|part| Value::String(part.to_string()))
            .collect()
    };
    Ok(Value::list(parts))
}

fn text_join(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    let list = expect_list_ref(&args[0], "text.join")?;
    let separator = args.get(1).map_or(String::new(), ToString::to_string);
    let rendered = list
        .borrow()
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(&separator);
    Ok(Value::String(rendered))
}

fn text_replace(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    Ok(Value::String(
        args[0]
            .to_string()
            .replace(&args[1].to_string(), &args[2].to_string()),
    ))
}

fn text_trim(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    Ok(Value::String(args[0].to_string().trim().to_string()))
}

fn text_contains(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    Ok(Value::Bool(
        args[0].to_string().contains(&args[1].to_string()),
    ))
}

fn matrix_zeros(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    build_tensor(&args, Value::Number(0.0))
}

fn matrix_ones(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    build_tensor(&args, Value::Number(1.0))
}

fn matrix_shape(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    Ok(Value::list(
        infer_shape(&args[0])
            .into_iter()
            .map(|size| Value::Number(size as f64))
            .collect(),
    ))
}

fn matrix_flatten(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    let mut output = Vec::new();
    flatten_value(&args[0], &mut output);
    Ok(Value::list(output))
}

fn matrix_transpose(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    let matrix = matrix_rows(&args[0], "matrix.transpose")?;
    if matrix.is_empty() {
        return Ok(Value::list(Vec::new()));
    }
    let cols = matrix[0].len();
    let mut transposed = vec![vec![0.0; matrix.len()]; cols];
    for (row_index, row) in matrix.iter().enumerate() {
        if row.len() != cols {
            return Err(BoryError::runtime("Matrix rows must have the same size", None));
        }
        for (col_index, value) in row.iter().enumerate() {
            transposed[col_index][row_index] = *value;
        }
    }
    Ok(number_matrix_to_value(transposed))
}

fn matrix_dot(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    let left = number_list(&args[0], "matrix.dot")?;
    let right = number_list(&args[1], "matrix.dot")?;
    if left.len() != right.len() {
        return Err(BoryError::runtime(
            "matrix.dot needs vectors with the same size",
            None,
        ));
    }
    Ok(Value::Number(
        left.iter().zip(right.iter()).map(|(a, b)| a * b).sum(),
    ))
}

fn matrix_matmul(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    let left = matrix_rows(&args[0], "matrix.matmul")?;
    let right = matrix_rows(&args[1], "matrix.matmul")?;
    if left.is_empty() || right.is_empty() {
        return Ok(Value::list(Vec::new()));
    }
    let left_cols = left[0].len();
    let right_rows = right.len();
    let right_cols = right[0].len();
    if left.iter().any(|row| row.len() != left_cols) || right.iter().any(|row| row.len() != right_cols) {
        return Err(BoryError::runtime(
            "matrix.matmul needs regular matrices",
            None,
        ));
    }
    if left_cols != right_rows {
        return Err(BoryError::runtime(
            "matrix.matmul needs compatible dimensions",
            None,
        ));
    }
    let mut result = vec![vec![0.0; right_cols]; left.len()];
    for row in 0..left.len() {
        for col in 0..right_cols {
            let mut sum = 0.0;
            for pivot in 0..left_cols {
                sum += left[row][pivot] * right[pivot][col];
            }
            result[row][col] = sum;
        }
    }
    Ok(number_matrix_to_value(result))
}

fn clock_now(_interpreter: &mut Interpreter, _args: Vec<Value>) -> Result<Value, BoryError> {
    Ok(Value::String(
        Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
    ))
}

fn clock_timestamp(_interpreter: &mut Interpreter, _args: Vec<Value>) -> Result<Value, BoryError> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs_f64())
        .map_err(|error| BoryError::runtime(format!("clock.timestamp failed: {error}"), None))?;
    Ok(Value::Number(timestamp))
}

fn clock_sleep(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    let millis = expect_number(&args[0], "clock.sleep needs a number")?;
    if millis < 0.0 {
        return Err(BoryError::runtime(
            "clock.sleep does not accept negative values",
            None,
        ));
    }
    thread::sleep(Duration::from_millis(millis as u64));
    Ok(Value::Nil)
}

fn net_request(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    let method = args[0].to_string().to_uppercase();
    let url = args[1].to_string();
    let body = args.get(2).cloned();
    let headers = args.get(3);
    perform_request(&method, &url, body, headers)
}

fn net_get(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    let url = args[0].to_string();
    let headers = args.get(1);
    perform_request("GET", &url, None, headers)
}

fn net_post(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    let url = args[0].to_string();
    let body = Some(args[1].clone());
    let headers = args.get(2);
    perform_request("POST", &url, body, headers)
}

fn net_put(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    let url = args[0].to_string();
    let body = Some(args[1].clone());
    let headers = args.get(2);
    perform_request("PUT", &url, body, headers)
}

fn net_delete(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    let url = args[0].to_string();
    let headers = args.get(1);
    perform_request("DELETE", &url, None, headers)
}

fn net_download(interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    let url = args[0].to_string();
    let destination = interpreter.resolve_path(&to_path_text(&args[1]));
    let headers = args.get(2);

    let response = perform_request("GET", &url, None, headers)?;
    let body = match response {
        Value::Object(object) => object
            .borrow()
            .get("body")
            .cloned()
            .unwrap_or(Value::String(String::new()))
            .to_string(),
        _ => String::new(),
    };

    std::fs::write(&destination, body).map_err(|error| {
        BoryError::runtime(
            format!("Could not write {}: {error}", destination.display()),
            None,
        )
    })?;

    Ok(Value::String(destination.display().to_string()))
}

fn flow_spawn(interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    let path = interpreter.resolve_path(&to_path_text(&args[0]));
    let payload = args.get(1).map(to_json).transpose()?;
    let job: JobRef = Arc::new(Mutex::new(Some(thread::spawn(move || {
        let mut worker = Interpreter::new();
        if let Some(payload) = payload {
            let input = from_json(payload).map_err(|error| error.to_string())?;
            worker.define_global("input", input);
        }
        let value = worker.run_file(&path).map_err(|error| error.to_string())?;
        to_json(&value).map_err(|error| error.to_string())
    }))));
    Ok(Value::Job(job))
}

fn flow_join(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    let Value::Job(job) = &args[0] else {
        return Err(BoryError::runtime("flow.join expects a job handle", None));
    };

    let handle = job
        .lock()
        .map_err(|_| BoryError::runtime("Could not lock the job handle", None))?
        .take()
        .ok_or_else(|| BoryError::runtime("That job was already joined", None))?;

    match handle.join() {
        Ok(Ok(result)) => {
            let value = from_json(result)?;
            let mut payload = BTreeMap::new();
            payload.insert("ok".to_string(), Value::Bool(true));
            payload.insert("value".to_string(), value);
            Ok(Value::object(payload))
        }
        Ok(Err(error)) => {
            let mut payload = BTreeMap::new();
            payload.insert("ok".to_string(), Value::Bool(false));
            payload.insert("error".to_string(), Value::String(error));
            Ok(Value::object(payload))
        }
        Err(_) => Err(BoryError::runtime(
            "The job thread panicked before completing",
            None,
        )),
    }
}

fn gc_stats(_interpreter: &mut Interpreter, _args: Vec<Value>) -> Result<Value, BoryError> {
    Ok(heap_stats_to_value(_interpreter.heap_stats_public()))
}

fn gc_collect(_interpreter: &mut Interpreter, _args: Vec<Value>) -> Result<Value, BoryError> {
    Ok(heap_stats_to_value(_interpreter.collect_garbage_major()))
}

fn screen_open(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    let width = expect_integer(&args[0], "screen.open needs an integer width")?;
    let height = expect_integer(&args[1], "screen.open needs an integer height")?;
    if width <= 0 || height <= 0 {
        return Err(BoryError::runtime(
            "screen.open needs positive dimensions",
            None,
        ));
    }
    let title = args
        .get(2)
        .map_or_else(|| "BORY Screen".to_string(), ToString::to_string);

    SCREEN_RUNTIME.with(|runtime| {
        let mut runtime = runtime.borrow_mut();
        runtime.next_id += 1;
        let id = runtime.next_id;
        let width = width as usize;
        let height = height as usize;
        let window = Window::new(&title, width, height, WindowOptions::default())
            .map_err(|error| BoryError::runtime(format!("Could not open window: {error}"), None))?;
        runtime.windows.insert(
            id,
            ScreenWindow {
                window,
                buffer: vec![0; width * height],
                width,
                height,
            },
        );
        Ok(screen_handle_value(id, width, height, &title))
    })
}

fn screen_clear(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    let id = screen_id_from_value(&args[0])?;
    let color = args.get(1).map_or(0x000000, parse_color);
    with_screen_window(id, |screen| {
        for pixel in &mut screen.buffer {
            *pixel = color;
        }
        Ok(Value::Nil)
    })
}

fn screen_set(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    let id = screen_id_from_value(&args[0])?;
    let x = expect_integer(&args[1], "screen.set needs an integer x")?;
    let y = expect_integer(&args[2], "screen.set needs an integer y")?;
    let color = parse_color(&args[3]);
    with_screen_window(id, |screen| {
        if x >= 0 && y >= 0 && (x as usize) < screen.width && (y as usize) < screen.height {
            let index = y as usize * screen.width + x as usize;
            screen.buffer[index] = color;
        }
        Ok(Value::Nil)
    })
}

fn screen_rect(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    let id = screen_id_from_value(&args[0])?;
    let x = expect_integer(&args[1], "screen.rect needs an integer x")?;
    let y = expect_integer(&args[2], "screen.rect needs an integer y")?;
    let width = expect_integer(&args[3], "screen.rect needs an integer width")?;
    let height = expect_integer(&args[4], "screen.rect needs an integer height")?;
    let color = parse_color(&args[5]);
    with_screen_window(id, |screen| {
        for row in 0..height.max(0) {
            for col in 0..width.max(0) {
                let px = x + col;
                let py = y + row;
                if px >= 0 && py >= 0 && (px as usize) < screen.width && (py as usize) < screen.height {
                    let index = py as usize * screen.width + px as usize;
                    screen.buffer[index] = color;
                }
            }
        }
        Ok(Value::Nil)
    })
}

fn screen_present(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    let id = screen_id_from_value(&args[0])?;
    with_screen_window(id, |screen| {
        screen
            .window
            .update_with_buffer(&screen.buffer, screen.width, screen.height)
            .map_err(|error| BoryError::runtime(format!("Could not present frame: {error}"), None))?;
        Ok(Value::Nil)
    })
}

fn screen_poll(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    let id = screen_id_from_value(&args[0])?;
    with_screen_window(id, |screen| {
        screen.window.update();
        let keys = screen
            .window
            .get_keys()
            .into_iter()
            .map(|key| Value::String(format!("{key:?}").to_lowercase()))
            .collect::<Vec<_>>();
        let mouse = screen.window.get_mouse_pos(MouseMode::Clamp);
        let mut state = BTreeMap::new();
        state.insert("open".to_string(), Value::Bool(screen.window.is_open()));
        state.insert("width".to_string(), Value::Number(screen.width as f64));
        state.insert("height".to_string(), Value::Number(screen.height as f64));
        state.insert("keys".to_string(), Value::list(keys));
        state.insert(
            "mouse_x".to_string(),
            Value::Number(mouse.map_or(-1.0, |(x, _)| x as f64)),
        );
        state.insert(
            "mouse_y".to_string(),
            Value::Number(mouse.map_or(-1.0, |(_, y)| y as f64)),
        );
        state.insert(
            "mouse_left".to_string(),
            Value::Bool(screen.window.get_mouse_down(MouseButton::Left)),
        );
        state.insert(
            "escape".to_string(),
            Value::Bool(screen.window.is_key_down(Key::Escape)),
        );
        Ok(Value::object(state))
    })
}

fn screen_close(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    let id = screen_id_from_value(&args[0])?;
    SCREEN_RUNTIME.with(|runtime| {
        runtime.borrow_mut().windows.remove(&id);
        Ok(Value::Nil)
    })
}

fn screen_is_open(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    let id = screen_id_from_value(&args[0])?;
    with_screen_window(id, |screen| Ok(Value::Bool(screen.window.is_open())))
}

fn screen_size(_interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, BoryError> {
    let id = screen_id_from_value(&args[0])?;
    with_screen_window(id, |screen| {
        let mut size = BTreeMap::new();
        size.insert("width".to_string(), Value::Number(screen.width as f64));
        size.insert("height".to_string(), Value::Number(screen.height as f64));
        Ok(Value::object(size))
    })
}

fn perform_request(
    method: &str,
    url: &str,
    body: Option<Value>,
    headers: Option<&Value>,
) -> Result<Value, BoryError> {
    let client = Client::builder()
        .build()
        .map_err(|error| BoryError::runtime(format!("Could not create HTTP client: {error}"), None))?;

    let method = Method::from_bytes(method.as_bytes())
        .map_err(|error| BoryError::runtime(format!("Invalid HTTP method: {error}"), None))?;

    let mut request = client.request(method, url);

    if let Some(headers) = headers {
        let header_object = expect_object_ref(headers, "http headers")?;
        for (key, value) in header_object.borrow().iter() {
            request = request.header(key, value.to_string());
        }
    }

    if let Some(body) = body {
        match body {
            Value::Object(_) | Value::List(_) => {
                let payload = serde_json::to_string(&to_json(&body)?)
                    .map_err(|error| BoryError::runtime(format!("Could not encode JSON body: {error}"), None))?;
                request = request
                    .header("content-type", "application/json")
                    .body(payload);
            }
            other => {
                request = request.body(other.to_string());
            }
        }
    }

    let response = request
        .send()
        .map_err(|error| BoryError::runtime(format!("HTTP request failed: {error}"), None))?;
    let status = response.status();
    let final_url = response.url().to_string();
    let mut header_values = BTreeMap::new();
    for (key, value) in response.headers() {
        header_values.insert(
            key.to_string(),
            Value::String(value.to_str().unwrap_or("").to_string()),
        );
    }
    let body = response
        .text()
        .map_err(|error| BoryError::runtime(format!("Could not read the HTTP response body: {error}"), None))?;

    let json = serde_json::from_str::<JsonValue>(&body)
        .ok()
        .map(from_json)
        .transpose()?
        .unwrap_or(Value::Nil);

    let mut result = BTreeMap::new();
    result.insert("status".to_string(), Value::Number(status.as_u16() as f64));
    result.insert("ok".to_string(), Value::Bool(status.is_success()));
    result.insert("url".to_string(), Value::String(final_url));
    result.insert("body".to_string(), Value::String(body));
    result.insert("headers".to_string(), Value::object(header_values));
    result.insert("json".to_string(), json);
    Ok(Value::object(result))
}

fn size_of_value(value: &Value) -> Result<usize, BoryError> {
    match value {
        Value::String(text) => Ok(text.chars().count()),
        Value::List(list) => Ok(list.borrow().len()),
        Value::Object(object) => Ok(object.borrow().len()),
        _ => Err(BoryError::runtime(
            "size needs a text, list, or object value",
            None,
        )),
    }
}

fn number_list(value: &Value, name: &str) -> Result<Vec<f64>, BoryError> {
    let list = expect_list_ref(value, name)?;
    list.borrow()
        .iter()
        .map(|value| expect_number(value, &format!("{name} needs a numeric list")))
        .collect()
}

fn min_or_max(value: &Value, is_min: bool) -> Result<Value, BoryError> {
    let list = expect_list_ref(value, if is_min { "min" } else { "max" })?;
    let borrowed = list.borrow();
    let Some(first) = borrowed.first() else {
        return Err(BoryError::runtime("The list cannot be empty", None));
    };
    if borrowed.iter().all(|value| matches!(value, Value::Number(_))) {
        let mut best = expect_number(first, "The list must contain only numbers")?;
        for value in borrowed.iter().skip(1) {
            let current = expect_number(value, "The list must contain only numbers")?;
            if (is_min && current < best) || (!is_min && current > best) {
                best = current;
            }
        }
        Ok(Value::Number(best))
    } else if borrowed.iter().all(|value| matches!(value, Value::String(_))) {
        let mut best = first.to_string();
        for value in borrowed.iter().skip(1) {
            let current = value.to_string();
            if (is_min && current < best) || (!is_min && current > best) {
                best = current;
            }
        }
        Ok(Value::String(best))
    } else {
        Err(BoryError::runtime(
            "min/max needs a uniform list of numbers or text",
            None,
        ))
    }
}

fn round_impl(args: &[Value]) -> Result<Value, BoryError> {
    let value = expect_number(&args[0], "round expects a number")?;
    let digits = if let Some(digits) = args.get(1) {
        expect_integer(digits, "round expects an integer in digits")?
    } else {
        0
    };
    let factor = 10f64.powi(digits as i32);
    Ok(Value::Number((value * factor).round() / factor))
}

fn expect_number(value: &Value, message: &str) -> Result<f64, BoryError> {
    match value {
        Value::Number(number) => Ok(*number),
        _ => Err(BoryError::runtime(message, None)),
    }
}

fn expect_integer(value: &Value, message: &str) -> Result<i64, BoryError> {
    let number = expect_number(value, message)?;
    if number.fract() != 0.0 {
        return Err(BoryError::runtime(message, None));
    }
    Ok(number as i64)
}

fn expect_list_ref(value: &Value, name: &str) -> Result<ListRef, BoryError> {
    match value {
        Value::List(list) => Ok(list.clone()),
        _ => Err(BoryError::runtime(format!("{name} needs a list"), None)),
    }
}

fn expect_object_ref(value: &Value, name: &str) -> Result<ObjectRef, BoryError> {
    match value {
        Value::Object(object) => Ok(object.clone()),
        _ => Err(BoryError::runtime(format!("{name} needs an object"), None)),
    }
}

fn normalize_index(index: i64, len: usize) -> Result<usize, BoryError> {
    let resolved = if index < 0 { len as i64 + index } else { index };
    if resolved < 0 || resolved as usize >= len {
        return Err(BoryError::runtime("Index out of range", None));
    }
    Ok(resolved as usize)
}

fn to_path_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        _ => value.to_string(),
    }
}

fn heap_stats_to_value(stats: crate::value::HeapStats) -> Value {
    let mut object = BTreeMap::new();
    object.insert("allocations".to_string(), Value::Number(stats.allocations as f64));
    object.insert("tracked_lists".to_string(), Value::Number(stats.tracked_lists as f64));
    object.insert(
        "tracked_objects".to_string(),
        Value::Number(stats.tracked_objects as f64),
    );
    object.insert("active_lists".to_string(), Value::Number(stats.active_lists as f64));
    object.insert(
        "active_objects".to_string(),
        Value::Number(stats.active_objects as f64),
    );
    object.insert(
        "reclaimed_entries".to_string(),
        Value::Number(stats.reclaimed_entries as f64),
    );
    object.insert("sweeps".to_string(), Value::Number(stats.sweeps as f64));
    object.insert("minor_sweeps".to_string(), Value::Number(stats.minor_sweeps as f64));
    object.insert("major_sweeps".to_string(), Value::Number(stats.major_sweeps as f64));
    object.insert(
        "promoted_entries".to_string(),
        Value::Number(stats.promoted_entries as f64),
    );
    object.insert(
        "compacted_entries".to_string(),
        Value::Number(stats.compacted_entries as f64),
    );
    object.insert("gen0_entries".to_string(), Value::Number(stats.gen0_entries as f64));
    object.insert("gen1_entries".to_string(), Value::Number(stats.gen1_entries as f64));
    object.insert("gen2_entries".to_string(), Value::Number(stats.gen2_entries as f64));
    Value::object(object)
}

fn screen_handle_value(id: u64, width: usize, height: usize, title: &str) -> Value {
    let mut object = BTreeMap::new();
    object.insert("id".to_string(), Value::Number(id as f64));
    object.insert("width".to_string(), Value::Number(width as f64));
    object.insert("height".to_string(), Value::Number(height as f64));
    object.insert("title".to_string(), Value::String(title.to_string()));
    Value::object(object)
}

fn screen_id_from_value(value: &Value) -> Result<u64, BoryError> {
    match value {
        Value::Number(number) if *number >= 0.0 && number.fract() == 0.0 => Ok(*number as u64),
        Value::Object(object) => object
            .borrow()
            .get("id")
            .and_then(Value::as_integer)
            .map(|value| value as u64)
            .ok_or_else(|| BoryError::runtime("screen handle is missing a valid numeric id", None)),
        _ => Err(BoryError::runtime(
            "screen functions expect a window handle or numeric id",
            None,
        )),
    }
}

fn with_screen_window(
    id: u64,
    action: impl FnOnce(&mut ScreenWindow) -> Result<Value, BoryError>,
) -> Result<Value, BoryError> {
    SCREEN_RUNTIME.with(|runtime| {
        let mut runtime = runtime.borrow_mut();
        let screen = runtime
            .windows
            .get_mut(&id)
            .ok_or_else(|| BoryError::runtime(format!("Window handle {id} does not exist"), None))?;
        action(screen)
    })
}

fn parse_color(value: &Value) -> u32 {
    match value {
        Value::Number(number) if *number >= 0.0 => *number as u32,
        Value::Object(object) => {
            let borrowed = object.borrow();
            let r = borrowed.get("r").and_then(Value::as_integer).unwrap_or(0) as u32;
            let g = borrowed.get("g").and_then(Value::as_integer).unwrap_or(0) as u32;
            let b = borrowed.get("b").and_then(Value::as_integer).unwrap_or(0) as u32;
            (r << 16) | (g << 8) | b
        }
        _ => 0,
    }
}

fn to_json(value: &Value) -> Result<JsonValue, BoryError> {
    match value {
        Value::Nil => Ok(JsonValue::Null),
        Value::Bool(value) => Ok(JsonValue::Bool(*value)),
        Value::Number(value) => serde_json::Number::from_f64(*value)
            .map(JsonValue::Number)
            .ok_or_else(|| BoryError::runtime("Cannot serialize NaN or infinite values", None)),
        Value::String(value) => Ok(JsonValue::String(value.clone())),
        Value::List(list) => {
            let mut values = Vec::new();
            for item in list.borrow().iter() {
                values.push(to_json(item)?);
            }
            Ok(JsonValue::Array(values))
        }
        Value::Object(object) => {
            let mut values = serde_json::Map::new();
            for (key, value) in object.borrow().iter() {
                values.insert(key.clone(), to_json(value)?);
            }
            Ok(JsonValue::Object(values))
        }
        Value::Type(_) | Value::Function(_) | Value::NativeFunction(_) | Value::Job(_) => Err(BoryError::runtime(
            "Cannot serialize executable values to JSON",
            None,
        )),
    }
}

fn from_json(value: JsonValue) -> Result<Value, BoryError> {
    Ok(match value {
        JsonValue::Null => Value::Nil,
        JsonValue::Bool(value) => Value::Bool(value),
        JsonValue::Number(value) => Value::Number(
            value
                .as_f64()
                .ok_or_else(|| BoryError::runtime("Invalid JSON number", None))?,
        ),
        JsonValue::String(value) => Value::String(value),
        JsonValue::Array(values) => Value::list(
            values
                .into_iter()
                .map(from_json)
                .collect::<Result<Vec<_>, _>>()?,
        ),
        JsonValue::Object(values) => {
            let mut object = BTreeMap::new();
            for (key, value) in values {
                object.insert(key, from_json(value)?);
            }
            Value::object(object)
        }
    })
}

fn build_tensor(args: &[Value], fill: Value) -> Result<Value, BoryError> {
    let dims = args
        .iter()
        .map(|value| {
            let dim = expect_integer(value, "Dimensions must be positive integers")?;
            if dim <= 0 {
                return Err(BoryError::runtime("Dimensions must be greater than 0", None));
            }
            Ok(dim as usize)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(tensor_value(&dims, fill))
}

fn tensor_value(dims: &[usize], fill: Value) -> Value {
    if dims.len() == 1 {
        Value::list((0..dims[0]).map(|_| fill.clone()).collect())
    } else {
        Value::list(
            (0..dims[0])
                .map(|_| tensor_value(&dims[1..], fill.clone()))
                .collect(),
        )
    }
}

fn infer_shape(value: &Value) -> Vec<usize> {
    match value {
        Value::List(list) => {
            let borrowed = list.borrow();
            if borrowed.is_empty() {
                vec![0]
            } else {
                let mut shape = vec![borrowed.len()];
                shape.extend(infer_shape(&borrowed[0]));
                shape
            }
        }
        _ => Vec::new(),
    }
}

fn flatten_value(value: &Value, out: &mut Vec<Value>) {
    match value {
        Value::List(list) => {
            for item in list.borrow().iter() {
                flatten_value(item, out);
            }
        }
        other => out.push(other.clone()),
    }
}

fn matrix_rows(value: &Value, name: &str) -> Result<Vec<Vec<f64>>, BoryError> {
    let rows = expect_list_ref(value, name)?;
    rows.borrow()
        .iter()
        .map(|row| number_list(row, name))
        .collect()
}

fn number_matrix_to_value(matrix: Vec<Vec<f64>>) -> Value {
    Value::list(
        matrix
            .into_iter()
            .map(|row| Value::list(row.into_iter().map(Value::Number).collect()))
            .collect(),
    )
}
