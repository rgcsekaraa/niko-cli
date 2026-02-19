use std::time::Instant;

use anyhow::Result;

use crate::llm::{self, Provider};

/// Maximum lines per chunk for LLM processing
const MAX_CHUNK_LINES: usize = 200;
/// Context overlap — carry last N lines from previous chunk for boundary continuity
const CONTEXT_OVERLAP_LINES: usize = 10;

/// A chunk of code with its line range
#[derive(Debug, Clone)]
pub struct CodeChunk {
    pub start_line: usize,
    pub end_line: usize,
    pub content: String,
    /// Overlapping lines from end of previous chunk (for boundary continuity)
    pub context_prefix: String,
}

/// Result of explaining a single chunk
#[derive(Debug)]
pub struct ChunkExplanation {
    pub start_line: usize,
    pub end_line: usize,
    pub explanation: String,
}

/// Full explanation result with summary and follow-up questions
#[derive(Debug)]
pub struct ExplainResult {
    pub total_lines: usize,
    pub total_chunks: usize,
    pub chunk_explanations: Vec<ChunkExplanation>,
    pub overall_summary: String,
    pub follow_up_questions: Vec<String>,
    pub elapsed: std::time::Duration,
}

/// Split code into logical chunks with overlap for boundary continuity.
///
/// Strategy: each chunk after the first carries the last N lines of code from the
/// previous chunk as a context prefix. This ensures the LLM can see across chunk
/// boundaries without lossy summarisation. The final synthesis step receives ALL
/// chunk explanations untruncated, so no details are ever lost.
pub fn chunk_code(code: &str) -> Vec<CodeChunk> {
    let lines: Vec<&str> = code.lines().collect();
    let total = lines.len();

    if total == 0 {
        return vec![];
    }

    // Single chunk — no splitting needed
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

        // Try to find a good split point (look back up to 30 lines)
        if end < total {
            let search_start = if end > 30 { end - 30 } else { start };
            let mut best_break = end;

            for i in (search_start..end).rev() {
                let line = lines[i].trim();

                // Best: empty line between definitions
                if line.is_empty() {
                    best_break = i + 1;
                    break;
                }

                // Good: closing brace/end keyword
                if line == "}" || line == "};" || line == "end" {
                    best_break = i + 1;
                    break;
                }

                // OK: start of a new top-level definition
                if is_definition_start(line) {
                    best_break = i;
                    break;
                }
            }

            end = best_break;
        }

        // Ensure progress
        if end <= start {
            end = (start + MAX_CHUNK_LINES).min(total);
        }

        // Build context prefix: last N lines from previous chunk for boundary continuity
        let context_prefix = if !chunks.is_empty() {
            let ctx_start = if start >= CONTEXT_OVERLAP_LINES {
                start - CONTEXT_OVERLAP_LINES
            } else {
                0
            };
            let ctx_lines = &lines[ctx_start..start];
            format!(
                "// [context: preceding lines {}-{} shown for continuity]\n{}\n// [chunk starts here]\n",
                ctx_start + 1,
                start,
                ctx_lines.join("\n")
            )
        } else {
            String::new()
        };

        let chunk_lines = &lines[start..end];
        chunks.push(CodeChunk {
            start_line: start + 1,
            end_line: end,
            content: chunk_lines.join("\n"),
            context_prefix,
        });

        start = end;
    }

    chunks
}

/// Check if a line starts a top-level definition (used for smart split points)
fn is_definition_start(line: &str) -> bool {
    let prefixes = [
        "fn ", "pub fn ", "pub(crate) fn ", "pub(super) fn ",
        "async fn ", "pub async fn ",
        "def ", "class ", "function ", "func ",
        "const ", "let ", "var ", "static ",
        "type ", "struct ", "impl ", "trait ",
        "interface ", "package ", "module ",
        "export ", "import ", "#[", "@",
    ];
    prefixes.iter().any(|p| line.starts_with(p))
}

/// Process all chunks through the LLM with retry logic and assemble a full explanation.
///
/// Architecture:
///   1. Each chunk is analysed independently with overlap lines for boundary context
///   2. Every LLM call uses generate_with_retry (exponential backoff, 3 attempts)
///   3. The final synthesis receives ALL chunk explanations untruncated — nothing is lost
///
/// This is faster than carrying running summaries (fewer tokens per chunk call) and
/// more accurate (no lossy intermediate summarisation).
pub fn explain_code(
    code: &str,
    provider: &dyn Provider,
    verbose: bool,
) -> Result<ExplainResult> {
    let start_time = Instant::now();
    let chunks = chunk_code(code);
    let total_lines = code.lines().count();
    let total_chunks = chunks.len();

    if verbose {
        eprintln!(
            "  [debug] {} lines → {} chunks (max {} lines/chunk, {} line overlap)",
            total_lines, total_chunks, MAX_CHUNK_LINES, CONTEXT_OVERLAP_LINES
        );
    }

    let mut chunk_explanations = Vec::new();

    for (i, chunk) in chunks.iter().enumerate() {
        if total_chunks > 1 {
            eprintln!(
                "  Chunk {}/{} (lines {}–{})…",
                i + 1, total_chunks, chunk.start_line, chunk.end_line
            );
        }

        let system_prompt = build_chunk_system_prompt(i + 1, total_chunks);

        // Feed the chunk with its overlap prefix for boundary continuity
        let user_prompt = if chunk.context_prefix.is_empty() {
            format!(
                "Lines {}-{} ({} lines):\n\n```\n{}\n```",
                chunk.start_line, chunk.end_line,
                chunk.end_line - chunk.start_line + 1,
                chunk.content
            )
        } else {
            format!(
                "{}\nLines {}-{} ({} lines):\n\n```\n{}\n```",
                chunk.context_prefix,
                chunk.start_line, chunk.end_line,
                chunk.end_line - chunk.start_line + 1,
                chunk.content
            )
        };

        // Use retry mechanism for resilience
        let explanation = llm::generate_with_retry(provider, &system_prompt, &user_prompt)?;

        chunk_explanations.push(ChunkExplanation {
            start_line: chunk.start_line,
            end_line: chunk.end_line,
            explanation,
        });
    }

    // Synthesis — all chunk explanations are fed in FULL (no truncation, no loss)
    let (summary, questions) = if total_chunks > 1 {
        generate_summary_and_questions(provider, &chunk_explanations, total_lines)?
    } else {
        let explanation = &chunk_explanations[0].explanation;
        let questions = generate_follow_up_only(provider, explanation).unwrap_or_default();
        (explanation.clone(), questions)
    };

    let elapsed = start_time.elapsed();

    Ok(ExplainResult {
        total_lines,
        total_chunks,
        chunk_explanations,
        overall_summary: summary,
        follow_up_questions: questions,
        elapsed,
    })
}

fn build_chunk_system_prompt(chunk_num: usize, total_chunks: usize) -> String {
    if total_chunks == 1 {
        return r#"You are an expert code analyst. Analyze the given code and provide:

1. **Overview**: What the code does at a high level
2. **Functions & Components**: Explain each function/method, its purpose, parameters, and return values
3. **Key Logic**: Highlight important algorithms, patterns, or design decisions
4. **Dependencies**: Note any imports, external libraries, or dependencies used

Be thorough but concise. Use markdown formatting."#
            .to_string();
    }

    format!(
        r#"You are an expert code analyst. You are analyzing chunk {chunk_num} of {total_chunks} of a larger codebase.

Some preceding lines may be included for boundary context — focus your analysis on the code after the "[chunk starts here]" marker.

Analyze this code segment and provide:

1. **Summary**: What this chunk does
2. **Functions & Components**: Explain each function/method in this chunk — purpose, parameters, return values
3. **Key Details**: Important patterns, edge cases, cross-references to functions/types you can infer exist elsewhere

Be thorough — capture every function, struct, and important constant. Do not omit anything. Use markdown formatting."#
    )
}

/// Synthesis step: receives ALL chunk explanations untruncated.
/// This is the only place where cross-chunk understanding happens.
fn generate_summary_and_questions(
    provider: &dyn Provider,
    explanations: &[ChunkExplanation],
    total_lines: usize,
) -> Result<(String, Vec<String>)> {
    let combined = explanations
        .iter()
        .map(|e| format!("### Lines {}-{}\n{}", e.start_line, e.end_line, e.explanation))
        .collect::<Vec<_>>()
        .join("\n\n---\n\n");

    let system_prompt = r#"You are an expert code analyst. You have analyzed a large codebase in chunks. 
Now synthesize the chunk analyses into:

1. **Overall Summary** (2-3 paragraphs): What the entire codebase does, its architecture, and key design decisions
2. **Follow-up Questions**: Generate exactly 5 insightful follow-up questions

Format your response exactly as:
## Summary
[your summary here]

## Follow-up Questions
1. [question 1]
2. [question 2]
3. [question 3]
4. [question 4]
5. [question 5]"#;

    let user_prompt = format!(
        "The codebase has {} total lines across {} chunks. Here are the complete chunk analyses:\n\n{}",
        total_lines, explanations.len(), combined
    );

    let response = llm::generate_with_retry(provider, system_prompt, &user_prompt)?;

    let (summary, questions) = parse_summary_response(&response);
    Ok((summary, questions))
}

fn generate_follow_up_only(provider: &dyn Provider, explanation: &str) -> Result<Vec<String>> {
    let system_prompt = r#"Based on the code explanation below, generate exactly 5 insightful follow-up questions that would help someone understand the code better. These could be about architecture, edge cases, potential improvements, testing, or usage.

Format your response as a numbered list:
1. [question]
2. [question]
3. [question]
4. [question]
5. [question]"#;

    let response = llm::generate_with_retry(provider, system_prompt, explanation)?;

    let questions: Vec<String> = response
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty() && trimmed.len() > 3
                && (trimmed.starts_with("1.")
                    || trimmed.starts_with("2.")
                    || trimmed.starts_with("3.")
                    || trimmed.starts_with("4.")
                    || trimmed.starts_with("5."))
        })
        .map(|line| {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix(|c: char| c.is_ascii_digit()) {
                rest.trim_start_matches('.').trim().to_string()
            } else {
                trimmed.to_string()
            }
        })
        .take(5)
        .collect();

    Ok(questions)
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
            let trimmed = trimmed.to_string();
            if !trimmed.is_empty()
                && (trimmed.starts_with("1.")
                    || trimmed.starts_with("2.")
                    || trimmed.starts_with("3.")
                    || trimmed.starts_with("4.")
                    || trimmed.starts_with("5."))
            {
                if let Some(rest) = trimmed.strip_prefix(|c: char| c.is_ascii_digit()) {
                    let q = rest.trim_start_matches('.').trim().to_string();
                    if !q.is_empty() {
                        questions.push(q);
                    }
                }
            }
        }
    }

    // Fallback: if parsing failed, use the whole response
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
        assert!(chunks[0].context_prefix.is_empty(), "First chunk should have no context");
    }

    #[test]
    fn test_chunk_large_code() {
        let mut code = String::new();
        for i in 0..500 {
            code.push_str(&format!("let x{} = {};\n", i, i));
        }
        let chunks = chunk_code(&code);
        assert!(chunks.len() > 1, "Should split into multiple chunks");

        // Verify no gaps
        for i in 1..chunks.len() {
            assert_eq!(
                chunks[i].start_line,
                chunks[i - 1].end_line + 1,
                "Chunks should be contiguous"
            );
        }

        // Verify all lines are covered
        assert_eq!(chunks.last().unwrap().end_line, 500);

        // Verify overlap context exists on chunks after the first
        for chunk in chunks.iter().skip(1) {
            assert!(
                !chunk.context_prefix.is_empty(),
                "Non-first chunks should have overlap context"
            );
            assert!(
                chunk.context_prefix.contains("context"),
                "Context prefix should be labelled"
            );
        }
    }

    #[test]
    fn test_chunk_empty_code() {
        let chunks = chunk_code("");
        assert_eq!(chunks.len(), 0);
    }

    #[test]
    fn test_parse_summary_response() {
        let response = "## Summary\nThis is a test summary.\n\n## Follow-up Questions\n1. Question one?\n2. Question two?\n3. Question three?\n4. Question four?\n5. Question five?\n";
        let (summary, questions) = parse_summary_response(response);
        assert_eq!(summary, "This is a test summary.");
        assert_eq!(questions.len(), 5);
    }

    #[test]
    fn test_parse_summary_fallback() {
        let response = "Here's my analysis without proper headers.\nLine two.";
        let (summary, questions) = parse_summary_response(response);
        assert_eq!(summary, response);
        assert!(questions.is_empty());
    }

    #[test]
    fn test_definition_start() {
        assert!(is_definition_start("fn main() {"));
        assert!(is_definition_start("pub fn foo() {"));
        assert!(is_definition_start("class Foo:"));
        assert!(is_definition_start("def bar():"));
        assert!(is_definition_start("export default function"));
        assert!(!is_definition_start("    let x = 5;"));
        assert!(!is_definition_start("// comment"));
    }
}
