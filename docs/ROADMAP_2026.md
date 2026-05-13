# Runtime Roadmap 2026

The current repository already ships:

- formal modules
- richer diagnostics
- REPL
- package manager
- structs/classes
- lightweight job concurrency

The deeper runtime rewrite items are planned as separate milestones:

## Planned Runtime Milestones

### 1. Dedicated Garbage Collector

Goal:

- remove the current host-memory dependence for language values
- support longer-lived heaps and future VM execution

### 2. Bytecode Compiler And VM

Goal:

- compile the AST into bytecode
- run programs on a dedicated VM instead of only the direct tree-walking runtime

### 3. AOT Build Targets

Goal:

- add `bory build`
- emit portable bytecode packages and later native bundles

### 4. JIT Research Track

Goal:

- evaluate a practical JIT path once the bytecode VM is stable

These milestones are not being claimed as complete in the current runtime. They need a larger engine pass and should be landed as explicit versions instead of hidden half-steps.

