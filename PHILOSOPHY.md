# Ursix Philosophy

## What Ursix Is

Ursix is a Unix-style agent harness. It exposes LLM capabilities as CLI commands that behave like any other Unix tool—structured input, structured output, predictable behavior, composable via pipes and scripts.

**Commands are functions, not conversations.**

Each command does one thing. It takes explicit input, produces structured output, and exits with a meaningful status code. There's no hidden state between invocations, no interactive back-and-forth, no ambient context that changes behavior unpredictably.

```bash
# This is Ursix
usx review --checks=style > issues.json
usx fix --from=issues.json
usx commit

# This is not Ursix
usx "hey can you look at my code and maybe fix some stuff and also write a commit message"
```

## What Ursix Isn't

**Ursix is not an interactive assistant.** If you want to have a conversation with an LLM, use a chat interface. Ursix is for automation.

**Ursix is not a coding agent.** Tools like Claude Code, Cursor, and Aider are designed for open-ended development tasks with human oversight. Ursix is for bounded, repeatable operations that run unattended.

**Ursix is not a SaaS product.** There's no cloud dependency, no GitHub App, no web dashboard. It's a binary that runs on your machine and calls whatever LLM backend you configure.

## Core Principles

### Automation-First

Every command must be scriptable. If it can't be used in a bash script or CI pipeline without human intervention, it doesn't belong in Ursix.

This means:
- Structured JSON output by default (`--text` for human-readable)
- Semantic exit codes (0 = success, 1 = issues found, 2 = error)
- No interactive prompts unless explicitly requested
- Deterministic behavior given the same inputs

### Isolated Agents

When a command spins up multiple agents (e.g., parallel checks), they are fully isolated. No shared context, no message passing, no collaboration.

This is intentional:
- **Atomic**: Each agent's success or failure is independent
- **Auditable**: You can inspect exactly what each agent saw and produced
- **Tuneable**: You can use different models for different agents without side effects

The cost of duplicated work (e.g., multiple agents reading the same file) is negligible compared to the complexity cost of coordination.

### Bring Your Own Model

Ursix doesn't care where inference happens. Local models via Ollama, cloud APIs via Anthropic/OpenAI, or your own self-hosted endpoint. The tool is model-agnostic.

This also means Ursix is optimized for *narrow, well-prompted tasks* where smaller models excel. You don't need GPT-4 to check if there are too many comments. The command structure lets you match model capability to task complexity.

### Composability Over Features

Rather than building every workflow into the tool, Ursix provides primitives that compose with standard Unix tools:

```bash
# Iteration via shell
while ! usx review --checks=style; do
    usx fix --from-last-review
done

# Filtering via jq
usx review | jq '.issues[] | select(.severity == "error")'

# Parallel execution via xargs
find . -name "*.rs" | xargs -P4 -I{} usx explain {}
```

If you find yourself wanting a complex built-in workflow, consider whether it could be a shell script instead.

## The Problem Ursix Solves

AI coding agents are powerful but error-prone. They find loopholes in linter rules, sneak in `// eslint-disable` without justification, and produce code that passes CI but violates team conventions.

Traditional code review catches these issues, but it requires humans. CI catches objective violations, but not subjective ones.

The gap is: **reviewers need to be automated, but automation tools assume human-in-the-loop.**

Existing AI review tools are built for GitHub comment threads and web UIs. They can't be called from a script, can't produce machine-parseable output, can't be composed into iteration loops.

Ursix fills this gap by treating AI review (and AI-powered fixes) as Unix commands. The iteration loop becomes trivial:

```bash
for i in {1..3}; do
    usx review --checks=style,security > /tmp/review.json
    [ $(jq '.issues | length' /tmp/review.json) -eq 0 ] && break
    usx fix --from=/tmp/review.json
done
```

No magic, no hidden state, trivially debuggable.

## Who Ursix Is For

- Developers who want to integrate AI into shell scripts and CI pipelines
- Teams using AI coding agents who need automated review/fix loops
- Anyone who's frustrated that AI tools are chat-first instead of automation-first

## Who Ursix Is Not For

- People who want an interactive coding assistant
- Teams looking for a turnkey SaaS code review solution
- Anyone who needs AI tools to "just work" without configuration

Ursix requires you to think about prompts, models, and workflows. It gives you control in exchange for effort.
