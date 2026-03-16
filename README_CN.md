# Stateless Codex

一个用于构建无状态、可恢复 AI Agent 的 Rust 库，支持工具调用、插件钩子、记忆系统和技能解析 —— 基于仅追加事件账本（append-only event ledger）驱动。

## 概述

Stateless Codex 采用**六边形架构**（Ports and Adapters），Agent 本身在会话之间不持有任何可变的对话状态。所有会话状态均从仅追加的事件账本（即 "codex"）按需投影而来，使系统具备以下特性：

- **可恢复** — 关闭会话、持久化账本，之后可以完整地恢复会话。
- **可审计** — 每条用户消息、助手回复、工具调用和工具结果都记录为账本事件。
- **确定性** — 会话投影（上下文窗口、摘要、压缩状态）是账本的纯函数。

## 架构

```
┌──────────────────────────────────────────────────────┐
│  api/          公共接口层 (Agent, SessionHandle)       │
├──────────────────────────────────────────────────────┤
│  application/  用例编排层                              │
│    TurnEngine, SessionService, ContextManager,        │
│    PromptBuilder, ToolDispatcher, SkillResolver, ...  │
├──────────────────────────────────────────────────────┤
│  domain/       纯领域模型层（零 I/O）                   │
│    Ledger, Config, Content, Events, Tools, Hooks, ... │
├──────────────────────────────────────────────────────┤
│  ports/        抽象接口层（trait 定义）                  │
│    ModelProvider, ToolHandler, Plugin,                 │
│    SessionStorage, MemoryStorage                      │
├──────────────────────────────────────────────────────┤
│  support/      具体适配器层                             │
│    InMemorySessionStorage, InMemoryMemoryStorage      │
└──────────────────────────────────────────────────────┘
```

### 分层说明

| 层 | 职责 |
|---|---|
| **`api/`** | 公共入口 — `Agent`、`AgentBuilder`、`SessionHandle`、`RunningTurn`。类型状态构建器在编译期确保至少注册一个模型提供者后才能调用 `build()`。 |
| **`application/`** | 通过端口 trait 编排领域类型。包含 Agent 工具循环（`TurnEngine`）、会话生命周期管理（`SessionService`）、上下文投影（`ContextManager`）、提示词组装（`PromptBuilder`）和工具调度（`ToolDispatcher`）。 |
| **`domain/`** | 零外部依赖的纯值类型 — `SessionLedger`、`AgentConfig`、`ContentBlock`、`AgentEvent`、`HookKind`、`ToolDescriptor`、`SkillDefinition` 等。 |
| **`ports/`** | 被驱动适配器的 trait 定义 — `ModelProvider`、`ToolHandler`、`Plugin`、`SessionStorage`、`MemoryStorage`。全部使用 async-trait 以实现对象安全。 |
| **`support/`** | 存储 trait 的内存实现，适用于测试和单进程使用场景。 |

## 核心设计模式

- **事件溯源（Event Sourcing）** — `SessionLedger` 是唯一的事实来源。摘要、上下文窗口和压缩状态都是从账本计算出的投影。
- **类型状态构建器（Type-State Builder）** — `AgentBuilder<NoProvider>` / `AgentBuilder<HasProvider>` 在编译期强制 `build()` 需要至少一个模型提供者。
- **预留-提交-回滚（Reserve-Commit-Rollback）** — 会话槽位获取采用预留模式，防止异步初始化过程中的竞态条件。
- **结构化并发（Structured Concurrency）** — `TurnEngine` 使用 `CancellationToken` 和 `catch_unwind()` 生成任务，确保 panic 安全，将 panic 转换为 `TurnOutcome::Panicked`。
- **可变/不可变工具调度** — 只读工具通过 `join_all` 并发执行；有副作用的工具串行执行以保证正确性。
- **插件钩子管道（Plugin Hook Pipeline）** — 插件声明它们关注的钩子。`BeforeToolUse` 钩子可以重写工具参数（同时完整记录原始参数和生效参数的审计轨迹）。

## 请求流程

```
session_handle.send_message(content)
  │
  ├─ TurnEngine::spawn()
  │    ├─ 校验输入
  │    ├─ 获取轮次锁
  │    └─ 生成异步任务 → 返回 RunningTurn { event_stream, controller, outcome }
  │
  └─ TurnEngine::run()  [在生成的任务内部]
       1. 发出 TurnStarted 事件，将 UserMessage 追加到账本
       2. 从用户输入中解析 /skill 命令
       3. 触发 on_turn_start 插件钩子（收集动态提示词片段）
       4. 通过语义搜索加载本轮相关记忆
       5. [工具循环]
          a. 从账本投影构建请求上下文
          b. 渲染系统提示词（指令 + 技能 + 插件 + 记忆 + 环境）
          c. 向模型提供者发送 ChatRequest（流式）
          d. 收到 ToolCalls → 调度工具批次（含插件钩子 + 超时） → 循环
          e. 收到 Stop → 追加 AssistantMessage，触发 on_turn_end，发出 TurnFinished
          f. 收到 Cancelled → 将消息标记为 Incomplete
```

## 使用示例

```rust
use codex_codex::api::{Agent, AgentBuilder};

// 构建 Agent：注册模型提供者、工具和可选插件
let agent = AgentBuilder::new(config)
    .register_model_provider(my_provider)
    .register_tool(my_tool)
    .register_skill(my_skill)
    .build()?;

// 创建新会话
let session = agent.new_session(session_config).await?;

// 发送消息并流式接收事件
let mut turn = session.send_message(content).await?;

while let Some(envelope) = turn.events_mut().next().await {
    match envelope.event {
        AgentEvent::TextDelta { text } => print!("{text}"),
        AgentEvent::ToolCallStart { tool_name, .. } => println!("[调用 {tool_name}]"),
        AgentEvent::TurnFinished => break,
        _ => {}
    }
}

let outcome = turn.join().await;
```

## 依赖

| Crate | 用途 |
|---|---|
| `tokio` | 异步运行时（多线程、同步原语、定时器） |
| `tokio-stream` / `tokio-util` | 异步流工具 |
| `serde` / `serde_json` | 序列化/反序列化 |
| `chrono` | 时间戳（含时钟和 serde 支持） |
| `uuid` | 会话和事件 ID（v4） |
| `thiserror` | 结构化错误类型 |
| `async-trait` | 对象安全的异步 trait 定义 |
| `futures` | Stream 组合子 |

## 开发

```bash
# 构建
cargo build

# 运行测试
cargo test

# 格式化（需要 nightly）
cargo +nightly fmt

# 代码检查
cargo clippy -- -D warnings
```

## 许可证

详见 [LICENSE](LICENSE)。
