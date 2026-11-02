# Koking

Koking is a self-hostable messaging app built with [Dioxus](https://dioxuslabs.com/) and a Rust API.

## UI
- Zed-inspired desktop workspace with Material-style layout and restrained Liquid Glass surfaces
- Responsive web, desktop and Android clients
- Username/password account creation and sign-in
- User search, direct conversations, persisted messages and 2.5-second message refresh

## Tech
- Axum Rust API + SQLite persistence + Dioxus frontend
- Native OS webview / WGPU render
- Small bundle, low memory

## Run locally

Start the API in one terminal. By default it listens only on `127.0.0.1:3000` and stores data in `koking.db` in the current directory.

```bash
cargo run -p koking-server
```

In a second terminal, run the web app:

```bash
cargo install dioxus-cli --version 0.6.3
KOKING_API_URL=http://127.0.0.1:3000 dx serve --platform web --package signal-alt
```

Open `http://127.0.0.1:8080`, create an account, then search for another username on the same server to start a conversation. You can run a second browser profile to try messaging between two accounts. The API exposes `GET /health` for its health check.

## Self-host the server

The server reads `KOKING_BIND`, `KOKING_DATABASE_URL`, `KOKING_ALLOWED_ORIGINS`, and `RUST_LOG`. See [.env.server.example](.env.server.example). For a local Docker server:

```bash
KOKING_ALLOWED_ORIGINS=http://127.0.0.1:8080 docker compose -f compose.server.yml up --build -d
```

The container publishes port 3000 on loopback and keeps SQLite data in a named volume. For remote clients, put the API behind HTTPS (the sample [Caddyfile](deploy/Caddyfile.example) proxies an API hostname), set `KOKING_ALLOWED_ORIGINS` to the exact HTTPS origin serving the web app, and build the web client with `KOKING_API_URL=https://api.example.com`. Do not expose the plain HTTP API directly to the public internet.

Tagged releases include a Linux server binary named `koking-server-linux-amd64`. For a systemd install, create a `koking` user and `/var/lib/koking`, install the binary at `/usr/local/bin/koking-server`, adapt [the service example](deploy/koking-server.service.example), and enable it with systemd.

Passwords are Argon2id-hashed; bearer session tokens are random, expire after 30 days, and are stored hashed on the server. User registration is open to anyone who can reach the server. Chat is currently a functional basic direct-message service; message transport and database storage are not end-to-end encrypted, and the server operator can read stored messages. Use a trusted server and HTTPS.

## Build
GitHub Actions builds a Debian 13 `.deb` package named Koking with an application launcher, a sideloadable Android APK, a static web bundle, and macOS and Windows desktop binaries on pull requests targeting `main`, when run manually, and when a `v*` tag is pushed. Tagged builds are attached to a GitHub Release. The Android artifact is debug-signed for testing and is not a Play Store release.

## Run the desktop client

```bash
cargo run -p signal-alt --features desktop
```
