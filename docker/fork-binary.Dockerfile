# Binary-only image — minimal, for COPY --from in other Dockerfiles
FROM debian:bookworm-slim

COPY dist/sandbox-agent /usr/local/bin/sandbox-agent
RUN chmod +x /usr/local/bin/sandbox-agent

ENTRYPOINT ["sandbox-agent"]
