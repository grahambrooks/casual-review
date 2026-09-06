//! Casual Review Zed extension.
//!
//! Zed removed extension-provided slash commands together with text threads
//! (zed-industries/zed#53760), so this extension integrates through the modern
//! path instead: it registers an **MCP server** that Zed's Agent talks to. The
//! server is `cr mcp` — the same `cr` binary, run in stdio MCP mode — which
//! exposes the review-comment operations (list/add/reply/resolve/reanchor and
//! sync) as tools. The Agent can then read and write review comments stored in
//! the repository directly.
//!
//! `cr` must be resolvable when Zed launches the server. By default we invoke
//! `cr` from `PATH`; override it in Zed settings if `cr` lives elsewhere:
//!
//! ```jsonc
//! "context_servers": {
//!   "casual-review": {
//!     "command": { "path": "/absolute/path/to/cr", "args": ["mcp"] }
//!   }
//! }
//! ```

use zed_extension_api::{
    self as zed, settings::ContextServerSettings, Command, ContextServerId, Project, Result,
};

struct CasualReview;

impl zed::Extension for CasualReview {
    fn new() -> Self {
        Self
    }

    fn context_server_command(
        &mut self,
        context_server_id: &ContextServerId,
        project: &Project,
    ) -> Result<Command> {
        // Honour a user-provided command override from Zed settings; otherwise
        // run `cr mcp` from PATH.
        let override_command = ContextServerSettings::for_project(context_server_id.as_ref(), project)
            .ok()
            .and_then(|settings| settings.command);

        let program = override_command
            .as_ref()
            .and_then(|c| c.path.clone())
            .unwrap_or_else(|| "cr".to_string());
        let args = override_command
            .as_ref()
            .and_then(|c| c.arguments.clone())
            .unwrap_or_else(|| vec!["mcp".to_string()]);

        let mut command = Command::new(program).args(args);
        if let Some(env) = override_command.and_then(|c| c.env) {
            command = command.envs(env);
        }
        Ok(command)
    }
}

zed::register_extension!(CasualReview);
