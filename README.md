# BORY

BORY is a general-purpose language and toolchain focused on simple syntax, fast iteration, and an increasingly capable runtime.

This revision expands the project in three important directions:

- the language now supports optional indentation-based blocks, so `end` is no longer mandatory
- the runtime now has a typed-contract layer, a stack-based bytecode VM for expression execution, and a heap registry exposed through `gc`
- the standard package set now ships with `Bscreen`, `Bgames`, and `Bdata`

The source file format is:

```text
.boy
```

## What Is In This Repository

### Language

- `bory` CLI
- parser, AST runtime, expression bytecode VM, and standard builtins
- modules with `use`
- `load`
- structs and classes
- local and remote package manager
- HTTP/URL access
- lightweight job-based concurrency
- typed variables, typed parameters, typed returns
- richer diagnostics with error codes, file, line, code frame, hint, notes, and trace
- heap inspection through `gc.stats()` and `gc.collect()`

### Included Packages

- `Bscreen`
  - simple window creation and framebuffer drawing through the native `screen` backend
- `Bgames`
  - helpers for sprites, movement, polling input, and button hit-state
- `Bdata`
  - helpers for text, JSON, CSV, and line-oriented file workflows

### IDE

The sibling repository folder `bory-studio` now includes:

- adaptive window sizing so it no longer opens larger than the desktop
- rounded toolbar buttons
- multiple terminal sessions with empty tabs
- brighter syntax colors
- in-editor search
- symbol suggestions while typing
- auto-close pairs such as `()`, `[]`, `{}`, `""`, and `''`

## Build

```powershell
cd .\bory
cargo build --release
```

Release binary:

```text
.\target\release\bory.exe
```

Windows package:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\build_windows_package.ps1
```

Installable output:

```text
.\dist\windows\BORY-0.2.0-windows-x64.zip
```

## CLI

```powershell
bory
bory repl
bory run .\examples\hello.boy
bory check .\examples\types_demo.boy
bory fmt .\examples\types_demo.boy
bory pkg init sample_pkg
bory pkg install .\examples\packages\demo_math demo_math
bory pkg list
```

## Language Overview

### Variables

```boy
var name = "BORY"
var count = 3
var ready = yes
var empty = nil
```

### Typed Variables

```boy
var total: number = 42
var title: text = "benchmark"
var values: list[number] = [1, 2, 3]
```

If the value does not match the declared type, the runtime raises a typed diagnostic with code `TYPE001`.

### Tasks

```boy
task power(base: number, expo: number) -> number =>
    give base ^ expo
```

### Indentation Blocks

Classic `end` blocks still work, but they are no longer required:

```boy
if count > 10 =>
    echo("high")
elif count > 5 =>
    echo("mid")
else =>
    echo("low")
```

This also applies to tasks, loops, structs, and classes.

### Loops

```boy
for i from 0 to 10 step 2 =>
    echo(i)
```

```boy
for item in ["a", "b", "c"] =>
    echo(item)
```

### Structs And Classes

```boy
struct Counter(value: number) =>
    task inc() -> number =>
        self.value += 1
        give self.value

class Label(text: text) =>
    task shout() -> text =>
        give text.upper(self.text)
```

### Modules

```boy
use demo_math
echo(demo_math.square(9))
```

```boy
use "lib/shared.boy" as shared
echo(shared.shared_name)
```

### HTTP And Downloads

```boy
var response = http.get("https://example.com")
echo(response.status)
echo(response.body)
```

```boy
http.download("https://example.com/data.txt", "data.txt")
```

### Concurrency

```boy
var job = flow.spawn("worker.boy", {base: 7})
var done = flow.join(job)
echo(done)
```

### Heap And Runtime Introspection

```boy
var before = gc.stats()
var after = gc.collect()
echo(before)
echo(after)
```

`gc.collect()` currently sweeps the runtime heap registry and removes released list/object entries from tracking metadata. It is a practical runtime-management foundation, not a final compacting collector.

### New Screen And Game Packages

```boy
use Bscreen as bs

var win = bs.open(320, 240, "BORY Screen")
bs.clear(win, bs.rgb(15, 24, 40))
bs.rect(win, 24, 24, 80, 50, bs.rgb(90, 150, 255))
bs.present(win)
```

```boy
use Bgames as game

var win = game.open(200, 140, "Input Test")
var state = game.input(win)
echo(state.keys)
```

### Data Package

```boy
use Bdata as data

var payload = {name: "bory", version: 1}
data.write_json("project.json", payload)
var restored = data.read_json("project.json")
echo(restored.name)
```

## Runtime Notes

### Bytecode VM

BORY now executes expressions through a stack-based bytecode VM. The control-flow runtime remains the existing statement interpreter, but repeated expression work now goes through a compiled bytecode path instead of only recursive AST walking.

This is an engine foundation step. It improves the architecture immediately and opens a cleaner path toward:

- deeper VM coverage
- bytecode dumping/debugging
- more aggressive optimization passes
- future AOT/JIT research

### Types

The type system is intentionally pragmatic at this stage:

- typed variables
- typed parameters
- typed task returns
- typed struct/class constructor fields
- list element typing with `list[T]`
- custom instance checks by type name

Supported examples:

```boy
number
text
bool
nil
any
list[number]
Player
```

### Diagnostics

Diagnostics now include:

- error kind
- error code
- file name
- line and column
- code frame
- hint
- notes
- execution trace

Example shape:

```text
[runtime:TYPE001] main.boy 4:1 Variable 'name' expected type 'number' but received 'text'
   4 | var name: number = "bory"
     | ^
hint: Adjust the declared type or pass a value with the expected shape
note: Runtime value: bory
```

## Package Manager

Create a package skeleton:

```powershell
bory pkg init demo_math
```

Install a local package file:

```powershell
bory pkg install .\mathkit.boy mathkit
```

Install a remote package entrypoint:

```powershell
bory pkg install https://example.com/calc.boy calc_remote
```

Installed packages are stored in:

```text
.\packages\
```

## Standard Runtime Modules

- `math`
- `rand`
- `sys`
- `json`
- `text`
- `matrix`
- `clock`
- `net`
- `http`
- `flow`
- `gc`
- `screen`

## Documentation

- [English Guide](C:\Users\gerso\Documents\bory\bory\docs\GUIDE_EN.md)
- [Guia En Espanol](C:\Users\gerso\Documents\bory\bory\docs\GUIDE_ES.md)
- [Syntax Reference](C:\Users\gerso\Documents\bory\bory\docs\SYNTAX.md)
- [Architecture](C:\Users\gerso\Documents\bory\bory\docs\ARCHITECTURE.md)
- [Diagnostics](C:\Users\gerso\Documents\bory\bory\docs\ERRORS.md)
- [Runtime Roadmap 2026](C:\Users\gerso\Documents\bory\bory\docs\ROADMAP_2026.md)
- [BORY Studio](C:\Users\gerso\Documents\bory\bory-studio\README.md)

## Examples

- [hello.boy](C:\Users\gerso\Documents\bory\bory\examples\hello.boy)
- [data_lab.boy](C:\Users\gerso\Documents\bory\bory\examples\data_lab.boy)
- [load_demo.boy](C:\Users\gerso\Documents\bory\bory\examples\load_demo.boy)
- [modules_demo.boy](C:\Users\gerso\Documents\bory\bory\examples\modules_demo.boy)
- [types_demo.boy](C:\Users\gerso\Documents\bory\bory\examples\types_demo.boy)

## Validation

Validated in this repository with:

- `cargo test`
- CLI tests
- runtime tests
- indentation-block tests
- typed-contract tests
- heap/gc tests
- BORY Studio unit tests through bundled Python
