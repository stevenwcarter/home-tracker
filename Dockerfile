# Stage 1: warm the musl dependency cache with a stub crate so source edits
# don't rebuild every dependency.
FROM docker.io/blackdex/rust-musl:x86_64-musl AS dependencybuilder
WORKDIR /home/rust/src
COPY Cargo.toml Cargo.lock ./
RUN mkdir -p src site/build \
  && echo "fn main() {}" > src/main.rs \
  && echo "" > src/lib.rs \
  && echo "<!doctype html>" > site/build/index.html
RUN cargo build --release
RUN rm -rf src

# Stage 2: the real build. site/build is produced by CI's build-site job (or
# `just build-site` locally) before this runs.
FROM dependencybuilder AS builder
COPY src ./src/
COPY migrations ./migrations/
COPY site/build ./site/build/
RUN find src -name '*.rs' -exec touch {} + && cargo build --release

# Stage 3: static binary only.
FROM scratch
WORKDIR /
COPY --from=builder /home/rust/src/target/x86_64-unknown-linux-musl/release/home-tracker /home-tracker
COPY env.prod /.env

VOLUME ["/data"]
EXPOSE 7008

HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
  CMD ["/home-tracker", "healthcheck"]

CMD ["/home-tracker"]
