FROM alpine:3.19

RUN apk add --no-cache \
    sqlite \
    sqlite-libs \
    ripgrep \
    jq \
    vim \
    coreutils \
    bash

WORKDIR /data

# Idle loop so the container stays up and users can exec into it.
CMD ["sh", "-c", "while true; do sleep 3600; done"]
