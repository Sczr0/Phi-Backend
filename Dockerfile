ARG RUST_VERSION=1.86.0
ARG APP_NAME=phi-backend-rust

FROM rust:${RUST_VERSION}-slim-bullseye as builder

ARG APP_NAME
WORKDIR /app

# 安装构建依赖 (例如，如果你的项目需要 openssl)
RUN apt-get update && apt-get install -y --no-install-recommends libssl-dev ca-certificates pkg-config && rm -rf /var/lib/apt/lists/*

# 仅复制 Cargo 文件以缓存依赖项
COPY Cargo.toml Cargo.lock* ./
# 构建一个空的 lib 项目来下载和编译依赖项 (利用层缓存)
RUN mkdir src && echo "fn main() {}" > src/main.rs && cargo build --release --locked
# 删除临时 main.rs
RUN rm -rf src

# 复制项目源代码
COPY src ./src
# 复制构建时可能需要的资源 (如果 build.rs 使用)
# COPY resources ./resources
# COPY info ./info
# COPY difficulty.csv ./
# COPY info.csv ./
# COPY nicklist.yaml ./

# 构建实际项目 (清理之前的空项目构建输出)
RUN rm -f target/release/deps/lib${APP_NAME}*
# 复制运行时所需的资源文件 (确保它们在最终镜像中)
COPY resources ./resources
# 复制整个 info 目录到 /app/info (builder stage)
COPY info ./info
# (不再需要单独复制 csv/yaml 到根目录)
# COPY info/difficulty.csv .
# COPY info/info.csv .
# COPY info/nicklist.yaml .
RUN cargo build --release --locked

# ---- Runtime Stage ----
FROM debian:bullseye-slim as runtime

ARG APP_NAME
WORKDIR /app

# 创建非 root 用户和组
RUN groupadd --gid 1001 appgroup && \
    useradd --uid 1001 --gid 1001 --shell /bin/false --create-home appuser

# (可选) 安装运行时依赖，例如 ca-certificates (用于 HTTPS)
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*

# 从构建阶段复制必要的运行时文件
COPY --from=builder /app/target/release/${APP_NAME} /usr/local/bin/
COPY --from=builder /app/resources ./resources
# 从 builder 复制整个 /app/info 目录到 runtime 的 /app/info
COPY --from=builder /app/info ./info
# (不再需要从 builder 复制单个文件)
# COPY --from=builder /app/difficulty.csv .
# COPY --from=builder /app/info.csv .
# COPY --from=builder /app/nicklist.yaml .
# 注意：如果需要 SQLite 数据库，需要在这里复制或通过卷挂载
# COPY --from=builder /app/phigros_bindings.db ./phigros_bindings.db

# 创建数据目录
RUN mkdir -p /app/data && chown -R appuser:appgroup /app/data

# 设置文件所有权
RUN chown -R appuser:appgroup /app /usr/local/bin/${APP_NAME}

# 切换到非 root 用户
USER appuser

# 暴露端口
EXPOSE 8080

# 设置环境变量 (从 .env 文件推断)
# ENV RUST_LOG=info
# 确保路径在容器内有效
# ENV INFO_DATA_PATH=/app/info
# ENV DIFFICULTY_FILE=/app/difficulty.csv
# ENV INFO_FILE=/app/info.csv
# ENV NICKLIST_FILE=/app/nicklist.yaml
# 数据库路径通常在运行时通过环境变量或配置文件提供，并结合卷挂载
# ENV DATABASE_URL=sqlite:/app/data/phigros_bindings.db

# 容器启动时运行的命令
CMD ["/usr/local/bin/phi-backend-rust"] 