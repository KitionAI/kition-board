# Roadmap

> **Last updated:** 2026-11-21 · **Source:** `features.yaml` (updated 2026-11-21) and the engine's command list

Open work on Kition Board, in order. Each item names where its detail lives.

1. **Finish the P0 features against the benchmark, Miro:** `board.editing`: every function is built; it becomes `has` once its change-shape and marks cases pass a CI run. Detail in [gaps.md](gaps.md).
2. **Prove the partial P0 features end to end:** `board.agent.guard`: each needs a case that shows it working in the editor and surviving save and reopen before it is `has`. Detail in [gaps.md](gaps.md).
3. **Then the P1 features:** `board.agent.environment`, `board.cards`, `board.collab.live`; `board.export` and `board.history` become `has` once their editor cases (web/tests/proof.mjs) pass a CI run.
4. **Then the P2 features:** `board.templates` becomes `has` once its editor case (web/tests/proof.mjs) passes a CI run.
5. **Keep the catalogue complete:** every new engine command joins a feature in `features.yaml` with a case.
