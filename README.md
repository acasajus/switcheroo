# Switcheroo

A Rust-based server to serve Nintendo Switch games to Tinfoil and DBI, featuring a real-time web UI.

## Features

- **File Serving:** Serves `.nsp`, `.nsz`, `.xci`, and `.xcz` files.
- **Tinfoil Support:** JSON index compatible with Tinfoil at `/tinfoil`.
- **DBI Support:** HTML index compatible with DBI at `/dbi`.
- **Real-time UI:** Tailwind CSS web interface showing indexed games and active download speeds/progress.
- **Download Tracking:** Server-side tracking of download speeds and progress updates via SSE.
- **Configurable:** Customize port, directories, and logging via environment variables or file.
- **Docker Ready:** Production-ready Docker image with embedded UI.

## Quick Start (Build & Run)

You can easily build the project using one of the following methods:

### Option 1: Using `mise` (Recommended)
If you have [mise](https://mise.jdx.dev/) installed:
```bash
mise run build   # Build frontend and backend
mise run start   # Run the built binary
# or
mise run dev     # Run in development mode
```

### Option 2: Using `make`
```bash
make build       # Build everything
make run         # Build and run
```

### Option 3: Using Shell Script
```bash
./build.sh
./target/release/switcheroo
```

### Option 4: Manual Steps
1.  **Frontend:**
    ```bash
    cd frontend
    npm install
    npm run build
    cd ..
    ```
2.  **Backend:**
    ```bash
    cargo build --release
    ./target/release/switcheroo
    ```

## Configuration

You can configure Switcheroo using `config.toml` or Environment Variables (prefix `SWITCHEROO_`).

| Variable | Default | Description |
|----------|---------|-------------|
| `SWITCHEROO_SERVER_PORT` | `3000` | Port to listen on. |
| `SWITCHEROO_GAMES_DIR` | `./games` | Directory containing game files. |
| `SWITCHEROO_LOG_LEVEL` | `info` | Log level (error, warn, info, debug, trace). |
| `RUST_LOG` | `info` | Advanced logging filter configuration. |

## Docker

Build and run with Docker:

```bash
docker build -t switcheroo .
docker run -p 3000:3000 -v $(pwd)/games:/app/games switcheroo
```

## Usage

- **Tinfoil:** Add a "File" source pointing to `http://<YOUR_IP>:3000/tinfoil`.
- **DBI:** Use "Install from HTTP" pointing to `http://<YOUR_IP>:3000/dbi`.
- **Web Browser:** Visit `http://<YOUR_IP>:3000` to view your collection and monitor downloads.
