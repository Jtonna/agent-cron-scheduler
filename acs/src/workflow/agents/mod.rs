use crate::workflow::step::CostFragment;

/// An agent runtime that ACS knows how to invoke and parse output from.
/// Each variant of `AgentType` (in models::workflow) maps to one impl.
pub trait AgentImpl: Send + Sync {
    /// The default command template used when AgentStep.command_template is None.
    /// The placeholder `${prompt}` (note: dollar-brace) is substituted with the actual prompt.
    fn default_command_template(&self) -> &str;
    /// Construct a fresh streaming output parser. Called once per run of this agent step.
    fn output_parser(&self) -> Box<dyn AgentOutputParser>;
}

/// Streaming parser that consumes the agent's stdout chunks as they arrive
/// and produces an AgentOutput on finalization.
pub trait AgentOutputParser: Send {
    /// Process a stdout chunk. May contain partial NDJSON lines (the parser must buffer).
    fn parse_chunk(&mut self, chunk: &[u8]);
    /// Finalize and return the accumulated output. Called once after the process exits.
    fn finalize(self: Box<Self>) -> AgentOutput;
}

/// Aggregated output of an agent step's execution.
#[derive(Debug, Clone, Default)]
pub struct AgentOutput {
    /// Aggregated cost/usage data extracted from the agent's output.
    pub cost: Option<CostFragment>,
    /// The final assistant message extracted from the stream, if any.
    pub final_message: Option<String>,
}

pub mod claude_code_cli;

/// Resolve an `AgentType` to its impl.
pub fn impl_for(agent_type: &crate::models::workflow::AgentType) -> Box<dyn AgentImpl> {
    use crate::models::workflow::AgentType;
    match agent_type {
        AgentType::ClaudeCodeCli => Box::new(claude_code_cli::ClaudeCodeCli),
    }
}
