# syntax=docker/dockerfile:1.13

FROM docker.gosh.sh/rust:stable AS builder
WORKDIR /build
COPY --link . ./
RUN \
  --mount=type=cache,target=/usr/local/cargo/registry \
  --mount=type=cache,target=/usr/local/cargo/git \
  --mount=type=cache,target=/target \
  <<EOF
  apt install libnuma-dev
  cargo build --release --target-dir=/target
  cp /target/release/acki-nacki-igniter /usr/local/bin/
EOF

FROM docker.gosh.sh/debian:stable

COPY --from=builder /usr/local/bin/acki-nacki-igniter /usr/local/bin/

EXPOSE 10000/tcp
EXPOSE 10000/udp

LABEL com.centurylinklabs.watchtower.enbled="true"
LABEL com.centurylinklabs.watchtower.scope="acki-nacki"
