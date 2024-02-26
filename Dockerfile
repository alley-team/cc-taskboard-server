FROM rust:latest as builder
WORKDIR /usr/src/cc-taskboard-backend
COPY . .
RUN cargo install --path .

FROM debian:bookworm-slim as runner
COPY --from=builder /usr/local/cargo/bin/cc-taskboard-server /usr/local/bin/cc-taskboard-server
COPY --from=builder /usr/src/cc-taskboard-backend/.env /etc/taskboard.conf
