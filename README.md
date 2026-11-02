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
GitHub Actions builds a Debian 13 `.deb` package, a sideloadable Android APK, a static web bundle, and macOS and Windows desktop binaries on pull requests targeting `main`, when run manually, and when a `v*` tag is pushed. Tagged builds are attached to a GitHub Release. The Android artifact is debug-signed for testing and is not a Play Store release.

## Run
```bash
cargo run -p signal-alt --features desktop
```
