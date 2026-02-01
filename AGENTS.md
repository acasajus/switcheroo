# Context for Future Agents

## Project Overview
**Switcheroo** is a Rust-based server for Nintendo Switch game files, designed to serve Tinfoil and DBI over the local network. It includes a real-time web UI built with Vite, TypeScript, and Tailwind CSS.

## Tech Stack
- **Backend:** Rust (Axum, Tokio, Tower-HTTP)
- **Frontend:** TypeScript, Vite, Tailwind CSS (Single Page Application)
- **Configuration:** `config` crate (TOML + Environment Variables)
- **Logging:** `tracing` + `tracing-subscriber` (JSON/EnvFilter supported)
- **Containerization:** Docker (Multi-stage build)

## Key Components

### Backend (`src/`)
- **`main.rs`:** Entry point. Sets up the Axum router, loads config, and initializes tracing. It serves the API at `/api` and the compiled frontend at `/`.
- **`config.rs`:** Handles configuration loading. Priorities: Env Vars > `config.toml` > Defaults.
- **`scanner.rs`:** Recursively scans the `games_dir` for `.nsp`, `.nsz`, `.xci`, `.xcz` files.
- **`downloads.rs`:** Manages active download states (speed, progress).
- **Events:** Server-Sent Events (SSE) at `/events` broadcast download updates.

### Frontend (`frontend/`)
- **`src/main.ts`:** Main logic. Fetches games list and listens to SSE for download updates.
- **`dist/`:** Production build output (served by Rust).

## Configuration
Configuration is managed via `Settings` struct in `src/config.rs`.
- `SWITCHEROO_SERVER_PORT` (Default: 3000)
- `SWITCHEROO_GAMES_DIR` (Default: `./games`)
- `SWITCHEROO_LOG_LEVEL` (Default: `info`)

## Docker
The `Dockerfile` performs a multi-stage build:
1. Builds frontend assets.
2. Builds Rust binary.
3. Packages both into a lightweight Debian Slim image.

## CI/CD
GitHub Actions (`.github/workflows/ci.yml`) handles:
- Formatting (`cargo fmt`)
- Linting (`cargo clippy`)
- Testing (`cargo test`)
- Docker Build & Push (on main push)

## Development Notes
- When adding new routes, ensure they don't conflict with `ServeDir` fallback for the frontend.
- Tinfoil/DBI routes are specific endpoints (`/tinfoil`, `/dbi`).
- Real-time updates rely on `tokio::sync::broadcast` and a background task loop.
