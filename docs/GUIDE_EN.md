# BORY Guide (English)

## Overview

BORY is designed around one central idea: keep the syntax direct, remove ceremonial friction, and gradually push the runtime toward a more serious execution model without losing readability.

This version introduces:

- optional indentation-based blocks
- typed variable, parameter, return, and constructor contracts
- a bytecode VM for expression execution
- runtime heap introspection through `gc`
- bundled packages for screen, games, and data work

## Core Syntax

### Variables

```boy
var score = 10
var title = "demo"
var ready = yes
var nothing = nil
```

### Typed Variables

```boy
var score: number = 10
var title: text = "demo"
var flags: list[text] = ["fast", "safe"]
```

### Tasks

```boy
task add(a: number, b: number) -> number =>
    give a + b
```

### Control Flow

```boy
if score > 50 =>
    echo("high")
else =>
    echo("low")
```

`end` still works, but indentation alone is valid now.

### Loops

```boy
for i from 0 to 5 =>
    echo(i)
```

```boy
for item in ["a", "b", "c"] =>
    echo(item)
```

### Structs And Classes

```boy
struct Vec2(x: number, y: number) =>
    task moved(dx: number, dy: number) -> Vec2 =>
        give Vec2(self.x + dx, self.y + dy)
```

## Type System

The current type layer is runtime-enforced, not a full static compiler pass. That still gives immediate practical value:

- bad values are rejected at the declaration boundary
- function/task contracts are explicit
- constructor field contracts are explicit
- list element expectations are possible with `list[T]`

Supported type forms:

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
- `job`
- custom names such as `Player`, `Counter`, `Vec2`

## VM And Runtime

### Expression VM

BORY now compiles expressions to bytecode and runs them on a stack VM. This reduces repeated recursive AST walking in the hot expression path and gives the project a real VM foothold without rewriting the entire statement runtime in one risky pass.

The current split is:

- statements: existing interpreter
- expressions: bytecode VM

This is a deliberate transitional architecture.

### Heap Introspection

The runtime now tracks list/object allocations in a heap registry. The `gc` module exposes that state:

```boy
var stats = gc.stats()
echo(stats.active_lists)
echo(stats.active_objects)

var after = gc.collect()
echo(after.reclaimed_entries)
```

## Standard Modules

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

## Bundled Packages

### Bscreen

High-level wrapper around the native `screen` module.

Main responsibilities:

- create a window
- clear the framebuffer
- set pixels
- draw filled rectangles
- present the current frame
- poll window/input state

Example:

```boy
use Bscreen as bs

var win = bs.open(320, 240, "Bscreen Demo")
bs.clear(win, bs.rgb(15, 24, 40))
bs.rect(win, 16, 16, 80, 60, bs.rgb(90, 150, 255))
bs.present(win)
```

### Bgames

Utility layer on top of `Bscreen`.

Main responsibilities:

- sprite-style matrix drawing
- simple movement helpers
- keyboard polling helpers
- button hit-state helpers

Example:

```boy
use Bgames as game

var player = [
    [0, 0, 255],
    [255, 255, 255],
    [0, 255, 0]
]

var win = game.open(200, 140, "Game Demo")
game.clear(win, game.colors.black)
game.sprite(win, player, 30, 20)
game.present(win)
```

### Bdata

Data and file helper package.

Main responsibilities:

- read/write text
- append text
- read/write JSON
- read lines
- CSV header/row parsing
- write CSV from object rows

Example:

```boy
use Bdata as data

data.write_json("save.json", {name: "bory", score: 99})
var save = data.read_json("save.json")
echo(save.score)
```

## CLI Workflow

```powershell
bory run .\main.boy
bory check .\main.boy
bory fmt .\main.boy
bory pkg list
```

## Studio Workflow

The bundled Studio is intentionally lightweight and now includes:

- workspace explorer
- brighter highlighting
- search
- suggestions from local symbols and builtins
- automatic pair insertion
- multiple terminal sessions

## Diagnostics

Errors now carry:

- kind
- code
- source location
- code frame
- hint
- notes
- trace

This makes typed-contract failures and nested task/native failures much easier to read.
