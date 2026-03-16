# Stateless Codex

A Rust library for building stateless, resumable AI agents with tool use, plugin hooks, memory, and skill resolution — powered by an append-only event ledger.

## Overview

Stateless Codex implements a **Hexagonal Architecture** (Ports and Adapters) where the agent itself holds no mutable conversational state between sessions. All session state is projected on-demand from an append-only event ledger (the "codex"), making the system:

- **Resumable** — close a session, persist the ledger, and restore it identically later.
- **Auditable** — every user message, assistant response, tool call, and tool result is recorded as a ledger event.
- **Deterministic** — session projections (context windows, summaries, compaction state) are pure functions of the ledger.

## Architecture

```
┌──────────────────────────────────────────────────────┐
│  api/          Public Surface (Agent, SessionHandle)  │
├──────────────────────────────────────────────────────┤
│  application/  Use-Case Orchestration                 │
│    TurnEngine, SessionService, ContextManager,        │
│    PromptBuilder, ToolDispatcher, SkillResolver, ...  │
├──────────────────────────────────────────────────────┤
│  domain/       Pure Domain Model (zero I/O)           │
│    Ledger, Config, Content, Events, Tools, Hooks, ... │
├──────────────────────────────────────────────────────┤
│  ports/        Abstract Interfaces (traits)           │
│    ModelProvider, ToolHandler, Plugin,                 │
│    SessionStorage, MemoryStorage                      │
├──────────────────────────────────────────────────────┤
│  support/      Concrete Adapters                      │
│    InMemorySessionStorage, InMemoryMemoryStorage      │
└──────────────────────────────────────────────────────┘
```

### Layer Breakdown

| Layer | Purpose |
|---|---|
| **`api/`** | Public entry points — `Agent`, `AgentBuilder`, `SessionHandle`, `RunningTurn`. Type-state builder ensures at least one model provider is registered before `build()`. |
| **`application/`** | Orchestrates domain types through port traits. Contains the agentic tool loop (`TurnEngine`), session lifecycle management (`SessionService`), context projection (`ContextManager`), prompt assembly (`PromptBuilder`), and tool dispatch (`ToolDispatcher`). |
| **`domain/`** | Pure value types with zero external dependencies — `SessionLedger`, `AgentConfig`, `ContentBlock`, `AgentEvent`, `HookKind`, `ToolDescriptor`, `SkillDefinition`, etc. |
| **`ports/`** | Trait definitions for driven adapters — `ModelProvider`, `ToolHandler`, `Plugin`, `SessionStorage`, `MemoryStorage`. All async-trait for object safety. |
| **`support/`** | In-memory implementations of storage traits, suitable for testing and single-process usage. |

## Key Design Patterns

- **Event Sourcing** — `SessionLedger` is the single source of truth. Summaries, context windows, and compaction state are projections computed from the ledger.
- **Type-State Builder** — `AgentBuilder<NoProvider>` / `AgentBuilder<HasProvider>` enforces that `build()` requires at least one model provider, checked at compile time.
- **Reserve-Commit-Rollback** — Session slot acquisition uses a reservation pattern to prevent race conditions during async initialization.
- **Structured Concurrency** — `TurnEngine` spawns tasks with `CancellationToken` and `catch_unwind()` for panic safety, converting panics to `TurnOutcome::Panicked`.
- **Mutating vs Non-Mutating Tool Scheduling** — Read-only tools run concurrently via `join_all`; mutating tools are serialized to preserve correctness.
- **Plugin Hook Pipeline** — Plugins declare which hooks they tap. `BeforeToolUse` hooks can rewrite tool arguments (with full audit trail of both requested and effective arguments).

## Request Flow

```
session_handle.send_message(content)
  │
  ├─ TurnEngine::spawn()
  │    ├─ Validate input
  │    ├─ Acquire turn lock
  │    └─ Spawn async task → returns RunningTurn { event_stream, controller, outcome }
  │
  └─ TurnEngine::run()  [inside spawned task]
       1. Emit TurnStarted, append UserMessage to ledger
       2. Resolve /skill commands from user input
       3. Fire on_turn_start plugin hooks (collect dynamic prompt sections)
       4. Load turn-specific memories via semantic search
       5. [Tool Loop]
          a. Build request context from ledger projection
          b. Render system prompt (instructions + skills + plugins + memories + env)
          c. Send ChatRequest to model provider (streaming)
          d. On ToolCalls → dispatch batch (with plugin hooks + timeouts) → loop
          e. On Stop → append AssistantMessage, fire on_turn_end, emit TurnFinished
          f. On Cancelled → mark message as Incomplete
```

## Usage

```rust
use codex_codex::api::{Agent, AgentBuilder};

// Build an agent with a model provider, tools, and optional plugins
let agent = AgentBuilder::new(config)
    .register_model_provider(my_provider)
    .register_tool(my_tool)
    .register_skill(my_skill)
    .build()?;

// Create a new session
let session = agent.new_session(session_config).await?;

// Send a message and stream events
let mut turn = session.send_message(content).await?;

while let Some(envelope) = turn.events_mut().next().await {
    match envelope.event {
        AgentEvent::TextDelta { text } => print!("{text}"),
        AgentEvent::ToolCallStart { tool_name, .. } => println!("[calling {tool_name}]"),
        AgentEvent::TurnFinished => break,
        _ => {}
    }
}

let outcome = turn.join().await;
```

## Dependencies

| Crate | Purpose |
|---|---|
| `tokio` | Async runtime (multi-thread, sync, time) |
| `tokio-stream` / `tokio-util` | Async stream utilities |
| `serde` / `serde_json` | Serialization |
| `chrono` | Timestamps with clock and serde support |
| `uuid` | Session and event IDs (v4) |
| `thiserror` | Structured error types |
| `async-trait` | Object-safe async trait definitions |
| `futures` | Stream combinators |

## Development

```bash
# Build
cargo build

# Run tests
cargo test

# Format (nightly)
cargo +nightly fmt

# Lint
cargo clippy -- -D warnings
```

## Acknowledgements

This project is based on [OpenAI Codex CLI](https://github.com/openai/codex). The architecture and code are derived from the Codex project and re-implemented in Rust with a hexagonal architecture approach.

## License

See [LICENSE](LICENSE) for details.
