# BORY Architecture

## Current Execution Pipeline

1. `lexer.rs`
   turns source text into tokens, including indentation-aware layout tokens.

2. `parser.rs`
   builds the AST with optional type annotations and indentation blocks.

3. `runtime.rs`
   executes statements, scopes, modules, structs/classes, jobs, and control flow.

4. `vm.rs`
   compiles expressions to bytecode and executes them on a stack VM.

5. `builtins.rs`
   registers standard runtime modules and native capabilities.

6. `format.rs`
   formats parsed AST back into canonical source code.

7. `main.rs`
   exposes the CLI, REPL, and package manager.

## Runtime Shape

- Global environment for the standard runtime.
- Child environments for tasks, modules, and type instances.
- Typed bindings in environments.
- Module cache for formal `use` imports.
- File-relative resolution for `load` and `use`.
- Instance construction for `struct` and `class`.
- Native HTTP access through `reqwest`.
- Lightweight jobs through host threads and isolated interpreters.
- Native simple window backend through `screen`.
- Heap registry for tracked list/object values exposed through `gc`.

## Source Layout

```text
bory/
  src/
    main.rs
    lib.rs
    lexer.rs
    parser.rs
    ast.rs
    runtime.rs
    builtins.rs
    value.rs
    env.rs
    error.rs
    token.rs
    span.rs
    vm.rs
    format.rs
  docs/
  examples/
  tests/
```

## Execution Strategy

The current architecture is intentionally hybrid:

- statements still run through the direct interpreter
- expressions now run through bytecode

That split keeps compatibility high while making runtime evolution more practical. It avoids a risky full-engine rewrite and still establishes:

- a real bytecode representation
- a reusable VM entrypoint
- a place for future optimization passes
- a natural path toward broader VM coverage later
