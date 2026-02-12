# Runtime image — pre-compiled binary + git/curl/ca-certs
# Built from binaries produced by the fork-release workflow.
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    git && \
    rm -rf /var/lib/apt/lists/*

COPY dist/sandbox-agent /usr/local/bin/sandbox-agent
RUN chmod +x /usr/local/bin/sandbox-agent

RUN useradd -m -s /bin/bash sandbox
USER sandbox
WORKDIR /home/sandbox

EXPOSE 2468

ENTRYPOINT ["sandbox-agent"]
CMD ["server", "--host", "0.0.0.0", "--port", "2468"]
