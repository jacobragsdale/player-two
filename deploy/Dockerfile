# The hub's image: one static binary, and the installer the release pipeline put in
# deploy/release. Build it from the repository's root:
#   docker build -f deploy/Dockerfile .

FROM rust:1-alpine AS build
RUN apk add --no-cache musl-dev
WORKDIR /src
COPY . .
RUN cargo build --release --locked -p player-two-hub \
 && mkdir -p /out/app/release \
 && cp target/release/player-two-hub /out/app/ \
 && cp deploy/release/* /out/app/release/

# Nothing but the binary: no shell, no package manager, nothing to patch.
FROM scratch
COPY --from=build --chown=10001:10001 /out/ /
USER 10001
ENV PLAYER_TWO_RELEASE=/app/release
EXPOSE 8787
ENTRYPOINT ["/app/player-two-hub"]
CMD ["serve"]
