# Signal Alt

Modern Signal alternative built with [Dioxus 0.5](https://dioxuslabs.com/).

## UI
- Liquid glass (backdrop blur, frosted glass)
- Material Design 3 expressive colors
- Responsive: web, desktop (PC), Android

## Tech
- Rust backend + Dioxus frontend
- Native OS webview / WGPU render
- Small bundle, low memory

## Build
GitHub Actions builds desktop binaries for Linux, macOS, and Windows on pull requests targeting `main`, when run manually, and when a `v*` tag is pushed. Tagged builds are attached to a GitHub Release.

## Run
```bash
cargo run -p signal-alt --features desktop
```
