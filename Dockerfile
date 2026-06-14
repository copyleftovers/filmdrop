FROM debian:bookworm-slim AS installer

ARG FILMDROP_WEB_VERSION

RUN apt-get update \
    && apt-get install -y --no-install-recommends curl ca-certificates \
    && rm -rf /var/lib/apt/lists/*

RUN curl -L --proto '=https' --tlsv1.2 -sSf \
    https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash

RUN /root/.cargo/bin/cargo-binstall "filmdrop-web@${FILMDROP_WEB_VERSION}" --no-confirm --install-path /usr/local/bin

FROM gcr.io/distroless/cc-debian12:nonroot

COPY --from=installer /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/
COPY --from=installer /usr/local/bin/filmdrop-web /usr/local/bin/filmdrop-web

EXPOSE 3000

CMD ["/usr/local/bin/filmdrop-web"]
