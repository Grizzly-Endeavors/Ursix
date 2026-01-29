# Writing Effective Rules

This guide covers best practices for writing rules that LLMs can apply accurately and consistently.

## Core Principles

### 1. Rules Must Be Single-File Detectable

Rules are applied per-file without cross-file context. Write rules that focus on patterns detectable within a single file.

**Good**: "Flag `unwrap()` calls outside of test modules"
**Bad**: "Ensure all errors are properly propagated to callers" (requires seeing call sites)

### 2. Avoid Examples in Rule Descriptions

LLMs may hallucinate examples from rule descriptions into the actual code findings. If you write "Flag comments like `// increment counter`", the LLM may report that exact comment even if it doesn't exist.

**Good**: "Flag comments that restate obvious code behavior"
**Bad**: "Flag comments like `// increment counter` above `counter += 1`"

### 3. Be Specific About What NOT to Flag

Explicitly list exceptions to reduce false positives. LLMs tend to over-apply rules unless told otherwise.

**Good**: "Flag bare `unwrap()` calls. EXCEPTION: `unwrap()` is allowed inside `#[cfg(test)]` modules"
**Bad**: "Flag bare `unwrap()` calls"

### 4. Use Positive Patterns, Not Absence Detection

LLMs struggle to reliably detect the *absence* of something (missing doc comments, missing error handling). They may hallucinate that something is missing when it exists.

**Good**: "Flag functions that use `panic!()` macro"
**Bad**: "Flag public functions without doc comments"

For absence detection, prefer static analysis tools (clippy, eslint) over LLM rules.

### 5. Scope Rules Appropriately

Use the `files` field to limit rules to relevant file patterns. This reduces noise and improves accuracy.

```yaml
- name: bounded-concurrency
  description: "Flag unbounded join_all() on user-controlled collections"
  files: "src/**/*.rs"  # Only check source files, not tests
```

### 6. Distinguish "Correct Patterns" from "Incorrect Patterns"

When a rule has both correct and incorrect variations, explicitly describe both so the LLM knows what to flag vs. what to ignore.

**Good**: "Flag unbounded `join_all()`. Do NOT flag `buffer_unordered(n)` or `buffered(n)` - these are correct bounded patterns."

### 7. Test Module Exceptions

Test code often has different standards (using `unwrap()`, simpler error handling, descriptive comments). Consider exempting test modules from style rules.

```yaml
- name: why-not-what-comments
  description: "... Do NOT flag comments in #[cfg(test)] modules (test comments often serve as spec documentation)."
```

## Rule Categories That Work Well

### Structural Patterns
- Missing attributes (`#[must_use]`, `#[derive(...)]`)
- Incorrect macro usage
- Naming convention violations

### Anti-Patterns
- Specific function calls to avoid (`unwrap()`, `expect()`, blocking calls in async)
- Deprecated API usage
- Known problematic patterns

### Style Consistency
- Logging format requirements
- Error message format
- Import organization

## Rule Categories to Avoid

### Cross-File Concerns
- "Ensure consistent error handling across modules"
- "Check that all public APIs have integration tests"

### Absence Detection
- "Missing documentation"
- "Missing error handling"
- "Missing validation"

### Subjective Judgment
- "Code should be well-organized"
- "Functions should be appropriately sized"

## Severity Guidelines

| Severity | Use For |
|----------|---------|
| `error` | Bugs, security issues, broken functionality |
| `warning` | Potential problems, code smells, maintainability concerns |
| `info` | Style suggestions, minor improvements |

Use `error` sparingly - if everything is an error, nothing is.

## Debugging False Positives

If a rule produces false positives:

1. **Check for examples in the description** - Remove them
2. **Add explicit exceptions** - List what NOT to flag
3. **Narrow the file scope** - Use `files` to target specific paths
4. **Split into multiple rules** - Simpler rules are more accurate
5. **Consider removing it** - Some things are better checked by static analysis

## Example: Evolution of a Rule

Initial (problematic):
```yaml
- name: no-unwrap
  description: "Don't use unwrap()"
```

Better (with exceptions):
```yaml
- name: no-unwrap
  description: "Flag bare unwrap() calls. EXCEPTION: allowed in #[cfg(test)] modules."
```

Best (specific and scoped):
```yaml
- name: no-unwrap-in-production
  description: "Flag unwrap() and expect() calls that could panic in production code. Do NOT flag: (1) calls inside #[cfg(test)] modules, (2) calls with explanatory comments justifying why panic is acceptable, (3) calls on values that are compile-time guaranteed (e.g., static regex)."
  severity: warning
  files: "src/**/*.rs"
```
