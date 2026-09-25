# Create the empty site bundle the Rust build embeds when the real site hasn't been built.
site-placeholder:
    mkdir -p site/build
    [ -f site/build/index.html ] || echo '<!doctype html><title>home-tracker</title>' > site/build/index.html

# Build the React site into site/build (what the release binary embeds).
build-site:
    cd site && yarn install --frozen-lockfile && yarn build

test: site-placeholder
    watchexec -e rs,toml,sql cargo test

cover: site-placeholder
    cargo llvm-cov --lcov --output-path lcov.info

# Import a Homebox backup zip or exploded directory into the dev database.
import path:
    cargo run -- import {{path}}

docker-build:
    docker build -t home-tracker:dev .
