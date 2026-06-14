FROM debian:bookworm-slim AS installer

ARG FILMDROP_WEB_VERSION=latest

RUN apt-get update \
    && apt-get install -y --no-install-recommends curl ca-certificates \
    && rm -rf /var/lib/apt/lists/*

RUN curl -L --proto '=https' --tlsv1.2 -sSf \
    https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash

RUN if [ "${FILMDROP_WEB_VERSION}" = "latest" ]; then \
        /root/.cargo/bin/cargo-binstall filmdrop-web --no-confirm --install-path /usr/local/bin; \
    else \
        /root/.cargo/bin/cargo-binstall "filmdrop-web@${FILMDROP_WEB_VERSION}" --no-confirm --install-path /usr/local/bin; \
    fi

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd -r filmdrop && useradd -r -g filmdrop filmdrop

COPY --from=installer /usr/local/bin/filmdrop-web /usr/local/bin/filmdrop-web

USER filmdrop

EXPOSE 3000

CMD ["/usr/local/bin/filmdrop-web"]
