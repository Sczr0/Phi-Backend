# Phi-Backend

基于Rust的Phigros后端服务，提供绑定、存档解析、RKS计算以及单曲成绩查询等功能。

## 功能

- 绑定Phigros的sessionToken
- 获取并解析Phigros存档
- 计算玩家RKS及其构成成绩 (B30, Best N)
- 查询单曲成绩
- 统一查询歌曲信息（支持ID、名称、别名）

## 使用前提

该服务需要使用Phigros游戏的数据文件，包括：

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
    ```dotenv
    DATABASE_URL=sqlite:phigros_bindings.db
    HOST=127.0.0.1
    PORT=8080
    # INFO_DATA_PATH=../info # 可选，默认使用项目根目录下的 info 文件夹
    # INFO_FILE=info.csv
    # DIFFICULTY_FILE=difficulty.csv
    # NICKLIST_FILE=nicklist.yaml
    ```

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
    -   请求体中必须包含 `token` 或 `qq` 字段中的至少一个。
    -   如果两者都提供，优先使用 `token`。
    ```json
    // 方式一：使用 SessionToken
    {
        "token": "用户的Phigros SessionToken"
    }
    // 方式二：使用 QQ 号 (如果已绑定)
    {
        "qq": "用户的QQ号"
    }
    ```

### 用户绑定

-   **`POST /bind`**
    -   描述: 绑定用户的QQ号和Phigros SessionToken。
    -   注意: 一个QQ号只能绑定一个SessionToken。如果尝试绑定的QQ号已被使用，会返回错误。一个SessionToken可以绑定多个QQ号。
    -   请求体:
        ```json
        {
            "qq": "用户的QQ号",
            "token": "用户的Phigros SessionToken"
        }
        ```
    -   成功响应: `200 OK`，包含成功信息。
    -   失败响应: `400 Bad Request` (参数错误), `401 Unauthorized` (Token格式无效), `409 Conflict` (QQ已被绑定), `500 Internal Server Error`。

-   **`POST /unbind`**
    -   描述: 解除用户绑定。提供两种方式：
        1.  **QQ + Token 验证**: 提供与绑定记录完全匹配的QQ号和SessionToken。
        2.  **QQ + 简介验证**: 仅提供QQ号，并需提前将游戏内个人简介修改为 `UNBIND-<你的QQ号>` (例如 `UNBIND-123456`)。
    -   请求体: `IdentifierRequest` (包含 `qq` 和可选的 `token`)
        ```json
        // 方式一：QQ + Token
        {
            "qq": "用户的QQ号",
            "token": "用户的SessionToken"
        }
        // 方式二：QQ + 简介验证
        {
            "qq": "用户的QQ号"
        }
        ```
    -   成功响应: `200 OK`，包含成功信息 (会注明是通过哪种方式解绑)。
    -   失败响应:
        -   `400 Bad Request`: 请求参数错误 (如QQ与Token不匹配，或未按要求提供参数)。
        -   `401 Unauthorized`: Token格式无效 (方式一)，或用于验证简介的Token已失效 (方式二)。
        -   `404 Not Found`: 提供的QQ号未绑定。
        -   `500 Internal Server Error`: 数据库或获取存档时发生内部错误。

### 存档与RKS

-   **`POST /get/cloud/saves`**
    -   描述: 获取并解析用户的Phigros云存档（不含难度定数）。
    -   请求体: `IdentifierRequest`
    -   成功响应: `200 OK`，包含解析后的存档数据。
    -   失败响应: `401 Unauthorized` (无效Token), `404 Not Found` (未找到存档), `500 Internal Server Error`。

-   **`POST /get/cloud/saves/with_difficulty`**
    -   描述: 获取并解析用户的Phigros云存档，并附加上歌曲难度定数信息。
    -   请求体: `IdentifierRequest`
    -   成功响应: `200 OK`，包含带难度定数的存档数据。
    -   失败响应: `401 Unauthorized`, `404 Not Found`, `500 Internal Server Error`。

-   **`POST /rks`**
    -   描述: 由高到低排列单曲rks，输出对应成绩。
    -   请求体: `IdentifierRequest`
    -   成功响应: `200 OK`，包含谱面成绩。
    -   失败响应: `401 Unauthorized`, `404 Not Found`, `500 Internal Server Error`。

-   **`POST /b30`**
    -   描述: 计算用户的B30 (Best 30) 成绩列表。
    -   请求体: `IdentifierRequest`
    -   成功响应: `200 OK`，包含B30成绩列表。
    -   失败响应: `401 Unauthorized`, `404 Not Found`, `500 Internal Server Error`。

-   **`POST /bn/n`** (例如 `/bn/40` 获取B40)
    -   描述: 计算用户的Best N成绩列表 (默认N=30，即B30)。
    -   请求体: `IdentifierRequest`
    -   查询参数: `n` (可选, 整数, 指定N的大小)
    -   成功响应: `200 OK`，包含Best N成绩列表。
    -   失败响应: `400 Bad Request` (n不是有效数字), `401 Unauthorized`, `404 Not Found`, `500 Internal Server Error`。

### 歌曲查询 (推荐使用新接口)

-   **`GET /song/search`**
    -   描述: **(推荐)** 统一查询歌曲信息。自动识别输入是歌曲ID、名称，别名。
    -   查询参数:
        -   `q`: (必需) 歌曲ID、名称或别名。
    -   示例: `/song/search?q=痉挛` 或 `/song/search?q=996.李化禹` 或 `/song/search?q=996`
    -   成功响应: `200 OK`，包含匹配的`SongInfo`。
    -   失败响应: `400 Bad Request` (未提供q参数), `404 Not Found` (找不到歌曲), `409 Conflict` (查询结果不唯一)。

-   **`POST /song/search/record`**
    -   描述: **(推荐)** 统一查询指定歌曲的成绩记录。
    -   查询参数:
        -   `q`: (必需) 歌曲ID、名称或别名。
        -   `difficulty`: (可选) 难度级别 (EZ, HD, IN, AT)。如果提供，只返回该难度的成绩；否则返回所有难度的成绩。
    -   请求体: `IdentifierRequest`
    -   示例: `POST /song/search/record?q=打工人&difficulty=IN`
    -   成功响应: `200 OK`，包含匹配的`SongRecord`。
    -   失败响应: `400 Bad Request`, `401 Unauthorized`, `404 Not Found`, `409 Conflict` (歌曲查询结果不唯一), `500 Internal Server Error`。

### 旧版歌曲接口 (兼容)

-   **`GET /song/info`**
    -   描述: 获取歌曲信息（旧版接口）。
    -   查询参数 (至少提供一个):
        -   `song_id`: 歌曲ID。
        -   `song_name`: 歌曲名称。
        -   `nickname`: 歌曲别名。
    -   成功响应: `200 OK`，包含`SongInfo`。
    -   失败响应: `400 Bad Request`, `404 Not Found`, `409 Conflict`。

-   **`POST /song/record`**
    -   描述: 获取特定歌曲的成绩记录（旧版接口）。
    -   查询参数 (至少提供一个):
        -   `song_id`: 歌曲ID。
        -   `song_name`: 歌曲名称。
        -   `nickname`: 歌曲别名。
        -   `difficulty`: (可选) 难度级别。
    -   请求体: `IdentifierRequest`
    -   成功响应: `200 OK`，包含`SongRecord`。
    -   失败响应: `400 Bad Request`, `401 Unauthorized`, `404 Not Found`, `409 Conflict`, `500 Internal Server Error`。

## 致谢

该项目的设计和部分数据处理逻辑参考了 [phi-plugin](https://github.com/Catrong/phi-plugin)

在此表示感谢awa