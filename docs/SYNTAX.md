# BORY Syntax

## Keywords

- `var`
- `task`
- `struct`
- `class`
- `use`
- `as`
- `give`
- `if`
- `elif`
- `else`
- `while`
- `for`
- `in`
- `from`
- `to`
- `step`
- `end`
- `load`
- `stop`
- `skip`
- `yes`
- `no`
- `nil`

## Blocks

BORY opens blocks with `=>` and closes them with `end`.

`end` is now optional when the block is clearly delimited by indentation.

```boy
if yes =>
    echo("ok")
```

## Variables

```boy
var total = 0
var score: number = 10
var items = [1, 2, 3]
var config = {
    mode: "prod",
    active: yes
}
```

## Tasks With Types

```boy
task add(a: number, b: number) -> number =>
    give a + b
```

## Expressions

Supported operators:

- `+`
- `-`
- `*`
- `/`
- `%`
- `^`
- `==`
- `!=`
- `>`
- `>=`
- `<`
- `<=`
- `and`
- `or`
- `not`
- `in`

## Types

- `number`
- `text`
- `bool`
- `nil`
- `any`
- `list`
- `list[number]`
- `object`
- `task`
- `native-task`
- `struct`
- `class`
- `job`

## Module Imports

```boy
use demo_math
use demo_math as dm
use "lib/shared.boy" as shared
```

Search order for module names:

- current directory
- `lib`
- `libs`
- `packages`
- `stdlib`

Accepted entrypoints:

- `<name>.boy`
- `<name>/main.boy`
- `<name>/mod.boy`

## Structs And Classes

```boy
struct Counter(value) =>
    task inc() =>
        self.value += 1
        give self.value
    end
end
```

```boy
class Label(text) =>
    task shout() =>
        give text.upper(self.text)
    end
end
```

Fields are available through `self`.

## Concurrency

```boy
var job = flow.spawn("worker.boy", {base: 9})
var done = flow.join(job)
```
