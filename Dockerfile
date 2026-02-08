# Stage 1: Build Frontend
FROM node:20-alpine AS frontend-builder
WORKDIR /app/frontend
COPY frontend/package*.json ./
RUN npm ci
COPY frontend/ .
RUN npm run build

# Stage 2: Build Backend
FROM rust:1-slim-bookworm AS backend-builder
WORKDIR /app
# Create empty shell project to cache dependencies
RUN cargo new --bin switcheroo
WORKDIR /app/switcheroo
COPY Cargo.toml Cargo.lock ./
# Build dependencies only (release mode)
RUN cargo build --release
# Remove the dummy source
RUN rm src/*.rs

# Copy actual source code
COPY src ./src
# Touch main.rs to ensure rebuild
RUN touch src/main.rs
# Build the actual application
RUN cargo build --release

# Stage 3: Runtime
FROM debian:bookworm-slim
WORKDIR /app
RUN apt-get update && apt-get install -y libssl3 ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=backend-builder /app/switcheroo/target/release/switcheroo /app/switcheroo
COPY --from=frontend-builder /app/frontend/dist /app/frontend/dist

# Create necessary directories
RUN mkdir -p /app/games /app/data

# Set environment variables
ENV SWITCHEROO_GAMES_DIR=/app/games
ENV SWITCHEROO_DATA_DIR=/app/data
ENV SWITCHEROO_SERVER_PORT=3000
ENV SWITCHEROO_LOG_LEVEL=info

EXPOSE 3000

CMD ["./switcheroo"]
