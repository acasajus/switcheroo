#!/bin/bash
set -e

echo "Building Switcheroo..."

echo "[1/2] Building Frontend..."
cd frontend
npm install
npm run build
cd ..

echo "[2/2] Building Backend..."
cargo build --release

echo "Build complete!"
echo "Run with: ./target/release/switcheroo"
