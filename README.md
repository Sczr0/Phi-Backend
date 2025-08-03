# Phi-Backend

基于Rust的Phigros后端服务，提供绑定、存档解析、RKS计算以及单曲成绩查询等功能。

## 功能

- 绑定Phigros的sessionToken
- 获取并解析Phigros存档
- 计算玩家RKS及其构成成绩 (B30, Best N)
- 查询单曲成绩
- 统一查询歌曲信息（支持ID、名称、别名）
- 生成B30/BN成绩图片
- 生成单曲成绩图片
- 生成RKS排行榜图片

## 使用前提

该服务需要使用[phi-plugin](https://github.com/Catrong/phi-plugin)的数据文件，包括：

- `info.csv`（歌曲信息）
- `difficulty.csv`（歌曲难度定数）
- `nicklist.yaml`（歌曲别名）

这些文件默认位于项目根目录下的 `info` 文件夹内。路径可以在`.env`中配置。

## 安装和运行

1.  **克隆仓库**
    ```bash
    git clone <repository-url>
    cd phi-backend-rust
    ```

2.  **创建`.env`文件** (可选, 用于自定义配置)
    参考`.env.example`文件创建`.env`文件，可以配置数据库URL、服务器地址、端口和数据文件路径等。
    ```bash
    # 复制示例配置文件
    cp .env.example .env
    # 然后按需修改 .env 中的配置
    ```
    
    **配置说明**:
    ```dotenv
    # 数据库URL，默认使用项目根目录的SQLite文件
    DATABASE_URL=sqlite:phigros_bindings.db
    
    # 服务器配置 (可选，有默认值)
    # HOST=127.0.0.1
    # PORT=8080
    
    # 数据文件路径 (可选，默认使用项目根目录下的info文件夹)
    # INFO_DATA_PATH=info
    ```
    
    **注意**: `.env` 文件用于本地开发环境，不应提交到Git仓库。

3.  **编译项目**
    ```bash
    cargo build --release
    ```

4.  **运行服务**
    ```bash
    cargo run --release
    ```

    服务将在配置的地址和端口启动（默认`127.0.0.1:8080`）。

## API接口

所有接口均使用JSON格式进行数据交换。

**通用请求体:**

-   **`IdentifierRequest`** (用于需要用户身份的接口)
    -   请求体中必须包含 `token` 或 (`platform` 和 `platform_id`) 中的至少一组。
    -   如果都提供，优先使用 `token`。
    -   `platform` 字段大小写不敏感。
    ```json
    // 方式一：使用 SessionToken
    {
        "token": "用户的Phigros SessionToken"
    }
    // 方式二：使用平台和平台ID (如果已绑定)
    {
        "platform": "qq", // 平台名称 (大小写不敏感)
        "platform_id": "用户的QQ号"
    }
    ```

### 扫码登录

-   **`GET /auth/qrcode`** 或 **`POST /auth/qrcode`**
    -   描述: 生成用于TapTap账号登录的二维码图片（Base64编码）。
    -   成功响应 (`200 OK`):
        ```json
        {
            "qrId": "唯一的二维码ID，用于查询状态",
            "qrCodeImage": "data:image/png;base64,xxxxxxxxxx..." // Base64编码的PNG图片数据
        }
        ```
    -   失败响应: `500 Internal Server Error` (二维码生成失败)。

-   **`GET /auth/qrcode/{qrId}/status`**
    -   描述: 查询指定二维码的登录状态。
    -   路径参数: `qrId` (通过 `/auth/qrcode` 获取的二维码ID)
    -   成功响应 (`200 OK`):
        -   **`status: "pending"`**: 等待用户扫描二维码。
        -   **`status: "scanned"`**: 用户已扫描二维码，等待授权。
        -   **`status: "success"`**: 用户已授权登录成功。
            ```json
            {
                "status": "success",
                "sessionToken": "用户的TapTap SessionToken"
            }
            ```
        -   **`status: "expired"`**: 二维码已过期（通常5分钟）。
        -   **`status: "error"`**: 发生其他错误。
    -   失败响应:
        -   `404 Not Found`: `qrId` 无效或已过期。
        -   `500 Internal Server Error`: 其他内部错误。

### 用户绑定

-   **`POST /bind`**
    -   描述: 绑定平台ID和Phigros SessionToken。
    -   请求体:
        ```json
        {
            "platform": "qq", // 平台名称 (大小写不敏感)
            "platform_id": "用户的QQ号",
            "token": "用户的Phigros SessionToken"
        }
        ```
    -   成功响应 (`200 OK`):
      ```json
      {
          "code": 200,
          "status": "success",
          "message": "绑定成功", // 或 "已更新...的Token", 或 "已绑定到现有内部用户"
          "data": {
              "internal_id": "生成的或已有的内部用户ID"
          }
      }
      ```
    -   失败响应: `400 Bad Request` (参数错误), `500 Internal Server Error`。

-   **`POST /token/list`**
    -   描述: 获取用户关联的所有平台ID和Token列表。
    -   请求体: `IdentifierRequest`
    -   成功响应 (`200 OK`):
      ```json
      {
          "code": 200,
          "status": "success",
          "message": "获取Token列表成功",
          "data": {
              "internal_id": "用户内部ID",
              "bindings": [
                  {
                      "platform": "qq",
                      "platform_id": "123456",
                      "session_token": "token_for_qq",
                      "bind_time": "2025-01-01T12:00:00Z"
                  }
              ]
          }
      }
      ```
    -   失败响应: `400 Bad Request`, `404 Not Found`, `500 Internal Server Error`。

-   **`POST /unbind`**
    -   描述: 解除指定平台账号的绑定。支持Token验证或简介验证。
    -   请求体: `IdentifierRequest` (必须包含 `platform`, `platform_id`; 可选 `token` 或 `verification_code`)
    -   成功响应:
        -   Token验证成功: `200 OK`
          ```json
          {
              "code": 200,
              "status": "success",
              "message": "解绑成功 (平台ID+Token验证)"
          }
          ```
        -   简介验证 - 获取验证码: `200 OK`
          ```json
          {
              "code": 200,
              "status": "verification_initiated",
              "message": "请在 xxx 秒内将您的 Phigros 简介修改为此验证码...",
              "data": {
                  "verification_code": "ABCDEF12",
                  "expires_in_seconds": "xxx"
              }
          }
          ```
        -   简介验证 - 提交验证码成功: `200 OK`
           ```json
           {
               "code": 200,
               "status": "success",
               "message": "解绑成功 (简介验证)"
           }
           ```
    -   失败响应: `400 Bad Request`, `401 Unauthorized`, `404 Not Found`, `500 Internal Server Error`。

### 存档与RKS

-   **`POST /get/cloud/saves`**
    -   描述: 获取并解析用户的Phigros云存档（不含难度定数和RKS）。
    -   请求体: `IdentifierRequest`
    -   成功响应 (`200 OK`): 返回基础 `GameSave` 结构。
    -   失败响应: `401 Unauthorized`, `404 Not Found`, `500 Internal Server Error`。

-   **`POST /get/cloud/saves/with_difficulty`**
    -   描述: 获取并解析用户的Phigros云存档，包含难度定数和计算出的RKS值。
    -   请求体: `IdentifierRequest`
    -   成功响应 (`200 OK`): 返回包含 `difficulty` 和 `rks` 的 `GameSave` 结构。
    -   失败响应: `401 Unauthorized`, `404 Not Found`, `500 Internal Server Error`。

-   **`POST /rks`**
    -   描述: 计算并返回用户所有歌曲的RKS分数，按分数由高到低排序。
    -   请求体: `IdentifierRequest`
    -   成功响应 (`200 OK`):
      ```json
      {
          "code": 200,
          "status": "OK",
          "data": {
              "records": [
                  {
                      "song_id": "...", "song_name": "...", "difficulty": "IN",
                      "difficulty_value": ..., "acc": ..., "score": ..., "rks": ...
                  }
              ]
          }
      }
      ```
    -   失败响应: `401 Unauthorized`, `404 Not Found`, `500 Internal Server Error`。

-   **`POST /b30`**
    -   描述: 计算并返回用户的B30成绩。
    -   请求体: `IdentifierRequest`
    -   成功响应 (`200 OK`): 返回 `B30Result` 结构。
    -   失败响应: `401 Unauthorized`, `404 Not Found`, `500 Internal Server Error`。

-   **`POST /bn/{n}`**
    -   描述: 计算并返回用户的 Best N 成绩。
    -   路径参数: `n` (整数, 必须大于0)
    -   请求体: `IdentifierRequest`
    -   成功响应 (`200 OK`): 返回 `BnResult` 结构。
    -   失败响应: `400 Bad Request`, `401 Unauthorized`, `404 Not Found`, `500 Internal Server Error`。

### 歌曲查询

-   **`GET /song/search`** (推荐)
    -   描述: 统一搜索歌曲信息。
    -   查询参数: `q` (必需) - 搜索关键词
    -   成功响应 (`200 OK`): 返回 `SongInfo`。
    -   失败响应: `400 Bad Request`, `404 Not Found`, `409 Conflict`。

-   **`POST /song/search/record`** (推荐)
    -   描述: 查询指定歌曲的成绩记录。
    -   查询参数:
        -   `q`: (必需) 歌曲ID、名称或别名。
        -   `difficulty`: (可选) 难度级别 (EZ, HD, IN, AT)。
    -   请求体: `IdentifierRequest`
    -   成功响应 (`200 OK`): 返回该歌曲的 `SongRecord`。
    -   失败响应: `400 Bad Request`, `401 Unauthorized`, `404 Not Found`, `409 Conflict`。

-   **`GET /song/search/predictions`**
    -   描述: 查询歌曲的预测常数信息。
    -   查询参数:
        -   `q` (必需) - 搜索关键词
        -   `difficulty` (可选) - 指定难度，如不提供则返回所有难度。
    -   成功响应 (`200 OK`): 返回预测常数列表。
    -   失败响应: `400 Bad Request`, `404 Not Found`, `409 Conflict`。

-   ***旧版兼容接口***: `GET /song/info` 和 `POST /song/record` 依然可用，但推荐使用新的 `/song/search/*` 接口。

### 图片生成

-   **`POST /image/bn/{n}`**
    -   描述: 生成用户的Best N成绩图片。
    -   路径参数: `n` (整数, 必须大于0)
    -   请求体: `IdentifierRequest`
    -   成功响应 (`200 OK`): 返回二进制PNG格式的图片数据。
    -   失败响应: `400 Bad Request`, `401 Unauthorized`, `404 Not Found`, `500 Internal Server Error`。

-   **`POST /image/song`**
    -   描述: 生成指定歌曲的成绩图片。
    -   查询参数: `q` (必需) - 歌曲关键词。
    -   请求体: `IdentifierRequest`
    -   成功响应 (`200 OK`): 返回二进制PNG格式的图片数据。
    -   失败响应: `400 Bad Request`, `401 Unauthorized`, `404 Not Found`, `409 Conflict`。

-   **`GET /image/leaderboard/rks`**
    -   描述: 生成RKS排行榜图片。
    -   查询参数: `limit` (可选) - 显示的玩家数量，默认为20。
    -   成功响应 (`200 OK`): 返回二进制PNG格式的图片数据。
    -   失败响应: `500 Internal Server Error`。

## 数据模型

系统使用以下主要数据模型：

1. **内部用户 (InternalUser)**
   - 每个用户都有一个系统生成的内部ID (`internal_id`)。
   - 包含可选的昵称和用户相关信息。

2. **平台绑定 (PlatformBinding)**
   - 关联内部用户ID与各平台账号。
   - 一个内部用户可以关联多个平台账号。
   - 每个平台账号绑定一个SessionToken。
   - 平台名称 (`platform`) 大小写不敏感。

3. **验证码 (UnbindVerificationCode)**
   - 用于验证解绑请求。
   - 通过平台名称和平台ID关联到特定绑定。

4. **存档 (GameSave)**
   - 包含玩家游戏进度、设置和成绩记录 (`game_record`)。
   - `game_record` 是一个嵌套的 `HashMap`，结构为 `Map<SongId, Map<DifficultyString, SongRecord>>`。
   - `SongRecord` 包含 `score`, `acc`, `fc`, `difficulty` (定数), `rks` 等字段。

5. **RKS记录 (RksRecord)**
   - 用于计算和展示RKS相关的成绩，通常按RKS值排序。

## Docker 部署

如果你想使用 Docker 运行 Phi-Backend，本项目提供了 Dockerfile 和 docker-compose.yml 文件。

1. **使用 Docker Compose (推荐)**

   使用 Docker Compose 可以更简单地管理容器、数据卷和环境变量。

   ```bash
   # 确保Docker和Docker Compose已安装
   # 启动服务
   docker-compose up -d
   
   # 查看日志
   docker-compose logs -f
   ```

   服务将在 `http://<your-host>:8080` 启动。

2. **手动使用 Docker**

   你也可以手动构建和运行Docker镜像。

   ```bash
   # 构建镜像
   docker build -t phi-backend .
   
   # 创建数据目录
   mkdir -p data info
   
   # 运行容器
   docker run -d --name phi-backend \
     -p 8080:8080 \
     -v $(pwd)/data:/app/data \
     -v $(pwd)/info:/app/info \
     -e DATABASE_URL=sqlite:/app/data/phigros_bindings.db \
     -e INFO_DATA_PATH=/app/info \
     -e HOST=0.0.0.0 \
     phi-backend
   ```

### Docker 环境变量配置

在 Docker 环境中，你可以通过环境变量设置配置，无需使用 `.env` 文件。关键的环境变量有:

- `DATABASE_URL`: 数据库连接URL (例如 `sqlite:/app/data/phigros_bindings.db`)
- `INFO_DATA_PATH`: 数据文件目录 (例如 `/app/info`)
- `HOST`: 绑定的主机地址，在容器中通常为 `0.0.0.0`
- `PORT`: 服务端口号
- `RUST_LOG`: 日志级别

这些环境变量可以在 `docker-compose.yml` 的 `environment` 部分进行配置，或在 `docker run` 命令中通过 `-e` 参数设置。

### 数据持久化

Docker 配置使用卷映射来持久化数据:

- `./data:/app/data`: 存储数据库文件
- `./info:/app/info`: 存储歌曲信息文件

确保这些目录在宿主机上存在并且包含必要的文件。

在使用 Phi-Backend 之前，请确保 `info` 目录中包含必要的数据文件 (`info.csv`, `difficulty.csv`, `nicklist.yaml`)，这些可以从 [phi-plugin](https://github.com/Catrong/phi-plugin) 获取。

## 致谢

该项目的大部分代码由LLM生成，设计和部分数据处理逻辑参考了 [phi-plugin](https://github.com/Catrong/phi-plugin)

在此一并表示感谢awa