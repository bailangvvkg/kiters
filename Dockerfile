# A kiters image is the one static binary, built against musl on Alpine,
# landing in a base that has nothing else to attack.
FROM rust:1-alpine AS build
RUN apk add --no-cache musl-dev
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY apps ./apps
COPY plugins ./plugins
COPY tests ./tests
# The one binary that ships; the plugins build for wasm, not for here.
RUN cargo build --release -p kiters-cli

FROM alpine:3
COPY --from=build /src/target/release/kiters /usr/local/bin/kiters
ENTRYPOINT ["/usr/local/bin/kiters"]
CMD ["--help"]
