# NAI Codex

> Codex 顺便生成的文档，自用没保证准确喵。

NAI Codex 是一个面向 NovelAI 的图像生成工具集，提供 Rust 后端（axum）与 Vue/Quasar 前端的完整工作流。

## 当前状态与限制

- 只有V4+的文生图功能，因为其它我会去官网用。
- 目前仅设计并支持“镜像容器部署”的运行模式（Docker/Docker Compose）。
- 非容器化的本地运行与完整工作流尚未细化，相关文档也仍待完善。

## 功能概览

- NovelAI 生成任务队列与记录管理
- Snippet / Preset / 主预设管理与预览图
- Prompt 解析、格式化与 dry-run 预览
- 词库检索（内嵌 lexicon）
- 画廊与预览图静态服务
- 归档：按日期打包 gallery 并同步清理记录

## 架构说明

- `src/`：二进制入口与环境配置
- `libs/core`：存储（redb）、预设/片段、prompt 解析、归档
- `libs/server`：HTTP API、任务队列、归档接口
- `libs/api`：NovelAI 请求类型与客户端
- `web/`：Vue/Quasar 单页应用

## 快速开始（容器部署）

1. 准备环境变量

```bash
cp .env.example .env
```

编辑 `.env`，至少设置 `CODEX_NAI_TOKEN`。

2. 启动容器

```bash
docker compose up -d
```

默认监听 `0.0.0.0:8080`，数据会写入 `./data`。

## 环境变量

- 必需：`CODEX_NAI_TOKEN`
- 可选：
  - `CODEX_ADDR`（默认 `0.0.0.0:8080`）
  - `CODEX_DB_PATH`（默认 `data/codex.redb`）
  - `CODEX_PREVIEW_DIR`（默认 `data/previews`）
  - `CODEX_GALLERY_DIR`（默认 `data/gallery`）
  - `CODEX_STATIC_DIR`（默认 `/app/static`）
  - `RUST_LOG`（日志级别）

## 开发与构建

后端：

```bash
cargo build
# 或
cargo run
```

Windows 开发脚本：

```powershell
./start-dev.ps1 -Mode dev
```

前端：

```bash
cd web
pnpm install
pnpm dev
```

构建前端：

```bash
cd web
pnpm build
```

## 测试与格式化

```bash
cargo test -p codex-core
```

```bash
cd web
pnpm lint
pnpm format
```

## 许可证

GPL-3.0-or-later
