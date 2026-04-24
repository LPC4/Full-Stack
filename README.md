# Full Stack

[![Live Site](https://img.shields.io/badge/Live%20Site-GitHub%20Pages-2ea44f?logo=github)](https://lpc4.github.io/Full-Stack/)
[![Deploy Pages](https://github.com/LPC4/Full-Stack/actions/workflows/pages.yml/badge.svg)](https://github.com/LPC4/Full-Stack/actions/workflows/pages.yml)

An interactive Rust project that explores the stack from high-level language features down through IR, assembly, and eventually machine execution.

It ships as a desktop/web UI built with `eframe` + `egui`, plus a growing compiler pipeline with fixtures and golden tests.

## What’s in here

- **High-level language front end**: lexer, parser, AST, semantic analysis, and HLL-to-IR lowering.
- **Intermediate language**: a textual IR with aggregate types, pointers, control flow, and snapshot tests.
- **Assembly direction**: the current bridge toward a minimal RISC-V backend.
- **UI + documentation**: a small app for exploring the project plus checked-in examples and fixtures.

## Examples

### 1) Simple HLL program

`programs/example/core_syntax.hll`

```hll
const ZERO = 0
const FIVE = 5

identity: (value: i32) -> i32 {
    return value
}

main: () -> i32 {
    start: i32 = ZERO
    next: i32 = identity(FIVE)
    return start + next
}
```

### 2) Pointer-heavy example with generated IR

`programs/debug/debug.hll`

```hll
type Node = {
    val: i32,
    next: Node*
}

main: () -> i32 {
    ptr: i32* = new(i32)
    x: i32 = 5
    addr: i32* = &x
    @ptr = @addr + 10
    defer free(ptr)
    if @ptr > 10 {
        return 1
    }
    return 0
}
```

`programs/debug/debug.ir`

```ir
type Node = {i32, Node*}

define i32 main() {
entry:
    $x = stack_alloc i32
    write i32 5 @ $x
    $addr = stack_alloc i32*
    write i32* $x @ $addr
    $1 = read i32* @ $addr
    $2 = read i32 @ $1
    $3 = math add i32 $2, 10
}
```

## Quick start

### Run locally

```powershell
cargo run --release
```

### Run in the browser

```powershell
rustup target add wasm32-unknown-unknown
cargo install --locked trunk
trunk serve
```

Open `http://127.0.0.1:8080/index.html#dev` to bypass service-worker caching during development.

### Build a release web bundle

```powershell
trunk build --release
```

## Deployment

This repo deploys through `.github/workflows/pages.yml` with GitHub Actions + GitHub Pages.

1. Set GitHub Pages source to **GitHub Actions**.
2. Push to the configured deployment branch.
3. Check the workflow run in the **Actions** tab.

If your default branch changes, update the branch list in `.github/workflows/pages.yml`.

