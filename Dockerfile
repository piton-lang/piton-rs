FROM ubuntu:26.04

ARG USER_ID=1000
ARG GROUP_ID=1000

ENV DEBIAN_FRONTEND=noninteractive

# General-purpose development environment.
RUN apt-get update && apt-get install -y --no-install-recommends \
    bash \
    build-essential \
    ca-certificates \
    curl \
    fd-find \
    git \
    jq \
    less \
    openssh-client \
    python3 \
    python3-pip \
    python3-venv \
    ripgrep \
    sudo \
    unzip \
    vim \
    wget \
    xz-utils \
    && rm -rf /var/lib/apt/lists/*

# Match the host UID/GID so files Claude creates in the mounted
# project belong to you rather than some mystery container user.
#
# Ubuntu >= 24.04 ships a default "ubuntu" user at UID/GID 1000,
# so evict whatever already occupies the host's UID/GID first.
RUN set -eux; \
    if existing_user="$(getent passwd "${USER_ID}" | cut -d: -f1)" && [ -n "$existing_user" ]; then \
        userdel --remove "$existing_user" 2>/dev/null || userdel "$existing_user"; \
    fi; \
    if existing_group="$(getent group "${GROUP_ID}" | cut -d: -f1)" && [ -n "$existing_group" ]; then \
        groupdel "$existing_group" 2>/dev/null || true; \
    fi; \
    groupadd --gid "${GROUP_ID}" claude; \
    useradd \
        --uid "${USER_ID}" \
        --gid "${GROUP_ID}" \
        --create-home \
        --shell /bin/bash \
        claude

# ------------------------------------------------------------
# Install Claude Code OUTSIDE /home/claude.
#
# /home/claude will later be mounted as a persistent volume,
# so installing Claude there would cause the installation to
# disappear behind the volume.
# ------------------------------------------------------------

RUN mkdir -p /opt/claude \
    && chown claude:claude /opt/claude

USER claude

# NOTE: the env assignment must apply to the shell that RUNS the
# installer, not just to curl -- otherwise the installer reads the
# build user's real HOME and lands in /home/claude, behind the volume.
RUN curl -fsSL https://claude.ai/install.sh | HOME=/opt/claude bash \
    && test -x /opt/claude/.local/bin/claude

USER root

RUN ln -sf /opt/claude/.local/bin/claude /usr/local/bin/claude

# Give Claude ownership of its persistent home.
RUN chown -R claude:claude /home/claude /opt/claude

USER claude

ENV HOME=/home/claude
ENV PATH="/opt/claude/.local/bin:${PATH}"

# The container filesystem is read_only at runtime, so Claude cannot
# rewrite its own install. Upgrade by rebuilding the image instead.
ENV DISABLE_AUTOUPDATER=1

WORKDIR /workspace

CMD ["claude"]
