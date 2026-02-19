use anyhow::Result;

use crate::llm::Provider;

/// Maximum lines per chunk for LLM processing
const MAX_CHUNK_LINES: usize = 200;

/// A chunk of code with its line range
#[derive(Debug)]
pub struct CodeChunk {
    pub start_line: usize,
    pub end_line: usize,
    pub content: String,
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
}

/// Split code into logical chunks, trying to respect function/block boundaries
pub fn chunk_code(code: &str) -> Vec<CodeChunk> {
    let lines: Vec<&str> = code.lines().collect();
    let total = lines.len();

    if total == 0 {
        return vec![];
    }

    // If the code fits in one chunk, don't split
    if total <= MAX_CHUNK_LINES {
        return vec![CodeChunk {
            start_line: 1,
            end_line: total,
            content: code.to_string(),
        }];
    }

    let mut chunks = Vec::new();
    let mut start = 0;

    while start < total {
        let mut end = (start + MAX_CHUNK_LINES).min(total);

        // Try to find a good break point (empty line, closing brace, function boundary)
        if end < total {
            let search_start = if end > 20 { end - 20 } else { start };
            let mut best_break = end;

            // Look backwards from the target end for a good split point
            for i in (search_start..end).rev() {
                let line = lines[i].trim();

                // Best: empty line between definitions
                if line.is_empty() {
                    best_break = i + 1;
                    break;
                }

                // Good: closing brace at start of line (end of function/block)
                if line == "}" || line == "};" || line == "end" {
                    best_break = i + 1;
                    break;
                }

                // OK: line starting with fn/def/func/class/pub (start of new definition)
                if line.starts_with("fn ")
                    || line.starts_with("pub fn ")
                    || line.starts_with("pub(crate) fn ")
                    || line.starts_with("def ")
                    || line.starts_with("func ")
                    || line.starts_with("class ")
                    || line.starts_with("function ")
                    || line.starts_with("const ")
                    || line.starts_with("let ")
                    || line.starts_with("var ")
                    || line.starts_with("type ")
                    || line.starts_with("struct ")
                    || line.starts_with("impl ")
                    || line.starts_with("trait ")
                    || line.starts_with("interface ")
                    || line.starts_with("package ")
                    || line.starts_with("module ")
                {
                    best_break = i;
                    break;
                }
            }

            end = best_break;
        }

        // Ensure we make progress
        if end <= start {
            end = (start + MAX_CHUNK_LINES).min(total);
        }

        let chunk_lines = &lines[start..end];
        chunks.push(CodeChunk {
            start_line: start + 1,
            end_line: end,
            content: chunk_lines.join("\n"),
        });

        start = end;
    }

    chunks
}

/// Process all chunks through the LLM and assemble a full explanation
pub fn explain_code(
    code: &str,
    provider: &dyn Provider,
    verbose: bool,
) -> Result<ExplainResult> {
    let chunks = chunk_code(code);
    let total_lines = code.lines().count();
    let total_chunks = chunks.len();

    if verbose {
        eprintln!(
            "[debug] code: {} lines, {} chunks (max {} lines/chunk)",
            total_lines, total_chunks, MAX_CHUNK_LINES
        );
    }

    let mut chunk_explanations = Vec::new();

    for (i, chunk) in chunks.iter().enumerate() {
        if total_chunks > 1 {
            eprintln!(
                "  Analyzing chunk {}/{} (lines {}-{})...",
                i + 1, total_chunks, chunk.start_line, chunk.end_line
            );
        }

        let system_prompt = build_chunk_system_prompt(i + 1, total_chunks);
        let user_prompt = format!(
            "Lines {}-{}:\n\n```\n{}\n```",
            chunk.start_line, chunk.end_line, chunk.content
        );

        let explanation = provider.generate(&system_prompt, &user_prompt)?;

        chunk_explanations.push(ChunkExplanation {
            start_line: chunk.start_line,
            end_line: chunk.end_line,
            explanation,
        });
    }

    // Generate overall summary and follow-up questions
    let (summary, questions) = if total_chunks > 1 {
        generate_summary_and_questions(provider, &chunk_explanations, total_lines)?
    } else {
        // For single chunk, extract summary from the explanation directly
        let explanation = &chunk_explanations[0].explanation;
        let questions = generate_follow_up_only(provider, explanation)?;
        (explanation.clone(), questions)
    };

    Ok(ExplainResult {
        total_lines,
        total_chunks,
        chunk_explanations,
        overall_summary: summary,
        follow_up_questions: questions,
    })
}

fn build_chunk_system_prompt(chunk_num: usize, total_chunks: usize) -> String {
    if total_chunks == 1 {
        return r#"You are an expert code analyst. Analyze the given code and provide:

1. **Overview**: What the code does at a high level
2. **Functions & Components**: Explain each function/method, its purpose, parameters, and return values
3. **Key Logic**: Highlight important algorithms, patterns, or design decisions
4. **Dependencies**: Note any imports, external libraries, or dependencies used

Be thorough but concise. Use markdown formatting."#.to_string();
    }

    format!(
        r#"You are an expert code analyst. You are analyzing chunk {chunk_num} of {total_chunks} of a larger codebase.

Analyze this code segment and provide:

1. **Summary**: What this chunk does
2. **Functions & Components**: Explain each function/method in this chunk
3. **Key Details**: Important patterns, edge cases, or notable logic

Be thorough but concise. Use markdown formatting. Focus only on what's in this chunk."#
    )
}

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
2. **Follow-up Questions**: Generate exactly 5 insightful follow-up questions that would help someone understand the code better

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
        "The codebase has {} total lines. Here are the chunk analyses:\n\n{}",
        total_lines, combined
    );

    let response = provider.generate(system_prompt, &user_prompt)?;

    // Parse summary and questions from the response
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

    let response = provider.generate(system_prompt, explanation)?;

    let questions: Vec<String> = response
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty()
                && (trimmed.starts_with("1.")
                    || trimmed.starts_with("2.")
                    || trimmed.starts_with("3.")
                    || trimmed.starts_with("4.")
                    || trimmed.starts_with("5."))
        })
        .map(|line| {
            let trimmed = line.trim();
            // Remove the number prefix
            if let Some(rest) = trimmed.strip_prefix(|c: char| c.is_ascii_digit()) {
                rest.trim_start_matches('.').trim().to_string()
            } else {
                trimmed.to_string()
            }
        })
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

        if trimmed.starts_with("## Summary") || trimmed.starts_with("**Summary**") {
            in_summary = true;
            in_questions = false;
            continue;
        }

        if trimmed.starts_with("## Follow-up") || trimmed.starts_with("**Follow-up") {
            in_summary = false;
            in_questions = true;
            continue;
        }

        if in_summary {
            summary.push_str(line);
            summary.push('\n');
        }

        if in_questions {
            let trimmed = trimmed.to_string();
            if !trimmed.is_empty()
                && (trimmed.starts_with("1.")
                    || trimmed.starts_with("2.")
                    || trimmed.starts_with("3.")
                    || trimmed.starts_with("4.")
                    || trimmed.starts_with("5."))
            {
                // Strip the number prefix
                if let Some(rest) = trimmed.strip_prefix(|c: char| c.is_ascii_digit()) {
                    let q = rest.trim_start_matches('.').trim().to_string();
                    if !q.is_empty() {
                        questions.push(q);
                    }
                }
            }
        }
    }

    // Fallback: if parsing failed, use the whole response as summary
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
    }

    #[test]
    fn test_chunk_large_code() {
        // Generate 500 lines of code
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
}
