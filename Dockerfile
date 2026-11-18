# syntax=docker/dockerfile:1.7

# ---- frontend -------------------------------------------------------------
FROM node:22-bookworm-slim AS web
WORKDIR /src/web
RUN corepack enable
COPY web/package.json web/pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile
COPY web/ ./
RUN pnpm exec vite build

# ---- backend --------------------------------------------------------------
FROM rust:1-bookworm AS chef
RUN cargo install cargo-chef --locked
WORKDIR /src

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS build
COPY --from=planner /src/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json -p pp-server
COPY . .
COPY --from=web /src/web/dist web/dist
RUN cargo build --release -p pp-server && cp target/release/pp-server /pp-server

# ---- runtime --------------------------------------------------------------
FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=build /pp-server /usr/local/bin/pp-server
ENV PP_BIND=0.0.0.0:8080 \
    PP_DATA_DIR=/data
VOLUME ["/data"]
EXPOSE 8080
USER nonroot
HEALTHCHECK --interval=30s --timeout=3s CMD ["/usr/local/bin/pp-server", "healthcheck"]
ENTRYPOINT ["/usr/local/bin/pp-server"]
