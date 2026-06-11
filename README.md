# copilot-openai-proxy

将 GitHub Copilot 的 WebSocket 协议转换为 OpenAI 兼容 HTTP API 的代理服务器。无需登录 Microsoft 账户，通过匿名 Cookie 即可使用 Copilot 后端能力。

编译为单一可执行文件，开箱即用。

## 功能特性

- **OpenAI 兼容 API** — 支持 `/v1/chat/completions`、`/v1/images/generations`、`/v1/models` 等标准端点
- **流式 & 非流式响应** — SSE 实时流式输出 / 完整 JSON 响应，自由切换
- **多模态输入** — 支持 `image_url` content part（data URL 和远程 URL）
- **图像生成** — 通过 Copilot 后端生成图像并返回 URL
- **多模型支持** — default、chat、smart、reasoning、research、computer_use
- **会话管理** — 自动创建 / 过期清理 / 容量驱逐，完整生命周期管理
- **API Key 认证** — 可选的 Bearer Token 访问控制
- **对话历史清理** — 会话删除时自动清理服务端对话历史
- **零依赖部署** — 编译为单个可执行文件，无需额外运行时环境

## 快速开始

### 前置要求

- Rust 1.96+ (2024 Edition)
- 网络可访问 `copilot.microsoft.com`

### 编译

```bash
# 开发构建
cargo build

# 发布构建（推荐，最小体积 + 最高性能）
cargo build --release

# 产物位于
# target/release/copilot-openai-proxy     (Linux/macOS)
# target/release/copilot-openai-proxy.exe (Windows)
```

### 运行

```bash
# 默认启动（监听 0.0.0.0:8080）
./copilot-openai-proxy

# 指定端口 + API Key
./copilot-openai-proxy --port 9090 --api-key "your-secret-key"

# 开启调试日志
./copilot-openai-proxy --debug
```

### 使用示例

```bash
# 非流式对话
curl http://localhost:8080/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "default",
    "messages": [{"role": "user", "content": "你好，介绍一下你自己"}]
  }'

# 流式对话 (SSE)
curl http://localhost:8080/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "smart",
    "messages": [{"role": "user", "content": "用 Rust 写一个快排"}],
    "stream": true
  }'

# 图像生成
curl http://localhost:8080/v1/images/generations \
  -H "Content-Type: application/json" \
  -d '{
    "model": "default",
    "prompt": "一只戴着墨镜的猫咪坐在海边",
    "n": 1,
    "response_format": "url"
  }'

# 多模态输入（图片理解）
curl http://localhost:8080/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "default",
    "messages": [{
      "role": "user",
      "content": [
        {"type": "text", "text": "这张图片里有什么？"},
        {"type": "image_url", "image_url": {"url": "https://example.com/photo.jpg"}}
      ]
    }]
  }'

# 列出可用模型
curl http://localhost:8080/v1/models

# 创建新会话
curl -X POST http://localhost:8080/v1/chat/session \
  -H "X-Session-Id: my-session"

# 删除会话（同时清理服务端对话历史）
curl -X DELETE http://localhost:8080/v1/chat/session \
  -H "X-Session-Id: my-session"

# 带 API Key 认证
curl http://localhost:8080/v1/chat/completions \
  -H "Authorization: Bearer your-secret-key" \
  -H "Content-Type: application/json" \
  -d '{"model":"default","messages":[{"role":"user","content":"hi"}]}'
```

## API 端点

| 方法 | 路径 | 说明 | 认证 |
|------|------|------|------|
| `GET` | `/healthz` | 健康检查 | 否 |
| `GET` | `/v1/models` | 列出可用模型 | 是 |
| `POST` | `/v1/chat/completions` | Chat Completions | 是 |
| `POST` | `/v1/images/generations` | 图像生成 | 是 |
| `POST` | `/v1/chat/session` | 创建会话 | 是 |
| `DELETE` | `/v1/chat/session` | 删除会话 + 清理历史 | 是 |

## 支持的模型

| 模型名称 | 说明 |
|---------|------|
| `default` | 默认模型 |
| `chat` | 聊天模型 |
| `smart` | 智能模型 |
| `reasoning` | 推理模型（也接受 `think-*` 前缀） |
| `research` | 研究模型 |
| `computer_use` | 计算机使用模型（也接受 `computer-use`） |

## 配置参数

```
      --host <HOST>              监听地址          [默认: 0.0.0.0]
      --port <PORT>              监听端口          [默认: 8080]
      --api-key <API_KEY>        API 密钥（空则不启用认证）
      --timeout <TIMEOUT>        请求超时（秒）      [默认: 120]
      --conn-timeout <SECS>      WebSocket 连接超时  [默认: 20]
      --session-ttl <SECS>       会话过期时间        [默认: 1800]
      --cleanup-interval <SECS>  会话清理间隔        [默认: 300]
      --max-sessions <N>         最大会话数          [默认: 1000]
      --debug                    打印原始协议日志
  -h, --help                     显示帮助信息
```

## 工作原理

```
┌──────────────────┐     OpenAI API      ┌──────────────────────────┐    WebSocket     ┌─────────────────────┐
│                  │  ─────────────────▶  │                          │  ─────────────▶  │                     │
│  客户端应用       │  (HTTP JSON/SSE)     │  copilot-openai-proxy    │  (JSON Events)   │  Copilot Backend    │
│  (ChatBox 等)    │  ◀─────────────────  │  :8080                   │  ◀─────────────  │  copilot.microsoft  │
│                  │     OpenAI 格式响应    │                          │   Copilot 事件流   │     .com            │
└──────────────────┘                      └──────────────────────────┘                  └─────────────────────┘
```

1. 客户端发送标准 OpenAI 格式请求到代理
2. 代理通过匿名 Cookie 认证连接 Copilot WebSocket 后端
3. 将 OpenAI 请求转换为 Copilot 事件协议
4. 接收 Copilot 事件流，转换回 OpenAI 格式返回给客户端

### 认证机制

代理使用**匿名模式**访问 Copilot 后端：
- 访问 `https://copilot.microsoft.com` 获取 `__Host-copilot-anon` Cookie
- 使用该 Cookie 建立 WebSocket 连接（`wss://copilot.microsoft.com/c/api/chat`）
- 伪装 User-Agent 为 Android 客户端

## 会话管理

- **自动创建** — 首次请求时自动创建会话（获取 Cookie + WebSocket 连接）
- **会话复用** — 通过 `X-Session-Id` 请求头关联同一会话
- **TTL 过期** — 会话超过 `--session-ttl` 后自动清理（含服务端历史）
- **容量驱逐** — 超过 `--max-sessions` 时自动驱逐最旧会话
- **手动删除** — 通过 `DELETE /v1/chat/session` 主动删除

## 项目结构

```
src/
├── main.rs                    # 入口：CLI 解析 + 服务启动 + 优雅关闭
├── config.rs                  # 配置结构体 (clap derive)
├── error.rs                   # 统一错误类型 + OpenAI 格式错误响应
├── server/
│   ├── mod.rs                 # axum Router + AppState
│   ├── middleware.rs          # API Key Bearer 认证中间件
│   └── handlers/
│       ├── health.rs          # GET /healthz
│       ├── models.rs          # GET /v1/models
│       ├── chat.rs            # POST /v1/chat/completions
│       ├── images.rs          # POST /v1/images/generations
│       └── session.rs         # POST/DELETE /v1/chat/session
├── copilot/
│   ├── client.rs              # CopilotClient 核心（WS 连接 + 事件交互）
│   ├── cookie.rs              # 匿名 Cookie 获取
│   ├── protocol.rs            # WS 事件协议定义 (ClientEvent / ServerEvent)
│   ├── image.rs               # 图片上传 / 下载
│   └── history.rs             # 对话历史删除
├── openai/
│   ├── types.rs               # OpenAI API 数据结构
│   ├── model.rs               # 模型名称映射
│   ├── adapter.rs             # 请求转换（OpenAI messages → Copilot 事件）
│   └── stream.rs              # SSE 流适配器（WS 事件 → SSE chunks）
├── session/
│   ├── manager.rs             # SessionManager 会话池
│   └── state.rs               # SessionState 单会话状态
└── util/
    └── id.rs                  # UUID / chatcmpl ID 生成
```

## 技术栈

| 组件 | 技术选型 |
|------|---------|
| 语言 | Rust (2024 Edition) |
| 异步运行时 | tokio |
| HTTP 服务器 | axum + tower |
| HTTP 客户端 | reqwest (rustls) |
| WebSocket | tokio-tungstenite |
| 序列化 | serde + serde_json |
| CLI 解析 | clap (derive) |
| 日志 | tracing + tracing-subscriber |
| 错误处理 | anyhow + thiserror |

## 错误响应格式

所有错误均返回 OpenAI 兼容格式：

```json
{
  "error": {
    "message": "unsupported model \"gpt-4o\"; available: default, chat, smart, reasoning, research, computer_use",
    "type": "invalid_request_error",
    "code": "400"
  }
}
```

常见错误类型：

| HTTP 状态码 | type | 场景 |
|------------|------|------|
| 400 | `invalid_request_error` | 无效 model、空 messages、参数校验失败 |
| 401 | `invalid_request_error` | API Key 无效或缺失 |
| 502 | `upstream_error` | Copilot 后端连接失败或超时 |
| 500 | `server_error` | 内部错误 |

## 注意事项

- **网络要求**：需要能够访问 `copilot.microsoft.com`（HTTPS + WebSocket）
- **代理设置**：如果使用代理，请确保对 `copilot.microsoft.com` 的 HTTPS 和 WSS 流量放行
- **速率限制**：Copilot 后端可能有请求频率限制，请勿过度请求
- **Cookie 有效期**：匿名 Cookie 有过期时间，会话管理器会自动处理
- **TLS 兼容性**：使用 rustls（纯 Rust TLS 实现），无需系统 OpenSSL
