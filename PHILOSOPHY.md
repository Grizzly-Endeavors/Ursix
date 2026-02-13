# Ursix Philosophy

## The Core Idea

Ursix treats LLMs as compute primitives: text in, structured output, done.

Not a chatty assistant. Not a magic developer. Just a function that transforms text, like `sed` or `jq`, but with semantic understanding.

## What Ursix Is

### A Semantic Linter

At its core, Ursix is a semantic linter — it catches issues that traditional linters can't: vague naming, poor error messages, missing edge cases, hardcoded secrets, style violations that require understanding intent. The `review` command is the flagship, and everything else (`derive`, `fix`, `status`) supports the semantic linting workflow.

### A Link in a Chain

Ursix is one step in your pipeline. It doesn't know what came before, doesn't care what comes after, and won't try to orchestrate a workflow for you.

```bash
# Ursix is the middle step—you control the rest
git diff --staged | usx review | jq '.issues[] | select(.severity == "error")'
```

The iteration loop? That's your script:

```bash
for i in {1..3}; do
    usx review > /tmp/review.json
    [ $(jq '.issues | length' /tmp/review.json) -eq 0 ] && break
    # Handle issues however you want
done
```

### Stateless

Each command runs, does a thing, and exits. No conversation history. No memory between invocations. No ambient context that changes behavior.

This means:
- **Atomic**: Each invocation's success or failure is independent
- **Auditable**: You can inspect exactly what the LLM received and produced
- **Predictable**: Same input produces consistent output

### Contract-Based

Commands are defined by their input/output contracts:

| Command | Input | Output | Exit Codes | Status |
|---------|-------|--------|------------|--------|
| `review` | diff or code file | issues array + passed | 0, 1, 2, 3, 4 | Stable |
| `derive` | arbitrary text | text (mode-dependent) | 0, 2, 3, 4 | Stable |
| `fix` | structured JSON | unified diff | 0, 2, 3, 4 | Experimental |
| `config` | none | configuration values | 0, 2 | Stable |

`review` earns its own command because the contract is different—it outputs structured issues, returns exit code 1 when issues are found, and integrates with rules.yml. `derive` is the generic transform.

## What Ursix Isn't

### Not an Interactive Assistant

If you want to have a conversation with an LLM, use a chat interface. Ursix is for automation.

### Not a Coding Agent

Tools like Claude Code, Cursor, and Aider are designed for open-ended development with human oversight. They gather context, iterate on solutions, and execute actions. Ursix does none of these—it's for bounded, repeatable operations that run unattended.

### Not a SaaS Product

No cloud dependency, no GitHub App, no web dashboard. Just a binary that calls whatever LLM backend you configure.

### Not Magic

**Ursix won't assume you want anything.**

- If you don't provide a file argument, it won't look for one
- If you don't tell it to chunk, it won't chunk
- If you don't specify rules, it uses sensible defaults
- If you don't pipe input, it waits on stdin

Explicit over implicit, always.

## Design Principles

### No Magic, No Batteries

Every behavior is opt-in. Ursix does exactly what you ask—nothing more, nothing less.

This means:
- **No context gathering**: Won't read files you didn't provide
- **No tool execution**: Won't run commands or modify your system
- **No feature auto-detection**: Won't enable chunking based on input size
- **No helpful guessing**: Won't infer what you probably meant

If something fails because you forgot a flag, that's feedback—not a bug.

### Structured Output Guarantee

Despite LLMs under the hood, output is always valid JSON (or explicit error JSON). Exit codes are always meaningful. Scripts can rely on output parsing.

```bash
# This will never break due to malformed JSON
usx review | jq '.passed'
```

If an error occurs, you get structured error JSON—not a stack trace mixed with partial output.

### Automation-First

CI/CD is the primary use case. Every design decision asks: "Does this work in a script? Can it be run unattended?"

This means:
- **Exit codes that stop builds**: 0 = success, 1 = issues found, 2+ = errors
- **No interactive prompts** unless explicitly requested
- **Deterministic(ish) behavior** given the same inputs
- **Retry logic** for transient failures (network, rate limits)

### Composability Over Features

Rather than building every workflow into the tool, Ursix provides primitives that compose with standard Unix tools:

```bash
# Filtering via jq
git diff | usx review | jq '.issues[] | select(.severity == "error")'

# Parallel execution via xargs
find . -name "*.rs" | xargs -P4 -I{} sh -c 'cat {} | usx derive explanation'

# Conditional logic via shell
usx review && echo "Clean" || echo "Issues found"
```

If you find yourself wanting a complex built-in workflow, consider whether it could be a shell script instead.

## The JSON Guarantee

LLMs can be unreliable for producing structured output. Ursix will throw a parsable error if it can't recover, *never malformed JSON*.

- Output is always valid JSON matching a documented schema
- Exit 0 = complete, valid result JSON
- Exit non-zero = valid error JSON (never malformed output)
- Partial failures return error JSON with `partial_results` for recovery

Your scripts will never crash parsing Ursix output.

## Who Ursix Is For

- Developers integrating LLMs into shell scripts and CI pipelines
- Teams using AI coding agents who need automated review/fix loops
- Anyone frustrated that AI tools are chat-first instead of automation-first

## Who Ursix Is Not For

- People who want an interactive coding assistant
- Teams looking for a turnkey SaaS code review solution
- Anyone who expects AI tools to "just work" without configuration

Ursix requires you to think about prompts, models, and workflows. It gives you control in exchange for effort.
