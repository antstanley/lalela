# Development Guidelines

**Status:** Draft · **Date:** 2026-10-21 · **Owner:** ant@senzo.io · **Scope:** Repo-wide

This page is the rules of the road for everyone, human or agent, who writes code in the Lalela repository. It covers the toolchain, the pervasive coding style (Tiger Style), defensive coding and assertions, limits, version control, Rust and Swift conventions, repository hygiene, emphases for AI agents, and the definition of done.

How the code is organised (hexagonal layering, crates, dependency direction) lives in [architecture-principles.md](architecture-principles.md). Concrete limit values live in [08-limits.md](08-limits.md).

---

## Toolchain

| Tool | Version / channel | Notes |
|---|---|---|
| Rust | stable, pinned (≥ 1.95) | Pinned in `rust-toolchain.toml`. 1.95 is the eframe 0.37 MSRV. |
| Rust target | `aarch64-apple-darwin` only | macOS 27 runs only on Apple silicon. |
| rustfmt | toolchain default | `max_width = 100` in `rustfmt.toml`. |
| clippy | toolchain default | `cargo clippy --workspace --all-targets --all-features -- -D warnings`. |
| Test runner | `cargo nextest run` | `cargo test --doc` covers doctests, which nextest does not run. |
| Swift | toolchain shipped with Xcode 27 (Swift ≥ 6.3) | Builds the `LalelaShim` static library via SwiftPM. |
| swift-format | toolchain default | Formats and lints `shim/`. |
| cargo-bundle | 0.12.x | Produces `Lalela.app` with `LSMinimumSystemVersion = 27.0`. |
| Version control | jujutsu (`jj`) on a Git backend | See [§Version control](#version-control-jujutsu). |
| Coding style | Tiger Style | See [§Tiger Style](#tiger-style--the-pervasive-style). |

---

## Tiger Style — the pervasive style

This project adopts **Tiger Style** as its pervasive coding style. It is the default, not a recommendation. A deviation needs a written reason in the change description.

The short form is **be defensive and validate everything**. Assume any input you did not produce is wrong: audio files, provider responses, saved sidecars, Keychain contents, Swift shim output. Assume any invariant you did not assert can be violated. Make every limit explicit, handle every error, and check every assumption.

The design priorities are **safety, performance, developer experience, in that order**. When they conflict, safety wins.

Load-bearing principles:

- **Zero technical debt.** Do it right the first time. The "second time" often does not arrive.
- **Simple, explicit control flow.** No recursion. No clever combinators that hide branches. Linear flow over chains that hide non-trivial control structure.
- **Limits on everything.** Every loop, channel, retry, buffer, and payload size has an explicit, declared upper bound. Every channel between the UI thread and the worker thread is a bounded `sync_channel`.
- **Assertions are first-class code.** They detect programmer errors, and the only correct response to a violated assertion is to crash. Aim for an average of at least two assertions per function: preconditions, postconditions, invariants.
- **Always say *why*.** Comments and commit descriptions explain the rationale, not the action.

---

## Defensive coding and assertions

### Where to validate

Validate at every boundary where data crosses from a place you do not control into one you do.

| Boundary | What to validate | How |
|---|---|---|
| User-chosen file → core | Extension allow-list, size, readability | `AudioSource::try_new` rejects before any provider sees the path |
| Adapter → core | Domain invariants on `Transcript` and `Segment` | `Transcript::from_draft` and `Transcript::from_parts` check ordering, bounds, and non-empty text |
| Core → adapter | Adapter contract | Assert the shape of what each port returns |
| Provider HTTP response | Status, content type, body size, body shape | Typed DTO decode, then conversion into domain types; unknown shapes are errors |
| Swift shim → Rust | Status code, output length vs. buffer capacity, JSON shape | Reject unknown status codes; never trust `out_len` beyond capacity |
| Sidecar read | Schema version, field shape, audio file presence | Re-validate on read, even though Lalela wrote it |
| Keychain read | UTF-8, length bounds | `ApiKey::try_new` |

### Assertions in Rust

- Use `assert!`, `debug_assert!`, and `assert_eq!` liberally in `lalela-core`. Release builds keep `assert!` enabled; invariants never rely on `debug_assert!` alone.
- **Average two or more assertions per function** in the core: preconditions on entry, postconditions on exit, invariants in the middle. `assert!(true)` does not count.
- **Pair assertions.** Enforce each property on two independent paths. Segment ordering is checked when a `Transcript` is constructed and again when the sidecar is decoded.
- **Assert positive and negative space**: what you expect, and what you do not.
- **Compile-time assertions** for size and layout invariants, including `const _: () = assert!(...)` on every limit relationship (for example, the sidecar cap holds a maximum-size transcript).
- **Split compound assertions.** `assert!(a); assert!(b);` over `assert!(a && b);`.
- **No `unwrap()` / `expect()` in production paths.** Tests and init-only code may use them; an init-time `expect()` carries a reason string.
- **No `panic!` for control flow.** Bad data from a provider, a file, or the shim is an error value, not a panic. Panics signal programmer error only.

### Assertions in Swift

- Use `precondition(_:_:)` for invariants; it stays active in `-O` builds. `assert(_:_:)` is reserved for checks that are too expensive for release.
- Every `@c` export asserts its pointer arguments are non-null and its capacities are non-zero before touching them.
- No force unwraps (`!`) or `try!` outside tests. Optional results map to a status code.
- No Swift error crosses the C ABI. Every thrown error maps to a `LalelaStatus` code at the export boundary.

### Errors are data, not exceptions

- Every error is a value with a typed reason. Each crate has one `Error` enum; adapters translate vendor errors (`ureq::Error`, `serde_json::Error`, `security_framework::base::Error`, shim status codes) into the port's error type with `From` impls. The core never sees a third-party error type.
- **Every error is handled or explicitly propagated.** Swallowing an error is a bug. The UI shows every terminal error in the status line.
- **Retry policies are explicit and bounded.** Max attempts, backoff schedule, and the retryable status set are named constants ([08-limits.md](08-limits.md)).
- **Never log a secret.** `ApiKey` implements `Debug` and `Display` as `ApiKey(<redacted>)`. Error values never carry request headers.

### Make invalid states unrepresentable

- Tagged newtypes for identifiers and units: `TranscriptId`, `Milliseconds`, `Bytes`, `ApiKey`, `LanguageTag`.
- Model state with enums, not strings or flag combinations: `ProviderId`, `AudioFormat`, `SessionState`, `LanguagePreference`. Match exhaustively with no wildcard arm on domain enums.
- Bounded collections: a `Transcript` holds at least one and at most `SEGMENTS_COUNT_MAX` segments.

---

## Limits and bounds

Every limit is **declared as a named constant**, named with its units last (`AUDIO_BYTES_MAX`, `HTTP_TIMEOUT_RECEIVE_MS`), and referenced everywhere it applies. No magic numbers.

Concrete values live in [08-limits.md](08-limits.md), not here. Each constant names the domain it bounds.

Reaching a limit is an **observable event**: it produces a typed error that the UI shows, and a structured log line. Reaching a hard limit rejects the input; it never silently truncates.

---

## Version control: jujutsu

### Shared core

- **Commits are small and well-described.** One coherent change per commit. Squash noise before pushing.
- **Empty commit descriptions are not accepted.** Describe the *why* before pushing.
- **Conventional Commits** for the subject line: `type(scope): subject`. Types: `feat`, `fix`, `docs`, `chore`, `refactor`, `test`, `build`, `ci`, `perf`, `style`. Scopes are crate names without the `lalela-` prefix (`core`, `voxtral`, `deepgram`, `macos`, `store`, `app`), plus `shim` and `specs`.
- **Branch model.** `main` stays releasable. Feature work happens on named bookmarks; pull requests target `main`.
- **Do not rewrite published history** unless the change is yours and unmerged.
- **Destructive operations need explicit confirmation**, even when they look like the cleanest path.

### jujutsu workflow

- **`jj` is the sole version-control front end.** Do not run `git commit`, `git add`, or `git status` against the jj working copy.
- **Describe before pushing.** `jj describe` sets the *why*; an empty description blocks the push.
- **Feature work happens on named bookmarks** (`jj bookmark create feat/x`); pull requests are pushed with `jj git push`.
- **Resolve conflicts in jj** (`jj resolve`), not by editing plain-text markers.
- **Destructive `jj` operations need explicit confirmation**: `jj abandon`, `jj op restore`, force-fetches, bookmark deletion.
- The `.jj/` directory is local and not committed.

---

## Rust conventions

### Formatting and linting

- `cargo fmt --all` clean before commit.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean before commit.
- The workspace `Cargo.toml` sets `[workspace.lints]`: `unsafe_code = "deny"` at the workspace level, `clippy::pedantic` at warn, and `clippy::unwrap_used`, `clippy::expect_used`, `clippy::panic` at deny for non-test code. Each opt-out carries a comment explaining why.
- Only `lalela-macos` allows `unsafe_code`, because it is the FFI boundary.

### Code style

- **Modules over files.** Many small files; a 1000-line `.rs` file is a smell.
- **No business logic in `main.rs` or in egui view code.** Views read state and emit `UiIntent` values; the session controller calls core use cases.
- **Hard limit: 70 lines per function.** Push `if`s up and `for`s down.
- **Hard limit: 100 columns per line**, enforced by `rustfmt`.
- **No recursion.** Iterate with an explicit bound.
- **Explicit fixed-width integers** (`u32`, `u64`) for domain values. No `usize` in the sidecar or the shim ABI.
- **Errors are `Result`** with one `Error` enum per crate, derived with `thiserror`.
- **Every `unsafe` block carries a `// SAFETY:` comment** that justifies each invariant it relies on.
- **`#[must_use]`** on `Result`-returning functions and builders.
- **Simpler return types win:** `()` > `bool` > integer > `Option<T>` > `Result<T, E>`. Prefer an explicit `match` over `.map().and_then().ok_or()` chains when the control flow is non-trivial.
- **Pass large structs by reference** (over 16 bytes and not moved → `&T`).
- **Calculate variables close to their use.** No dead bindings, no aliasing.
- **State invariants positively:** `if index < length`, not `if index >= length`.
- **Comments explain *why*,** in full sentences.

### Naming

- `snake_case` for functions, variables, modules, and files; `CamelCase` for types and traits.
- **Acronyms in proper case:** `HttpClient`, `ApiKey`, `JsonSidecar`.
- **No abbreviations** beyond ecosystem-standard short names (`ctx`, `cfg`, `id`, loop counters).
- **Units last, descending significance:** `duration_ms`, `timeout_receive_ms`, `segments_count_max`.
- **Same-length names for related variables** where reasonable: `source` / `target`.
- **Helpers prefix with the parent name**; callbacks go last in parameter lists.

### Testing

- `cargo nextest run` is how tests run.
- **Test pyramid.**
  - Unit tests in-module, mostly in `lalela-core`.
  - Integration tests in each crate's `tests/`. Core use cases run against in-memory fakes of every port. Provider adapters decode recorded response fixtures in `tests/fixtures/`.
  - Live tests hit real providers or the real Swift shim. They are marked `#[ignore]`, read keys from the environment, and run explicitly.
- **Parsing is separate from transport.** Each HTTP adapter exposes a pure `decode_response(&[u8]) -> Result<TranscriptDraft, TranscribeError>` that fixtures test without a network.
- **Positive and negative space.** Every validation path has an accept test and a reject test.
- **Test the validity boundary**: one below a limit, at the limit, one above.
- **Property tests** (`proptest`) for `segment_at_position` and for sidecar encode/decode round-trips.
- **Determinism.** Core tests use the `Clock` and `IdSource` fakes; no `SystemTime::now()` or randomness in test bodies.
- **No flaky tests.** A flaky test is a bug to fix immediately.

### Documentation

- Public items in library crates carry doc comments.
- Each crate's `lib.rs` documents what the crate is, the ports it implements or depends on, and the surface it offers.
- No bare `// TODO` without an owner and a tracking reference.

---

## Swift conventions

The Swift code is the `LalelaShim` package under `shim/`. It is a translation layer between the C ABI and Apple frameworks. It holds no domain logic.

### Formatting and linting

- `swift format lint --strict --recursive shim/` clean before commit.
- Swift 6 language mode with complete strict concurrency checking.

### Code style

- **Every export uses `@c`** (SE-0495) with a `lalela_` prefix, and is declared in `shim/include/lalela_shim.h`. That header is the single source of truth for the ABI that `lalela-macos` mirrors.
- **Only C-representable types cross the boundary:** fixed-width integers, `UnsafePointer<CChar>` for UTF-8 paths, caller-owned output buffers with explicit capacity, and `@convention(c)` callbacks with an opaque context pointer.
- **No heap ownership crosses the boundary.** The shim never returns memory the caller must free. Output goes into caller-owned buffers sized by named constants.
- **Async work blocks the calling worker thread** on a `DispatchSemaphore` with a bounded timeout. The shim never blocks the main thread.
- **Hard limit: 70 lines per function, 100 columns per line**, the same as Rust.

### Naming

- Swift API Design Guidelines for internal code. C exports use `lalela_<area>_<verb>` (`lalela_speech_transcribe`, `lalela_player_seek`).
- Status codes are a single `enum LalelaStatus: Int32` mirrored by `ShimStatus` in Rust.

### Testing

- The shim is exercised through the Rust live tests in `lalela-macos/tests/`, using bundled fixture audio. The shim has no separate Swift test target.

---

## Repository hygiene

- **`.specs/`** is the canonical home for specs and decisions.
- **Secrets never live in the repository.** API keys come from the Keychain or from `MISTRAL_API_KEY` / `DEEPGRAM_API_KEY` in the developer's shell. `.env` files are gitignored.
- **Build output** (`target/`, `shim/.build/`) is gitignored.
- **Test fixtures** under `crates/*/tests/fixtures/` are small. Each audio fixture is under 1 MiB and carries a `SOURCE.md` naming its licence. Recorded provider responses have request IDs and any account data removed.
- **The pre-push hook** runs `cargo fmt --check`, `cargo clippy`, `swift format lint`, and `cargo nextest run`. CI runs the same on a macOS 27 Apple-silicon runner.

---

## Guidelines for AI agents

1. **The pervasive style applies to you too.** Defensive validation and explicit limits are not optional, even on a small change.
2. **Add assertions as you go.** Every function you touch leaves with at least two meaningful assertions.
3. **No silent error swallowing.** Every error is handled. Every match on a domain enum is exhaustive, with no `_ =>` arm.
4. **Stay inside the architecture.** Do not add file, network, Keychain, or FFI calls to `lalela-core`. Define a port, implement an adapter, wire it in `lalela-app`.
5. **Do not add backwards-compat shims.** If a type changes, change every caller. The sidecar's `schema_version` is the only compatibility surface.
6. **Do not invent fields** that are not in [canonical-types.schema.json](canonical-types.schema.json). Update the schema first.
7. **Tests run before claiming complete.** "Compiles" is not "works." Run `cargo nextest run` and report the actual output.
8. **Test positive and negative space together.**
9. **Limits are explicit.** A new loop, channel, retry, or buffer ships with a named constant in the same change, and a row in [08-limits.md](08-limits.md).
10. **Prefer small, frequent commits.**
11. **No comments that paraphrase the code.**
12. **Do not run destructive `jj` operations without explicit confirmation.**
13. **Do not skip hooks.** If a hook fails, fix the underlying issue.
14. **Rust-specific slip:** do not call `rfd` dialogs or the shim player from the worker thread, or call the shim's blocking speech export from the UI thread. [06-app-shell.md](06-app-shell.md) gives the thread ownership.
15. **Provider-specific slip:** verify provider request parameters against the provider's current docs before changing an adapter. Model ids and parameter compatibility change between model versions.

---

## Definition of done

A change is done when:

- The behaviour is exercised by a test (unit, integration, or live as appropriate).
- The change includes **negative-space tests** for every new validation path.
- Every new or touched function has at least two meaningful assertions.
- Every new bound is a named constant listed in [08-limits.md](08-limits.md).
- `cargo fmt --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `swift format lint --strict --recursive shim/`, and `cargo nextest run` all pass locally.
- If domain or sidecar types changed, [canonical-types.schema.json](canonical-types.schema.json) is updated in the same change.
- If the shim ABI changed, `lalela_shim.h` and the Rust mirror in `lalela-macos` changed together.
- The commit description states the *why*.
- The change description lists what changed at the architecture level.

---

## Assumptions and open questions

**Assumptions**

- The repository is greenfield. No toolchain file, hook, or CI workflow exists yet. The toolchain table and hygiene rules are adopted policy, and the setup work is tracked under Open questions.
- Developers build on Apple-silicon Macs running macOS 27 with Xcode 27 installed.
- Swift 6.3 or later is available, so `@c` exports are supported.

**Decisions**

- *Coding style.* **Tiger Style.** The app parses untrusted provider responses and crosses an FFI boundary. Assertion-heavy, bounded code catches those faults where they enter.
- *Test runner.* **cargo-nextest.** Per-test process isolation keeps the FFI-backed live tests from contaminating each other. `cargo test --doc` covers doctests.
- *Parsing separate from transport.* **Pure `decode_response` functions tested with fixtures.** This removes the need for an HTTP mock-server dependency and keeps tests deterministic.
- *Swift export attribute.* **`@c` over `@_cdecl`.** `@c` is the official attribute from Swift 6.3; `@_cdecl` is underscored and unsupported.
- *Heavy gate placement.* **Pre-push, not pre-commit.** It keeps `jj` commits cheap while making sure nothing unchecked reaches the remote.
- *Version control.* **jujutsu on a Git backend.** The owner chose it, and the Git backend keeps hosting on standard Git forges.

**Open questions**

- *Hooks and CI.* No pre-push hook or CI workflow exists yet. Which hook manager should wire the pre-push gate under jj, and which CI provider has macOS 27 Apple-silicon runners?
- *Swift section provenance.* The Swift conventions were written by analogy with the Rust Tiger Style overlay; there is no Swift template. Should they be reviewed against an established Swift style guide?
- *UI tests.* Should egui view logic get snapshot tests (for example with `egui_kittest`), or is controller-level testing of `UiIntent` handling enough?
