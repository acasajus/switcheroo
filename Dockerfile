# Stage 1: Build Frontend
FROM node:20-alpine AS frontend-builder
WORKDIR /app
COPY frontend/package*.json ./
RUN npm install
COPY frontend/ .
RUN npm run build

# Stage 2: Build Backend
FROM rust:1-slim-bookworm AS backend-builder
WORKDIR /app
COPY . .
RUN cargo build --release

# Stage 3: Runtime
FROM debian:bookworm-slim
WORKDIR /app
RUN apt-get update && apt-get install -y libssl3 ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=backend-builder /app/target/release/switcheroo /app/switcheroo
COPY --from=frontend-builder /app/dist /app/frontend/dist

RUN mkdir -p /app/games

ENV SWITCHEROO_GAMES_DIR=/app/games
ENV SWITCHEROO_SERVER_PORT=3000
ENV SWITCHEROO_LOG_LEVEL=info
ENV RUST_LOG=info

EXPOSE 3000

CMD ["./switcheroo"]
