# BORY Diagnostics

## Current Diagnostic Output

Each error can carry:

- error kind
- error code
- file name
- line and column
- source line
- caret marker
- optional hint
- optional notes
- execution trace

Example shape:

```text
[runtime:RUNTIME001] main.boy 4:7 Variable 'score' does not exist
   4 | score += 1
     |       ^
hint: Declare it first with: var score = ...
trace:
  at task tick()
```

Type-contract failures now follow the same format:

```text
[runtime:TYPE001] main.boy 2:1 Variable 'name' expected type 'number' but received 'text'
   2 | var name: number = "bory"
     | ^
hint: Adjust the declared type or pass a value with the expected shape
note: Runtime value: bory
```

## Common Failures

### Invalid assignment target

```boy
3 = 9
```

### Missing variable

```boy
echo(user_name)
```

### Index out of range

```boy
var items = [1]
echo(items[9])
```

### Calling a non callable value

```boy
var data = {name: "bory"}
data()
```

### Broken module path

```boy
use missing_pkg
```
