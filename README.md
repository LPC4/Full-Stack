# Full Stack

[![Live Site](https://img.shields.io/badge/Live%20Site-GitHub%20Pages-2ea44f?logo=github)](https://lpc4.github.io/Full-Stack/)
[![Deploy Pages](https://github.com/LPC4/Full-Stack/actions/workflows/pages.yml/badge.svg)](https://github.com/LPC4/Full-Stack/actions/workflows/pages.yml)

Desktop and web UI app built with `eframe` + `egui`.

## Description

This project is an interactive learning site that walks through the computer stack from first principles: logic gates, combinational/sequential circuits, simple CPU architecture, assembly language, and compiler basics.

The goal is to make each layer visual and connected so users can see how high-level code eventually becomes low-level machine behavior.

## Project metadata

- Crate name: `full_stack`
- Author: `Liam De Koster`
- Rust edition: `2024`

## Run locally (Windows)

```powershell
cargo run --release
```

## Run on the web (WASM)

```powershell
rustup target add wasm32-unknown-unknown
cargo install --locked trunk
trunk serve
```

Open `http://127.0.0.1:8080/index.html#dev`.

Using `#dev` bypasses service worker caching so you always load the newest build during development.

## Build release web assets

```powershell
trunk build --release
```

## Deploy to GitHub Pages

This repo includes a workflow at `.github/workflows/pages.yml` that builds with Trunk and deploys via official GitHub Pages actions.

1. In GitHub, open your repository `Settings` -> `Pages`.
2. Set `Source` to `GitHub Actions`.
3. Push to your deploy branch (currently `main`) to trigger deployment.
4. Check the `Actions` tab for the `Deploy Pages` workflow run.

If your default branch is not `main`, update `.github/workflows/pages.yml`:

```yml
on:
  push:
    branches:
      - <your-branch>
```

### Optional local release build before pushing

```powershell
trunk build --release
```
