use octocrab::models::repos::Content;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;

/// Configuration for repository content fetching
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoFetchConfig {
    /// Owner of the repository
    pub owner: String,
    /// Name of the repository
    pub repo: String,
    /// List of file extensions to include (e.g. ["rs", "toml"])
    pub file_extensions: Vec<String>,
    /// Maximum size of each chunk in characters
    pub chunk_size: usize,
    /// Optional specific branch or commit SHA
    pub ref_name: Option<String>,
}

/// Represents a chunk of code from the repository
#[derive(Debug, Clone, Serialize)]
pub struct CodeChunk {
    /// Path of the file this chunk is from
    pub file_path: String,
    /// The actual code content
    pub content: String,
    /// Start line number in the original file
    pub start_line: usize,
    /// End line number in the original file
    pub end_line: usize,
}

/// Custom error types for GitHub operations
#[derive(Error, Debug)]
pub enum GitHubError {
    #[error("GitHub API error: {0}")]
    ApiError(#[from] octocrab::Error),
    #[error("Content decoding error: {0}")]
    DecodingError(String),
    #[error("Invalid file type")]
    InvalidFileType,
    #[error("Rate limit exceeded")]
    RateLimitExceeded,
}

/// GitHub client wrapper for repository operations
pub struct GitHubClient {
    client: Arc<octocrab::Octocrab>,
    cache: Arc<RwLock<HashMap<String, Vec<Content>>>>,
}

impl GitHubClient {
    /// Create a new GitHub client instance
    pub fn new(token: Option<String>) -> Result<Self, GitHubError> {
        let builder = octocrab::OctocrabBuilder::new();
        let builder = if let Some(token) = token {
            builder.personal_token(token)
        } else {
            builder
        };

        Ok(Self {
            client: Arc::new(builder.build()?),
            cache: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Fetch repository content and chunk it according to the configuration
    pub async fn fetch_and_chunk_repo(
        &self,
        config: RepoFetchConfig,
    ) -> Result<Vec<CodeChunk>, GitHubError> {
        let cache_key = format!("{}/{}/{:?}", config.owner, config.repo, config.ref_name);

        let contents = {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.get(&cache_key) {
                cached.clone()
            } else {
                drop(cache); // Release the read lock before writing
                let contents = self.fetch_repository_contents(&config).await?;
                self.cache.write().await.insert(cache_key, contents.clone());
                contents
            }
        };

        let mut chunks = Vec::new();
        for content in contents {
            if let Some(ext) = content.path.split('.').last() {
                if config.file_extensions.contains(&ext.to_string()) {
                    if let Some(content_chunks) =
                        self.process_file_content(&content, &config).await?
                    {
                        chunks.extend(content_chunks);
                    }
                }
            }
        }

        Ok(chunks)
    }

    /// Fetch all contents of a repository recursively
    async fn fetch_repository_contents(
        &self,
        config: &RepoFetchConfig,
    ) -> Result<Vec<Content>, GitHubError> {
        println!(
            "Fetching repository contents for {}/{}",
            config.owner, config.repo
        );
        let mut contents = Vec::new();

        let ref_name = config.ref_name.as_deref().unwrap_or("main");
        println!("Using ref: {}", ref_name);

        let initial_contents = match self
            .client
            .repos(&config.owner, &config.repo)
            .get_content()
            .path("/")
            .r#ref(ref_name)
            .send()
            .await
        {
            Ok(content) => {
                println!("Successfully fetched root directory contents");
                content
            }
            Err(e) => {
                println!("Error fetching root directory: {:?}", e);
                return Err(GitHubError::ApiError(e));
            }
        };

        println!(
            "Found {} items in root directory",
            initial_contents.items.len()
        );

        // Process all items in the directory
        for item in initial_contents.items {
            println!("Processing item: {} (type: {})", item.path, item.r#type);
            contents.extend(self.fetch_content_recursive(config, &item).await?);
        }

        println!("Total files found: {}", contents.len());
        Ok(contents)
    }

    /// Recursively fetch content from the repository
    async fn fetch_content_recursive(
        &self,
        config: &RepoFetchConfig,
        content: &Content,
    ) -> Result<Vec<Content>, GitHubError> {
        let mut contents = Vec::new();

        match content.r#type.as_str() {
            "file" => {
                // Only fetch content for files with target extensions
                if let Some(ext) = content.path.split('.').last() {
                    if config.file_extensions.contains(&ext.to_string()) {
                        let file_content = self
                            .client
                            .repos(&config.owner, &config.repo)
                            .get_content()
                            .path(&content.path)
                            .r#ref(config.ref_name.as_deref().unwrap_or("main"))
                            .send()
                            .await?;

                        if let Some(item) = file_content.items.into_iter().next() {
                            contents.push(item);
                        }
                    }
                }
            }
            "dir" => {
                let dir_contents = self
                    .client
                    .repos(&config.owner, &config.repo)
                    .get_content()
                    .path(&content.path)
                    .r#ref(config.ref_name.as_deref().unwrap_or("main"))
                    .send()
                    .await?;

                for item in dir_contents.items {
                    contents.extend(Box::pin(self.fetch_content_recursive(config, &item)).await?);
                }
            }
            _ => {} // Ignore other types
        }

        Ok(contents)
    }

    /// Process file content and split it into chunks
    async fn process_file_content(
        &self,
        content: &Content,
        config: &RepoFetchConfig,
    ) -> Result<Option<Vec<CodeChunk>>, GitHubError> {
        if content.r#type != "file" {
            return Ok(None);
        }

        let Some(ext) = content.path.split('.').last() else {
            return Ok(None);
        };

        if !config.file_extensions.contains(&ext.to_string()) {
            return Ok(None);
        }

        let Some(encoded_content) = content.content.as_ref() else {
            return Ok(None);
        };

        // Remove any whitespace or newlines from base64 content
        let clean_content = encoded_content.replace('\n', "");

        let decoded_content = base64::decode(&clean_content)
            .map_err(|e| GitHubError::DecodingError(e.to_string()))?;

        let content_str = String::from_utf8(decoded_content)
            .map_err(|e| GitHubError::DecodingError(e.to_string()))?;

        let chunks = self.chunk_content(&content.path, &content_str, config.chunk_size);
        Ok(Some(chunks))
    }

    /// Split content into chunks while preserving line integrity
    fn chunk_content(&self, path: &str, content: &str, chunk_size: usize) -> Vec<CodeChunk> {
        let lines: Vec<&str> = content.lines().collect();
        let mut chunks = Vec::new();
        let mut current_chunk = String::new();
        let mut current_start_line = 1;
        let mut current_size = 0;

        for (i, line) in lines.iter().enumerate() {
            let line_with_newline = format!("{}\n", line);
            let line_size = line_with_newline.len();

            // Create a new chunk if this line would exceed the chunk size
            if current_size + line_size > chunk_size && !current_chunk.is_empty() {
                chunks.push(CodeChunk {
                    file_path: path.to_string(),
                    content: current_chunk,
                    start_line: current_start_line,
                    end_line: i,
                });

                current_chunk = String::new();
                current_start_line = i + 1;
                current_size = 0;
            }

            current_chunk.push_str(&line_with_newline);
            current_size += line_size;
        }

        // Add the last chunk if there's anything remaining
        if !current_chunk.is_empty() {
            chunks.push(CodeChunk {
                file_path: path.to_string(),
                content: current_chunk,
                start_line: current_start_line,
                end_line: lines.len(),
            });
        }

        chunks
    }
}

#[cfg(test)]
mod tests {
    use once_cell::sync::OnceCell;

    use super::*;

    static RUSTLS_INIT: OnceCell<()> = OnceCell::new();

    fn init_rustls() {
        RUSTLS_INIT.get_or_init(|| {
            rustls::crypto::ring::default_provider()
                .install_default()
                .expect("Failed to install rustls crypto provider");
        });
    }

    #[tokio::test]
    async fn test_chunk_content() {
        init_rustls();

        let client = GitHubClient::new(None).unwrap();
        let content = "line 1\nline 2\nline 3\nline 4\nline 5\n";
        let chunks = client.chunk_content("test.rs", content, 12);

        // With 12-char limit and 7-char lines ("line N\n"), each line should be its own chunk
        // because adding a second line would exceed the limit (7 + 7 = 14 > 12)
        assert_eq!(
            chunks.len(),
            5,
            "Should create a chunk per line due to size limit"
        );

        // Each line should be in its own chunk
        for (i, chunk) in chunks.iter().enumerate() {
            let line_num = i + 1;
            assert_eq!(chunk.start_line, line_num);
            assert_eq!(chunk.end_line, line_num);
            assert_eq!(chunk.content, format!("line {}\n", line_num));
        }
    }

    #[tokio::test]
    async fn test_fetch_real_repo() {
        init_rustls();

        let client = GitHubClient::new(None).unwrap();
        let config = RepoFetchConfig {
            owner: "tangle-network".to_string(),
            repo: "eigenlayer-bls-template".to_string(),
            file_extensions: vec!["rs".to_string(), "toml".to_string(), "sol".to_string()],
            chunk_size: 1000,
            ref_name: None,
        };

        let chunks = client.fetch_and_chunk_repo(config).await.unwrap();
        assert!(!chunks.is_empty(), "Should have found some source files");

        // Verify all chunks are valid
        for chunk in &chunks {
            assert!(chunk.content.len() <= 1000, "Chunk size exceeds limit");
            assert!(
                chunk.file_path.ends_with(".rs")
                    || chunk.file_path.ends_with(".toml")
                    || chunk.file_path.ends_with(".sol"),
                "Invalid file type: {}",
                chunk.file_path
            );
            assert!(!chunk.content.is_empty(), "Empty chunk content");
            assert!(chunk.start_line <= chunk.end_line, "Invalid line numbers");
        }

        // Count file types
        let (mut rs_count, mut toml_count, mut sol_count) = (0, 0, 0);
        for chunk in &chunks {
            match chunk.file_path.split('.').last() {
                Some("rs") => rs_count += 1,
                Some("toml") => toml_count += 1,
                Some("sol") => sol_count += 1,
                _ => panic!("Unexpected file type"),
            }
        }

        // Ensure we found at least one of each file type
        assert!(rs_count > 0, "No Rust files found");
        assert!(toml_count > 0, "No TOML files found");
        assert!(sol_count > 0, "No Solidity files found");
    }
}
