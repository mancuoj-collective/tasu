# AGENTS.md

- NEVER write unit tests after you write code.
- Highly prefer E2E tests as the sole testing mechanism. Use them to verify complex features work. At the end of E2E tests, produce a verifiable and repeatable artifact.
- If you must test a system in isolation, FIRST write all the ways it could fail, THEN write the code.
- Before implementing any feature, first find how the authoritative projects in `../references` already do it, and follow that pattern. Treat those implementations as the source of truth instead of inventing your own approach.
- Do not add useless comments. Only comment when it is genuinely necessary.
