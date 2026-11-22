# Roadmap

> **Last updated:** 2026-11-21 · **Source:** `features.yaml` (updated 2026-11-21) and the engine's command list

Open work on Kition Board, in order. Each item names where its detail lives.

1. **Build the missing P0 features against the benchmark, Miro:** `board.editing`, and the rest of the partial `board.connectors.ends` (bound ends that meet the target's outline): each becomes `has` with its gating tests. Detail in [gaps.md](gaps.md).
2. **Prove the partial P0 features end to end:** `board.agent.guard`, `board.shapes`: each needs a case that shows it working in the editor and surviving save and reopen before it is `has`. Detail in [gaps.md](gaps.md).
3. **Then the P1 features:** `board.agent.environment`, `board.cards`, `board.collab.live`, `board.export`, `board.history`.
4. **Then the P2 features:** `board.templates`.
5. **Keep the catalogue complete:** every new engine command joins a feature in `features.yaml` with a case.
