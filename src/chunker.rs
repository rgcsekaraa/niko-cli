use std::time::Instant;

use anyhow::Result;

use crate::llm::{self, Provider};

/// Maximum lines per chunk for LLM processing
const MAX_CHUNK_LINES: usize = 200;
/// Context overlap — carry last N lines from previous chunk for boundary continuity
const CONTEXT_OVERLAP_LINES: usize = 10;
/// Max tokens for chunk analysis
const CHUNK_MAX_TOKENS: u32 = 4096;
/// Max tokens for synthesis step
const SYNTHESIS_MAX_TOKENS: u32 = 4096;
/// Max tokens for follow-up questions
const FOLLOWUP_MAX_TOKENS: u32 = 2048;

/// A chunk of code with its line range
#[derive(Debug, Clone)]
pub struct CodeChunk {
    pub start_line: usize,
    pub end_line: usize,
    pub content: String,
    pub context_prefix: String,
}

/// Result of explaining a single chunk
#[derive(Debug, Clone)]
pub struct ChunkExplanation {
    pub start_line: usize,
    pub end_line: usize,
    pub explanation: String,
}

/// Full explanation result
#[derive(Debug, Clone)]
pub struct ExplainResult {
    pub total_lines: usize,
    pub total_chunks: usize,
    pub chunk_explanations: Vec<ChunkExplanation>,
    pub overall_summary: String,
    pub follow_up_questions: Vec<String>,
    pub elapsed: std::time::Duration,
}

/// Split code into logical chunks with overlap for boundary continuity.
pub fn chunk_code(code: &str) -> Vec<CodeChunk> {
    let lines: Vec<&str> = code.lines().collect();
    let total = lines.len();

    if total == 0 {
        return vec![];
    }

    if total <= MAX_CHUNK_LINES {
        return vec![CodeChunk {
            start_line: 1,
            end_line: total,
            content: code.to_string(),
            context_prefix: String::new(),
        }];
    }

    let mut chunks = Vec::new();
    let mut start = 0;

    while start < total {
        let mut end = (start + MAX_CHUNK_LINES).min(total);

        if end < total {
            let search_start = if end > 30 { end - 30 } else { start };
            let mut best_break = end;

            for i in (search_start..end).rev() {
                let line = lines[i].trim();
                if line.is_empty() {
                    best_break = i + 1;
                    break;
                }
                if line == "}" || line == "};" || line == "end" {
                    best_break = i + 1;
                    break;
                }
                if is_definition_start(line) {
                    best_break = i;
                    break;
                }
            }
            end = best_break;
        }

        if end <= start {
            end = (start + MAX_CHUNK_LINES).min(total);
        }

        let context_prefix = if !chunks.is_empty() {
            let ctx_start = start.saturating_sub(CONTEXT_OVERLAP_LINES);
            let ctx_lines = &lines[ctx_start..start];
            format!(
                "// [context: preceding lines {}-{} shown for continuity]\n{}\n// [chunk starts here]\n",
                ctx_start + 1, start, ctx_lines.join("\n")
            )
        } else {
            String::new()
        };

        chunks.push(CodeChunk {
            start_line: start + 1,
            end_line: end,
            content: lines[start..end].join("\n"),
            context_prefix,
        });
        start = end;
    }

    chunks
}

fn is_definition_start(line: &str) -> bool {
    let prefixes = [
        "fn ",
        "pub fn ",
        "pub(crate) fn ",
        "pub(super) fn ",
        "async fn ",
        "pub async fn ",
        "def ",
        "class ",
        "function ",
        "func ",
        "const ",
        "let ",
        "var ",
        "static ",
        "type ",
        "struct ",
        "impl ",
        "trait ",
        "interface ",
        "package ",
        "module ",
        "export ",
        "import ",
        "#[",
        "@",
    ];
    prefixes.iter().any(|p| line.starts_with(p))
}

/// Process all chunks through the LLM with streaming and retry.
///
/// If `stream_callback` is provided, tokens are passed to it as they arrive.
/// The synthesis step always uses non-streaming with retry since we need the full response.
pub fn explain_code<F>(
    code: &str,
    provider: &dyn Provider,
    verbose: bool,
    mut stream_callback: Option<F>,
) -> Result<ExplainResult>
where
    F: FnMut(&str),
{
    let start_time = Instant::now();
    let chunks = chunk_code(code);
    let total_lines = code.lines().count();
    let total_chunks = chunks.len();

    if verbose {
        eprintln!(
            "  [debug] {} lines → {} chunks (max {} lines/chunk, {} line overlap, stream={})",
            total_lines,
            total_chunks,
            MAX_CHUNK_LINES,
            CONTEXT_OVERLAP_LINES,
            stream_callback.is_some()
        );
    }

    let mut chunk_explanations = Vec::new();

    for (i, chunk) in chunks.iter().enumerate() {
        if total_chunks > 1 {
            // TODO: If we have a callback, maybe we should report progress too?
            // For now, assume the caller handles progress indication or we print it if verbose.
            if verbose {
                eprintln!(
                    "  [debug] Chunk {}/{} (lines {}–{})…",
                    i + 1,
                    total_chunks,
                    chunk.start_line,
                    chunk.end_line
                );
            }
        }

        let system_prompt = build_chunk_system_prompt(i + 1, total_chunks);

        let user_prompt = if chunk.context_prefix.is_empty() {
            format!(
                "Lines {}-{} ({} lines):\n\n```\n{}\n```",
                chunk.start_line,
                chunk.end_line,
                chunk.end_line - chunk.start_line + 1,
                chunk.content
            )
        } else {
            format!(
                "{}\nLines {}-{} ({} lines):\n\n```\n{}\n```",
                chunk.context_prefix,
                chunk.start_line,
                chunk.end_line,
                chunk.end_line - chunk.start_line + 1,
                chunk.content
            )
        };

        let explanation = if let Some(ref mut callback) = stream_callback {
            // Stream tokens to callback
            llm::generate_streaming(
                provider,
                &system_prompt,
                &user_prompt,
                CHUNK_MAX_TOKENS,
                callback,
            )?
        } else {
            llm::generate_with_retry(provider, &system_prompt, &user_prompt, CHUNK_MAX_TOKENS)?
        };

        let sanitized_explanation = explanation.replace('—', "-");

        chunk_explanations.push(ChunkExplanation {
            start_line: chunk.start_line,
            end_line: chunk.end_line,
            explanation: sanitized_explanation,
        });
    }

    // Synthesis — always non-streaming with retry (needs full response for parsing)
    // We could stream status updates if we had a status callback, but for now we just run it.
    let (mut summary, mut questions) = if total_chunks > 1 {
        generate_summary_and_questions(provider, &chunk_explanations, total_lines)?
    } else {
        let explanation = &chunk_explanations[0].explanation;
        let questions = generate_follow_up_only(provider, explanation).unwrap_or_default();
        (explanation.clone(), questions)
    };

    // Sanitize summary and questions to prevent any UI rendering panics on em-dashes
    summary = summary.replace('—', "-");
    questions = questions.into_iter().map(|q| q.replace('—', "-")).collect();

    Ok(ExplainResult {
        total_lines,
        total_chunks,
        chunk_explanations,
        overall_summary: summary,
        follow_up_questions: questions,
        elapsed: start_time.elapsed(),
    })
}

fn build_chunk_system_prompt(chunk_num: usize, total_chunks: usize) -> String {
    if total_chunks == 1 {
        return r#"You are a senior software engineer conducting a thorough code review. Analyze the code and produce a structured explanation in markdown.

## Overview
One paragraph: what does this code do, what problem does it solve, and what is its role in a larger system.

## Architecture & Design Patterns
- Identify design patterns (builder, factory, observer, strategy, etc.)
- Note architectural decisions (layering, separation of concerns, dependency injection)
- Call out any anti-patterns or code smells

## Detailed Walkthrough
For EVERY function, method, struct, enum, trait, and significant constant:

### `function_name(params) -> ReturnType`
- **Purpose**: What it does and why
- **Parameters**: Each param with its type and role
- **Returns**: What the return value represents
- **Key logic**: Algorithm, branching, error handling
- **Edge cases**: Boundary conditions, nil/empty checks, overflow risks

## Error Handling
- How errors are created, propagated, and recovered from
- Missing error handling that should exist

## Dependencies & Imports
- External crates/packages and what they provide
- Standard library usage patterns

## Potential Issues
- Performance concerns (N+1 queries, unnecessary allocations, blocking calls)
- Security considerations (input validation, injection risks, secret handling)
- Concurrency issues (race conditions, deadlocks, shared mutable state)
- Missing edge cases or insufficient validation

Be thorough — do NOT skip any function or type definition. Use code-aware language (refer to actual names from the code)."#.to_string();
    }

    format!(
        r#"You are a senior software engineer analyzing chunk {chunk_num} of {total_chunks} of a larger codebase.

Some preceding lines may be included for boundary context — focus your analysis on the code after the "[chunk starts here]" marker. Do NOT re-explain the context lines.

For this chunk, provide:

## Summary
What this chunk does, what components it defines, and how it likely connects to the rest of the codebase.

## Detailed Walkthrough
For EVERY function, method, struct, enum, trait, and constant in this chunk:

### `name(params) -> ReturnType`
- **Purpose**: What and why
- **Parameters**: Each param, type, role
- **Returns**: What the value represents
- **Key logic**: Core algorithm, branching, errors
- **Edge cases**: Boundary conditions, potential failures

## Cross-References
- Functions or types called/used that are likely defined in other chunks
- Dependencies on external types or traits

## Potential Issues
- Performance, security, or correctness concerns specific to this chunk

Be thorough — capture EVERY definition. Do not omit anything. Use actual names from the code."#
    )
}

fn generate_summary_and_questions(
    provider: &dyn Provider,
    explanations: &[ChunkExplanation],
    total_lines: usize,
) -> Result<(String, Vec<String>)> {
    let combined = explanations
        .iter()
        .map(|e| {
            format!(
                "### Lines {}-{}\n{}",
                e.start_line, e.end_line, e.explanation
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n---\n\n");

    let system_prompt = r#"You are a senior software architect synthesizing a multi-chunk code analysis. You have been given individual analyses of each chunk — now produce a unified view.

Your response MUST use this exact format:

## Summary
2-3 paragraphs covering:
- What the entire codebase does (purpose, domain, user-facing behavior)
- Architecture: how the chunks connect (data flow, call chains, dependency graph)
- Key design decisions: patterns used, tradeoffs made, and why they matter
- Quality assessment: code health, consistency, and notable strengths or weaknesses

## Key Components
Bullet list of the most important types/functions and their roles across the codebase.

## Follow-up Questions
5 targeted questions, one from each category:
1. [Architecture] — about structure, modularity, or coupling
2. [Testing] — about test coverage, edge cases, or testability
3. [Security] — about input validation, secrets, or access control
4. [Performance] — about bottlenecks, scaling, or resource usage
5. [Maintainability] — about readability, tech debt, or extensibility

Do NOT just summarize each chunk sequentially — synthesize across chunks to show the bigger picture."#;

    let user_prompt = format!(
        "The codebase has {} total lines across {} chunks:\n\n{}",
        total_lines,
        explanations.len(),
        combined
    );

    let response =
        llm::generate_with_retry(provider, system_prompt, &user_prompt, SYNTHESIS_MAX_TOKENS)?;
    let (summary, questions) = parse_summary_response(&response);
    Ok((summary, questions))
}

fn generate_follow_up_only(provider: &dyn Provider, explanation: &str) -> Result<Vec<String>> {
    let system_prompt = r#"Based on the code analysis below, generate exactly 5 follow-up questions that would help someone deeply understand and improve this code. One question from each category:

1. [Architecture] — How the code is structured, modularity, coupling, or design patterns
2. [Testing] — Test coverage gaps, edge cases to test, or testability improvements
3. [Security] — Input validation, secret handling, injection risks, or access control
4. [Performance] — Potential bottlenecks, unnecessary allocations, or scaling concerns
5. [Maintainability] — Readability, tech debt, documentation, or future extensibility

Make each question specific to the actual code (reference real function/type names). Do NOT ask generic questions."#;

    let response =
        llm::generate_with_retry(provider, system_prompt, explanation, FOLLOWUP_MAX_TOKENS)?;

    Ok(response
        .lines()
        .filter(|line| {
            let t = line.trim();
            !t.is_empty()
                && t.len() > 3
                && (t.starts_with("1.")
                    || t.starts_with("2.")
                    || t.starts_with("3.")
                    || t.starts_with("4.")
                    || t.starts_with("5."))
        })
        .map(|line| {
            let t = line.trim();
            if let Some(rest) = t.strip_prefix(|c: char| c.is_ascii_digit()) {
                rest.trim_start_matches('.').trim().to_string()
            } else {
                t.to_string()
            }
        })
        .take(5)
        .collect())
}

fn parse_summary_response(response: &str) -> (String, Vec<String>) {
    let mut summary = String::new();
    let mut questions = Vec::new();
    let mut in_summary = false;
    let mut in_questions = false;

    for line in response.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("## Summary")
            || trimmed.starts_with("**Summary**")
            || trimmed.starts_with("# Summary")
        {
            in_summary = true;
            in_questions = false;
            continue;
        }
        if trimmed.starts_with("## Follow-up")
            || trimmed.starts_with("**Follow-up")
            || trimmed.starts_with("# Follow-up")
            || trimmed.starts_with("## Questions")
        {
            in_summary = false;
            in_questions = true;
            continue;
        }

        if in_summary {
            summary.push_str(line);
            summary.push('\n');
        }

        if in_questions && questions.len() < 5 {
            let t = trimmed.to_string();
            if !t.is_empty()
                && (t.starts_with("1.")
                    || t.starts_with("2.")
                    || t.starts_with("3.")
                    || t.starts_with("4.")
                    || t.starts_with("5."))
            {
                if let Some(rest) = t.strip_prefix(|c: char| c.is_ascii_digit()) {
                    let q = rest.trim_start_matches('.').trim().to_string();
                    if !q.is_empty() {
                        questions.push(q);
                    }
                }
            }
        }
    }

    if summary.trim().is_empty() {
        summary = response.to_string();
    }
    (summary.trim().to_string(), questions)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_small_code() {
        let code = "fn main() {\n    println!(\"hello\");\n}\n";
        let chunks = chunk_code(code);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].start_line, 1);
        assert_eq!(chunks[0].end_line, 3);
        assert!(chunks[0].context_prefix.is_empty());
    }

    #[test]
    fn test_chunk_large_code() {
        let mut code = String::new();
        for i in 0..500 {
            code.push_str(&format!("let x{} = {};\n", i, i));
        }
        let chunks = chunk_code(&code);
        assert!(chunks.len() > 1);
        for i in 1..chunks.len() {
            assert_eq!(chunks[i].start_line, chunks[i - 1].end_line + 1);
        }
        assert_eq!(chunks.last().unwrap().end_line, 500);
        for chunk in chunks.iter().skip(1) {
            assert!(!chunk.context_prefix.is_empty());
        }
    }

    #[test]
    fn test_chunk_empty_code() {
        assert_eq!(chunk_code("").len(), 0);
    }

    #[test]
    fn test_parse_summary_response() {
        let response = "## Summary\nTest summary.\n\n## Follow-up Questions\n1. Q1?\n2. Q2?\n3. Q3?\n4. Q4?\n5. Q5?\n";
        let (summary, questions) = parse_summary_response(response);
        assert_eq!(summary, "Test summary.");
        assert_eq!(questions.len(), 5);
    }

    #[test]
    fn test_parse_summary_fallback() {
        let response = "No headers here.\nLine two.";
        let (summary, questions) = parse_summary_response(response);
        assert_eq!(summary, response);
        assert!(questions.is_empty());
    }

    #[test]
    fn test_definition_start() {
        assert!(is_definition_start("fn main() {"));
        assert!(is_definition_start("pub fn foo() {"));
        assert!(is_definition_start("class Foo:"));
        assert!(is_definition_start("export default function"));
        assert!(!is_definition_start("    let x = 5;"));
        assert!(!is_definition_start("// comment"));
    }
}
