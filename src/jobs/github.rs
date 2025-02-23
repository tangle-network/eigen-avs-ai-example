use crate::{
    apis::github::{GitHubClient, RepoFetchConfig},
    llm::{self, LlmTaskConfig},
    ExampleContext, ProcessorError, TaskManager, TASK_MANAGER_ABI_STRING,
};
use blueprint_sdk::{alloy::rpc::types::Log, event_listeners::evm::EvmContractEventListener, job};
use serde::Serialize;
use std::convert::Infallible;

/// Example pre-processor for handling inbound events
async fn pre_processor(
    (event, _log): (TaskManager::NewTaskCreated, Log),
) -> Result<Option<(String, String, Option<String>)>, ProcessorError> {
    // Extract owner, repo, and ref_name from the event
    let _task = event.task.message;
    let owner = "".to_string();
    let repo = "".to_string();
    let ref_name = None;

    Ok(Some((owner, repo, ref_name)))
}

#[job(
    id = 1,
    params(owner, repo, ref_name),
    event_listener(
        listener = EvmContractEventListener<ExampleContext, TaskManager::NewTaskCreated>,
        instance = TaskManager,
        abi = TASK_MANAGER_ABI_STRING,
        pre_processor = pre_processor,
    ),
)]
pub async fn github_spelling_job(
    context: ExampleContext,
    owner: String,
    repo: String,
    ref_name: Option<String>,
) -> Result<String, Infallible> {
    // Initialize GitHub client
    let github_client = GitHubClient::new(None).unwrap();

    // Configure repository fetch
    let config = RepoFetchConfig {
        owner: owner.clone(),
        repo: repo.clone(),
        file_extensions: vec!["rs".to_string(), "md".to_string(), "txt".to_string()],
        chunk_size: 5000, // Large enough for meaningful context
        ref_name: ref_name.clone(),
    };

    // Fetch repository contents
    let chunks = github_client.fetch_and_chunk_repo(config).await.unwrap();

    // Process each chunk for spelling errors
    let mut corrections = Vec::new();

    for chunk in &chunks {
        // Prompt for AI to check spelling with clear instructions
        let prompt = format!(
            "Please analyze this code/text for spelling errors. For each error:\n\
            1. Consider if it's actually a spelling error (not a variable name, command, or technical term)\n\
            2. Provide the correction only if you're highly confident it's an error\n\
            3. Maintain any special formatting or code syntax\n\n\
            Content to check:\n{}\n
            Write the output in JSON format with an object like this:\n
            {{
                \"file_path\": \"path/to/file.rs\",
                \"start_line\": 1,
                \"end_line\": 10,
                \"corrections\": [
                    {{
                        \"line_number\": 1,
                        \"original_text\": \"exmaple\",
                        \"corrected_text\": \"example\",
                        \"start_column\": 0,
                        \"end_column\": 7,
                        \"confidence\": 0.95
                    }}
                ]
            }}",
            chunk.content
        );

        let response = llm::execute_llm_task(LlmTaskConfig {
            model: "gpt-4o-mini".to_string(),
            system_prompt: "You are a helpful assistant that checks for spelling errors in code."
                .to_string(),
            content: prompt,
        })
        .await
        .unwrap();

        // Parse the LLM response into a SpellingCorrection
        if let Ok(spelling_result) = llm::parse_spelling_correction(
            &response,
            chunk.file_path.clone(),
            chunk.start_line,
            chunk.end_line,
        ) {
            // Only add corrections if there are any
            if !spelling_result.corrections.is_empty() {
                corrections.push(spelling_result);
            }
        }
    }

    // Create structured JSON output
    let result = SpellingResult {
        repository: RepositoryInfo {
            owner,
            repo,
            ref_name: ref_name.unwrap_or_else(|| "main".to_string()),
        },
        corrections,
        metadata: ResultMetadata {
            total_files_checked: chunks.len(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        },
    };

    Ok(serde_json::to_string(&result).unwrap())
}

#[derive(Debug, Serialize)]
struct SpellingResult {
    repository: RepositoryInfo,
    corrections: Vec<SpellingCorrection>,
    metadata: ResultMetadata,
}

#[derive(Debug, Serialize)]
struct RepositoryInfo {
    owner: String,
    repo: String,
    ref_name: String,
}

#[derive(Debug, Serialize)]
pub struct SpellingCorrection {
    pub file_path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub corrections: Vec<CorrectionDetail>,
}

#[derive(Debug, Serialize)]
pub struct CorrectionDetail {
    pub line_number: usize,
    pub original_text: String,
    pub corrected_text: String,
    pub start_column: usize,
    pub end_column: usize,
    pub confidence: f64,
}

#[derive(Debug, Serialize)]
struct ResultMetadata {
    total_files_checked: usize,
    timestamp: String,
}
