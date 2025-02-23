use rig::{
    completion::{Prompt, PromptError},
    message::{Message, Text, UserContent},
    OneOrMany,
};
use serde::{Deserialize, Serialize};
use std::env;
use thiserror::Error;

/// Error types for LLM operations
#[derive(Error, Debug)]
pub enum LlmError {
    #[error("Prompt error: {0}")]
    PromptError(#[from] PromptError),
    #[error("Failed to parse response: {0}")]
    ParseError(String),
}

/// Configuration for LLM tasks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmTaskConfig {
    /// The model to use (e.g. "deepseek-ai/DeepSeek-R1")
    pub model: String,
    /// System prompt/preamble for the task
    pub system_prompt: String,
    /// Input content to process
    pub content: String,
}

/// Execute an LLM task with the given configuration
pub async fn execute_llm_task(config: LlmTaskConfig) -> Result<String, PromptError> {
    let client = rig::providers::hyperbolic::Client::new(
        &env::var("HYPERBOLIC_API_KEY").expect("HYPERBOLIC_API_KEY not set"),
    );

    let agent = client
        .agent(&config.model)
        .preamble(&config.system_prompt)
        .build();

    let message: Message = Message::User {
        content: OneOrMany::one(UserContent::Text(Text {
            text: config.content,
        })),
    };

    agent.prompt(message).await
}

/// Parse the LLM response into a SpellingCorrection
pub fn parse_spelling_correction(
    llm_response: &str,
    file_path: String,
    start_line: usize,
    end_line: usize,
) -> Result<crate::jobs::github::SpellingCorrection, LlmError> {
    // Expected format:
    // {
    //   "file_path": "path/to/file.rs",
    //   "start_line": 1,
    //   "end_line": 10,
    //   "corrections": [
    //     {
    //       "line_number": 1,
    //       "original_text": "exmaple",
    //       "corrected_text": "example",
    //       "start_column": 0,
    //       "end_column": 7,
    //       "confidence": 0.95
    //     }
    //   ]
    // }

    let response: serde_json::Value = serde_json::from_str(llm_response)
        .map_err(|e| LlmError::ParseError(format!("Failed to parse JSON: {}", e)))?;

    let mut corrections = Vec::new();
    if let Some(corr_array) = response["corrections"].as_array() {
        for corr in corr_array {
            corrections.push(crate::jobs::github::CorrectionDetail {
                line_number: corr["line_number"].as_u64().ok_or_else(|| {
                    LlmError::ParseError("Missing or invalid 'line_number'".to_string())
                })? as usize,
                original_text: corr["original_text"]
                    .as_str()
                    .ok_or_else(|| LlmError::ParseError("Missing 'original_text'".to_string()))?
                    .to_string(),
                corrected_text: corr["corrected_text"]
                    .as_str()
                    .ok_or_else(|| LlmError::ParseError("Missing 'corrected_text'".to_string()))?
                    .to_string(),
                start_column: corr["start_column"].as_u64().ok_or_else(|| {
                    LlmError::ParseError("Missing or invalid 'start_column'".to_string())
                })? as usize,
                end_column: corr["end_column"].as_u64().ok_or_else(|| {
                    LlmError::ParseError("Missing or invalid 'end_column'".to_string())
                })? as usize,
                confidence: corr["confidence"].as_f64().ok_or_else(|| {
                    LlmError::ParseError("Missing or invalid 'confidence'".to_string())
                })?,
            });
        }
    }

    Ok(crate::jobs::github::SpellingCorrection {
        file_path,
        start_line,
        end_line,
        corrections,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_spelling_correction() {
        let llm_response = r#"{
            "file_path": "src/main.rs",
            "start_line": 1,
            "end_line": 10,
            "corrections": [
                {
                    "line_number": 3,
                    "original_text": "exmaple",
                    "corrected_text": "example",
                    "start_column": 10,
                    "end_column": 17,
                    "confidence": 0.95
                }
            ]
        }"#;

        let result =
            parse_spelling_correction(llm_response, "src/main.rs".to_string(), 1, 10).unwrap();

        assert_eq!(result.file_path, "src/main.rs");
        assert_eq!(result.start_line, 1);
        assert_eq!(result.end_line, 10);
        assert_eq!(result.corrections.len(), 1);

        let correction = &result.corrections[0];
        assert_eq!(correction.line_number, 3);
        assert_eq!(correction.original_text, "exmaple");
        assert_eq!(correction.corrected_text, "example");
        assert_eq!(correction.start_column, 10);
        assert_eq!(correction.end_column, 17);
        assert_eq!(correction.confidence, 0.95);
    }

    #[test]
    fn test_parse_invalid_json() {
        let result = parse_spelling_correction("invalid json", "test.rs".to_string(), 1, 10);

        assert!(matches!(result, Err(LlmError::ParseError(_))));
    }
}
