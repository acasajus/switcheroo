# Switcheroo

Switcheroo is a high-performance Rust-based server for Nintendo Switch game files (`.nsp`, `.nsz`, `.xci`, `.xcz`). It is designed to serve as a local network repository for homebrew installers like **Tinfoil** and **DBI**, providing a sleek web-based dashboard to manage and monitor your collection.

## Features

- 🚀 **High Performance**: Built with Rust and Axum for maximum efficiency.
- 📂 **Automatic Scanning**: Recursively scans your games directory and detects new files automatically using a file watcher.
- 🖼️ **Metadata Support**: Automatically downloads game covers and metadata (Title ID, size, etc.).
- 🌐 **Web Dashboard**: Modern, real-time UI built with Vite, TypeScript, and Tailwind CSS.
- 📡 **Tinfoil & DBI Support**: Dedicated endpoints for seamless integration with Switch homebrew.
- 📁 **WebDAV Support**: Built-in WebDAV server for remote file management.
- 🐳 **Docker Ready**: Easy deployment using multi-stage Docker builds.
- ⚡ **Real-time Updates**: Uses Server-Sent Events (SSE) to push download progress and scan updates to the UI.

## Getting Started

### Using Docker (Recommended)

The easiest way to run Switcheroo is using Docker:

```bash
docker run -d \
  --name switcheroo \
  -p 3000:3000 \
  -v /path/to/your/games:/games \
  -v /path/to/your/data:/data \
  -e SWITCHEROO_GAMES_DIR=/games \
  -e SWITCHEROO_DATA_DIR=/data \
  ghcr.io/your-username/switcheroo:latest
```

### Building from Source

#### Prerequisites
- Rust (latest stable)
- Node.js & npm (for building the frontend)

#### Using Make (Convenient)
You can use the provided `Makefile` to build and run the project:
```bash
make run      # Build both frontend and backend and run
make build    # Build both without running
make clean    # Remove build artifacts
```

#### Manual Steps
1. **Build the Frontend**:
   ```bash
   cd frontend
   npm install
   npm run build
   cd ..
   ```

2. **Run the Backend**:
   ```bash
   cargo run --release
   ```

By default, the server will be available at `http://localhost:3000`.

## Configuration

Switcheroo can be configured using environment variables (prefixed with `SWITCHEROO_`) or a `config.toml` file in the root directory.

| Environment Variable | Description | Default |
|----------------------|-------------|---------|
| `SWITCHEROO_SERVER_PORT` | Port the server listens on | `3000` |
| `SWITCHEROO_GAMES_DIR` | Path to the directory containing game files | `./games` |
| `SWITCHEROO_DATA_DIR` | Path to store metadata and images | `./data` |
| `SWITCHEROO_LOG_LEVEL` | Logging verbosity (`debug`, `info`, `warn`, `error`) | `info` |
| `SWITCHEROO_WEBDAV_ENABLED` | Enable/Disable WebDAV server | `true` |
| `SWITCHEROO_WEBDAV_USERNAME` | WebDAV username (Basic Auth) | `None` |
| `SWITCHEROO_WEBDAV_PASSWORD` | WebDAV password (Basic Auth) | `None` |

### Example `config.toml`
```toml
server_port = 3000
games_dir = "/media/games"
data_dir = "./data"
log_level = "info"
webdav_enabled = true
# webdav_username = "admin"
# webdav_password = "password"
```

## Connecting from your Switch

### Tinfoil
In Tinfoil, add a new "File Index" source:
- **Protocol**: `http`
- **Host**: Your computer's local IP address
- **Port**: `3000`
- **Path**: `/tinfoil/`

### DBI
In DBI, select "Install title from URL" or use the "Homebrew Menu" to browse:
- **URL**: `http://<your-ip>:3000/dbi/`

### WebDAV
You can mount the games directory via WebDAV at:
- `http://<your-ip>:3000/dav/`

## Development

- **Backend**: The Rust source code is located in `src/`.
- **Frontend**: The UI source code is in `frontend/src/`.
- **Static Assets**: Frontend assets are served from `frontend/dist/` after building.

To run the frontend in development mode with hot-reloading:
```bash
cd frontend
npm run dev
```
Note: You'll need to configure the frontend to point to your backend API if running separately.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

---

*Keep this README updated by ensuring any new configuration options added to `src/config.rs` are also documented here.*