FROM node:22-bookworm-slim
RUN apt-get update -qq && apt-get install -y -qq --no-install-recommends ca-certificates > /dev/null 2>&1 && \
    rm -rf /var/lib/apt/lists/* && \
    npm install -g --silent @sandbox-agent/cli@latest && \
    sandbox-agent install-agent claude
