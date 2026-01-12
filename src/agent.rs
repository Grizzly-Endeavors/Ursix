use anyhow::Result;

use crate::config::Config;
use crate::llm::LlmClient;

/// The core agent loop that orchestrates LLM calls and tool execution
pub struct Agent<L: LlmClient> {
    config: Config,
    client: L,
}

impl<L: LlmClient> Agent<L> {
    pub fn new(config: Config, client: L) -> Self {
        Self { config, client }
    }

    /// Run the agent loop with an initial user message
    pub async fn run(&mut self, _initial_message: &str) -> Result<()> {
        // TODO: Implement agent loop
        // 1. Send message to LLM with tool definitions
        // 2. Parse response for tool calls
        // 3. Execute tools and collect results
        // 4. Send tool results back to LLM
        // 5. Repeat until LLM responds without tool calls or max_turns reached
        let _ = &self.config;
        let _ = &self.client;
        Ok(())
    }
}
