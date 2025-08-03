# 项目重构最终蓝图

本文档旨在规划和指导 Phi-Backend-Rust 项目的重构工作，以提高代码质量、可维护性和可扩展性。

## 阶段一：结构与逻辑清理 (Structural & Logical Cleanup)

*目标：解决文件混乱和逻辑冲突，为后续工作奠定清晰的基础。*

1.  **合并 RKS 逻辑**:
    *   删除 `src/controllers/rks_controller.rs` 文件。
    *   将 `rks_controller.rs` 中的 `get_bn` 函数移动到 `src/controllers/rks.rs` 中，并修改它，使其复用 `rks.rs` 中 `get_rks` 的完整逻辑，而不是自己去调用服务。
2.  **清理其他控制器**:
    *   对 `save.rs` / `save_controller.rs` 和 `song.rs` / `song_controller.rs` 执行类似操作，保留功能更完整的版本，并进行合并。
3.  **统一文件命名**:
    *   将所有幸存的控制器文件重命名为简洁的单数形式，例如 `auth_controller.rs` -> `auth.rs`。
4.  **重构模块入口 (`controllers/mod.rs`)**:
    *   删除此文件中的所有 `pub use ...` 语句。
    *   只保留模块声明，如 `pub mod auth;` `pub mod rks;` 等。
5.  **更新路由 (`routes.rs`)**:
    *   修改所有路由定义，使用完整的、层级化的路径来调用控制器函数，例如 `controllers::rks::get_rks`。

---
## 阶段二：API 规范化 (API Standardization)

*目标：统一所有API的输入输出，并提供自动化文档。*

6.  **引入 `utoipa`**:
    *   在 `Cargo.toml` 中添加 `utoipa` 和 `utoipa-swagger-ui` 依赖。
7.  **定义标准响应体**:
    *   在项目中创建一个通用的 `ApiResponse<T>` 结构体（或改造现有结构），并让它实现 `utoipa::ToSchema`。
8.  **配置 Swagger UI**:
    *   在 `main.rs` 中添加一个新的路由，用于托管自动生成的 Swagger UI 界面。
9.  **改造所有端点**:
    *   审查所有控制器函数，确保它们都返回统一的 `ApiResponse`。
    *   为所有控制器函数和相关的数据模型添加 `#[utoipa::path(...)]` 和 `#[derive(utoipa::ToSchema)]` 宏，以生成文档。