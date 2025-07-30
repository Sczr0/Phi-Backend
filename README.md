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

### 用户绑定

-   **`POST /bind`**
    -   描述: 绑定平台ID和Phigros SessionToken。系统会返回一个唯一的内部用户ID (`internal_id`)。
    -   注意:
        -   同一个平台 (`platform`) 下的同一个ID (`platform_id`) 只能绑定一个SessionToken。
        -   如果使用相同的 `platform` 和 `platform_id` 尝试绑定不同的 `token`，会更新绑定的 `token`。
        -   如果一个 `token` 已被其他 `platform` 和 `platform_id` 绑定，那么当前 `platform` 和 `platform_id` 会被关联到同一个内部用户。
        -   `platform` 字段大小写不敏感。
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
    -   失败响应:
        -   `400 Bad Request`: 参数错误（例如 `token` 格式无效）。
        -   `500 Internal Server Error`: 数据库错误或其他内部错误。

-   **`POST /token/list`**
    -   描述: 获取用户关联的所有平台ID和Token列表。
    -   请求体: `IdentifierRequest` (提供 `token` 或 `platform` + `platform_id` 来识别用户)
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
                  },
                  {
                      "platform": "discord",
                      "platform_id": "987654321",
                      "session_token": "token_for_discord",
                      "bind_time": "2025-01-02T13:00:00Z"
                  }
              ]
          }
      }
      ```
    -   失败响应:
        -   `400 Bad Request`: 未提供有效的身份标识。
        -   `404 Not Found`: 未找到用户绑定。
        -   `500 Internal Server Error`: 数据库错误。

-   **`POST /unbind`**
    -   描述: 解除指定平台账号的绑定。提供两种方式：
        1.  **平台ID + Token 验证**: 提供与绑定记录完全匹配的平台、平台ID和SessionToken。
        2.  **简介验证**:
         - 仅提供平台和平台ID，系统会返回一个验证码。
         - 提供平台、平台ID和之前收到的验证码，系统将检查游戏内简介是否与验证码匹配。
    -   注意: `platform` 字段大小写不敏感。
    -   请求体: `IdentifierRequest` (必须包含 `platform`, `platform_id`; 可选 `token` 或 `verification_code`)
        ```json
        // 方式一：平台+平台ID+Token
        {
            "platform": "qq",
            "platform_id": "123456",
            "token": "token_for_qq"
        }
        // 方式二：简介验证 - 获取验证码
        {
            "platform": "qq",
            "platform_id": "123456"
        }
        // 简介验证 - 提交验证码
        {
            "platform": "qq",
            "platform_id": "123456",
            "verification_code": "ABCDEF12"
        }
        ```
    -   成功响应:
        -   方式一 (Token验证)、方式三 (简介验证成功): `200 OK`
          ```json
          {
              "code": 200,
              "status": "success",
              "message": "解绑成功 (平台ID+Token验证)", // 或 "解绑成功 (简介验证)"
              "data": {
                  "internal_id": "解绑前的内部用户ID"
              }
          }
          ```
        -   方式二 (验证码获取): `200 OK`，状态为 `verification_initiated`。
          ```json
          {
              "code": 200,
              "status": "verification_initiated",
              "message": "请在 xxx 秒内将您的 Phigros 简介修改为此验证码...",
              "data": {
                  "verification": {
                      "verification_code": "ABCDEF12",
                      "expires_in_seconds": 300,
                      "message": "请在 300 秒内将您的 Phigros 简介修改为此验证码..."
                  },
                  "internal_id": "用户内部ID"
              }
          }
          ```
    -   失败响应:
        -   `400 Bad Request`: 请求参数错误（例如方式一中 `token` 不匹配，方式三中验证码无效）。
        -   `401 Unauthorized`: Token格式无效，或简介验证时使用的存储 `token` 失效。
        -   `404 Not Found`: 提供的平台和平台ID未绑定。
        -   `500 Internal Server Error`: 数据库或获取存档时发生内部错误。

### 存档与RKS

-   **`POST /get/cloud/saves`**
    -   描述: 获取并解析用户的Phigros云存档（不含难度定数和RKS）。**注意：** 返回的 `game_record` 仅包含 `score`, `acc`, `fc`。
    -   请求体: `IdentifierRequest`
    -   成功响应 (`200 OK`):
      ```json
      {
          "code": 200,
          "status": "ok",
          "message": null,
          "data": {
              "game_key": "...",
              "game_progress": { ... },
              "game_record": {
                  "SONG_ID_1": {
                      "EZ": { "score": ..., "acc": ..., "fc": ... },
                      "HD": { "score": ..., "acc": ..., "fc": ... }
                  }
              },
              "settings": { ... },
              "user": { ... },
              "nickname": "玩家昵称" // 如果获取成功
          }
      }
      ```
    -   失败响应: `401 Unauthorized` (无效Token), `404 Not Found` (未找到存档), `500 Internal Server Error`。

-   **`POST /get/cloud/saves/with_difficulty`**
    -   描述: 获取并解析用户的Phigros云存档，**包含难度定数和计算出的RKS值**。
    -   请求体: `IdentifierRequest`
    -   成功响应 (`200 OK`): 返回完整的 `GameSave` 结构，其中 `SongRecord` 包含 `difficulty` 和 `rks` 字段。
      ```json
      {
          "code": 200,
          "status": "success",
          "message": "成功获取并解析带定数的云存档",
          "data": { // 完整的 GameSave 结构
              "game_key": "...",
              "game_progress": { ... },
              "game_record": {
                  "SONG_ID_1": {
                      "EZ": { "score": ..., "acc": ..., "fc": ..., "difficulty": ..., "rks": ... },
                      "HD": { "score": ..., "acc": ..., "fc": ..., "difficulty": ..., "rks": ... }
                  }
              },
              "settings": { ... },
              "user": { ... }
          }
      }
      ```
    -   失败响应: `401 Unauthorized`, `404 Not Found`, `500 Internal Server Error`。

-   **`POST /rks`**
    -   描述: 计算并返回用户所有歌曲的RKS分数，按分数由高到低排序。
    -   请求体: `IdentifierRequest`
    -   成功响应 (`200 OK`): 返回 `RksResult` 结构。
      ```json
      {
          "code": 200,
          "status": "success",
          "message": "RKS计算成功",
          "data": {
              "records": [
                  {
                      "song_id": "...", 
                      "song_name": "...", 
                      "difficulty": "IN", 
                      "difficulty_value": ..., 
                      "acc": ..., 
                      "score": ..., 
                      "rks": ...
                  },
                  // ... 其他记录
              ]
          }
      }
      ```
    -   失败响应: `401 Unauthorized`, `404 Not Found`, `500 Internal Server Error`。

-   **`POST /b30`**
    -   描述: 计算并返回用户的B30 (Best 30) 成绩列表。
    -   请求体: `IdentifierRequest`
    -   成功响应 (`200 OK`): 返回 `RksRecord` 列表 (最多30条)。
      ```json
      {
          "code": 200,
          "status": "success",
          "message": "B30获取成功",
          "data": [
              // RksRecord 列表
          ]
      }
      ```
    -   失败响应: `401 Unauthorized`, `404 Not Found`, `500 Internal Server Error`。

-   **`POST /bn/{n}`** (例如 `/bn/40` 获取B40)
    -   描述: 计算并返回用户的 Best N 成绩列表。路径参数 `n` 指定 N 的大小 (默认为 30)。
    -   请求体: `IdentifierRequest`
    -   路径参数: `n` (整数, 指定最佳成绩的数量)
    -   成功响应 (`200 OK`): 返回 `RksRecord` 列表 (最多n条)。
      ```json
      {
          "code": 200,
          "status": "success",
          "message": "B{n}获取成功",
          "data": [
              // RksRecord 列表
          ]
      }
      ```
    -   失败响应: `400 Bad Request` (n不是有效数字), `401 Unauthorized`, `404 Not Found`, `500 Internal Server Error`。

### 歌曲查询

-   **`GET /song/search`**
    -   描述: 统一搜索歌曲信息，支持通过ID、歌曲名和别名查询。
    -   参数: `q` (必需) - 搜索关键词
    -   示例: `/song/search?q=fractured+angel`
    -   成功响应 (`200 OK`):
      ```json
      {
          "code": 200,
          "status": "OK",
          "message": null,
          "data": {
              "id": "FracturedAngel.Cametek",
              "song": "Fractured Angel",
              "artist": "Camellia",
              "difficulty": {
                  "ez": 3.5,
                  "hd": 7.8,
                  "in": 13.7,
                  "at": 15.8
              }
          }
      }
      ```
    -   失败响应:
        -   `400 Bad Request`: 未提供搜索关键词。
        -   `404 Not Found`: 未找到匹配的歌曲。
        -   `409 Conflict`: 搜索结果不唯一，提供了可能的匹配列表。

-   **`GET /song/search/predictions`**
    -   描述: 查询歌曲的预测常数信息，支持通过ID、歌曲名和别名查询。
    -   参数:
        -   `q` (必需) - 搜索关键词（ID、歌名或别名）
        -   `difficulty` (可选) - 指定难度，可选值：`EZ`、`HD`、`IN`、`AT`。如不提供，则返回所有难度。
    -   示例: `/song/search/predictions?q=fractured+angel&difficulty=IN`
    -   成功响应 (`200 OK`):
      ```json
      {
          "code": 200,
          "status": "OK",
          "message": null,
          "data": [
              {
                  "song_id": "FracturedAngel.Cametek",
                  "difficulty": "IN",
                  "predicted_constant": 13.8
              }
          ]
      }
      ```
    -   如果不指定难度，则返回所有难度:
      ```json
      {
          "code": 200,
          "status": "OK",
          "message": null,
          "data": [
              {
                  "song_id": "FracturedAngel.Cametek",
                  "difficulty": "EZ",
                  "predicted_constant": 3.6
              },
              {
                  "song_id": "FracturedAngel.Cametek",
                  "difficulty": "HD",
                  "predicted_constant": 7.9
              },
              {
                  "song_id": "FracturedAngel.Cametek",
                  "difficulty": "IN",
                  "predicted_constant": 13.8
              },
              {
                  "song_id": "FracturedAngel.Cametek",
                  "difficulty": "AT",
                  "predicted_constant": 15.9
              }
          ]
      }
      ```
    -   失败响应:
        -   `400 Bad Request`: 未提供搜索关键词。
        -   `404 Not Found`: 未找到匹配的歌曲或预测常数数据。
        -   `409 Conflict`: 搜索结果不唯一，提供了可能的匹配列表。

-   **`POST /song/search/record`**
    -   描述: **(推荐)** 统一查询指定歌曲的成绩记录。
    -   查询参数:
        -   `q`: (必需) 歌曲ID、名称或别名。
        -   `difficulty`: (可选) 难度级别 (EZ, HD, IN, AT)。如果提供，只返回该难度的成绩；否则返回所有难度的成绩。
    -   请求体: `IdentifierRequest`
    -   示例: `POST /song/search/record?q=打工人&difficulty=IN`
    -   成功响应 (`200 OK`):
      ```json
      {
          "code": 200,
          "status": "OK",
          "message": null,
          "data": {
              "EZ": { ... }, // SongRecord 结构，包含 difficulty 和 rks
              "HD": { ... }
              // ... 其他难度
          }
      }
      ```
    -   失败响应: `400 Bad Request`, `401 Unauthorized`, `404 Not Found`, `409 Conflict` (歌曲查询结果不唯一), `500 Internal Server Error`。

### 旧版歌曲接口 (兼容)

-   **`GET /song/info`**
    -   描述: 获取歌曲信息（旧版接口）。
    -   查询参数 (至少提供一个):
        -   `song_id`: 歌曲ID。
        -   `song_name`: 歌曲名称。
        -   `nickname`: 歌曲别名。
    -   成功响应 (`200 OK`): 返回 `SongInfo`。
    -   失败响应: `400 Bad Request`, `404 Not Found`, `409 Conflict`。

-   **`POST /song/record`**
    -   描述: 获取特定歌曲的成绩记录（旧版接口）。
    -   查询参数 (至少提供一个):
        -   `song_id`: 歌曲ID。
        -   `song_name`: 歌曲名称。
        -   `nickname`: 歌曲别名。
        -   `difficulty`: (可选) 难度级别。
    -   请求体: `IdentifierRequest`
    -   成功响应 (`200 OK`): 返回包含 `SongRecord` 的 `HashMap`。
    -   失败响应: `400 Bad Request`, `401 Unauthorized`, `404 Not Found`, `409 Conflict`, `500 Internal Server Error`。

### 图片生成接口

-   **`POST /image/bn/{n}`**
    -   描述: 生成用户的Best N成绩图片。
    -   路径参数: `n` (整数, 指定最佳成绩的数量)
    -   请求体: `IdentifierRequest`
    -   成功响应 (`200 OK`): 返回PNG格式的图片数据。
    -   失败响应: `400 Bad Request` (n不是有效数字), `401 Unauthorized`, `404 Not Found`, `500 Internal Server Error`。

-   **`POST /image/song`**
    -   描述: 生成指定歌曲的成绩图片。
    -   查询参数:
        -   `q`: (必需) 歌曲ID、名称或别名。
    -   请求体: `IdentifierRequest`
    -   示例: `POST /image/song?q=痉挛`
    -   成功响应 (`200 OK`): 返回PNG格式的图片数据。
    -   失败响应: `400 Bad Request`, `401 Unauthorized`, `404 Not Found`, `409 Conflict` (歌曲查询结果不唯一), `500 Internal Server Error`。

-   **`GET /image/leaderboard/rks`**
    -   描述: 生成RKS排行榜图片。（暂不推荐使用）
    -   查询参数:
        -   `limit`: (可选) 显示的玩家数量，默认为20。
    -   示例: `GET /image/leaderboard/rks?limit=10`
    -   成功响应 (`200 OK`): 返回PNG格式的图片数据。
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