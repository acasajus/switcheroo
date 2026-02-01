.PHONY: all build frontend backend clean run help

all: build

help: ## Show this help
	help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-15s\033[0m %s\n", $$1, $$2}'

build: frontend backend ## Build both frontend and backend

frontend: ## Install and build the frontend
	cd frontend && npm install && npm run build

backend: ## Build the backend in release mode
	cargo build --release

run: build ## Build and run the application
	./target/release/switcheroo

clean: ## Clean build artifacts
	cargo clean
	rm -rf frontend/dist
	rm -rf frontend/node_modules

