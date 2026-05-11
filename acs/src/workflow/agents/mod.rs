use crate::workflow::step::CostFragment;

/// An agent runtime that ACS knows how to invoke and parse output from.
/// Each variant of `AgentType` (in models::workflow) maps to one impl.
pub trait AgentImpl: Send + Sync {
    /// Build the argv array for spawning this agent.
    ///
    /// `prompt` is the fully-resolved prompt string (already substituted by the
    /// template engine). `model` overrides the default model when present.
    /// `extra_args` are appended verbatim at the end of the argument list.
    ///
    /// The returned Vec always has at least one element (the program name).
    fn build_argv(&self, prompt: &str, model: Option<&str>, extra_args: &[String]) -> Vec<String>;
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
