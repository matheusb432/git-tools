# cqrs

MediatR's request/handler/pipeline model as a zero-cost abstraction: dispatch and middleware resolved entirely at compile time — no DI container, no `dyn`, no boxing, no runtime registry.

## The trio

| Crate | Role |
| -- | -- |
| `cqrs` | the facade — the one dependency you take (`features = ["derive"]`) |
| `cqrs-core` | contracts: `Request`, `RequestHandler`, `Behavior`, `Next`, `Pipeline`, `dispatch` — zero deps |
| `cqrs-macros` | `#[derive(Request)]` and `#[derive(Mediator)]` |

Same shape and reasons as serde/serde_core/serde_derive.

## The boundary: contracts are ours, behaviors are yours

This library ships the `Behavior` trait and **no implementations**. Which cross-cutting concerns exist (logging, timing, validation) and what they do is application policy — implement behaviors in your application layer. That's why `cqrs-core` has zero dependencies.

## Usage

Declare a request with `#[derive(cqrs::Request)]`, naming its response/error types and, optionally, its own per-operation behavior pipeline via `with(...)`:

```rust
#[derive(Debug, cqrs::Request)]
#[request(response = Vec<TodoItem>, error = GetTodosError, with(Timed))]
pub struct GetTodos;
```

Collect one handler field per operation into a mediator struct. `#[derive(cqrs::Mediator)]` implements `RequestHandler<R>` for every `#[handles(R)]`-annotated field; an optional struct-level `#[with(...)]` declares behaviors that wrap *every* dispatch through the mediator:

```rust
#[derive(Clone, cqrs::Mediator)]
#[with(Logged)]
pub struct Mediator<R, Q>
where
    R: TodoRepo + Clone + Send + Sync + 'static,
    Q: IngestQueue + Clone + Send + Sync + 'static,
{
    #[handles(EnqueueCreateTodo)]
    pub create: EnqueueCreateTodoHandler<Q>,
    #[handles(GetTodos)]
    pub list: GetTodosHandler<R>,
}
```

Callers depend on `RequestHandler<SomeRequest>` alone — never on a mediator field or the mediator's concrete type — so an entrypoint stays generic over exactly the one contract it uses:

```rust
pub async fn handle<H>(State(handler): State<H>) -> Result<impl IntoResponse, ApiError>
where
    H: RequestHandler<GetTodos> + Clone + Send + Sync + 'static,
{
    let items = handler.handle(GetTodos).await?;
    // ...
}
```

## Semantics

- Order: mediator `#[with(...)]` left-to-right outermost → request `with(...)` left-to-right → handler.
- Pipelines run on mediator dispatch only; calling a handler directly bypasses them (unit tests stay pure).
- Behaviors pass errors through as values and may short-circuit by returning without calling `next`.

## Non-goals

Stateful behaviors (zero-sized/`Default` only), runtime registration, event/notification engine.

## Known limitation

Generated code emits `::cqrs::` paths — the facade must be in scope under the name `cqrs`.
