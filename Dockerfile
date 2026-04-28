# ============================================
# Aggressively Optimized Multi-Stage Build
# ============================================

# Stage 1: Prepare cargo-chef
FROM rust:1.91-slim-bookworm AS chef
RUN cargo install cargo-chef --locked
WORKDIR /app

# Stage 2: Generate recipe for dependency caching
FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
COPY filmdrop-core ./filmdrop-core
COPY filmdrop-cli ./filmdrop-cli
COPY filmdrop-web ./filmdrop-web
RUN cargo chef prepare --recipe-path recipe.json

# Stage 3: Build dependencies (cached layer)
FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json

# Build dependencies with aggressive optimizations
RUN cargo chef cook --release \
    --recipe-path recipe.json \
    --bin filmdrop-web

# Stage 4: Build application
COPY Cargo.toml Cargo.lock ./
COPY filmdrop-core ./filmdrop-core
COPY filmdrop-cli ./filmdrop-cli
COPY filmdrop-web ./filmdrop-web

# Aggressive optimization flags
ENV CARGO_PROFILE_RELEASE_LTO=true \
    CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1 \
    CARGO_PROFILE_RELEASE_OPT_LEVEL=3 \
    CARGO_PROFILE_RELEASE_STRIP=true

RUN cargo build --release --bin filmdrop-web

# Stage 5: Runtime - Google Distroless (minimal, secure)
FROM gcr.io/distroless/cc-debian12:nonroot

# Copy CA certificates for HTTPS/S3
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/

# Copy optimized binary
COPY --from=builder /app/target/release/filmdrop-web /usr/local/bin/filmdrop-web

# Expose port
EXPOSE 3000

# Run as non-root user (distroless default)
CMD ["/usr/local/bin/filmdrop-web"]
