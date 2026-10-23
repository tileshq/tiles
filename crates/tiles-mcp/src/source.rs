//! The source seam (ADR-031): how a data source beyond Omni joins the `tiles`
//! command and the MCP server. The binary's entry point passes a list of
//! sources to `tiles_cli::main`, so a crate outside this workspace adds one
//! without editing `tiles-cli`. The warehouse source registers this way
//! (ADR-034).
//!
//! A source has a command and `tiles doctor` lines. While it is configured
//! it also [`Source::offer`]s a session its tools, the lines it adds to the
//! session instruction and to `tiles_chart`'s description, and fetches the
//! rows of the intents it serves ([`Source::fetch`]). [`Reach`] is what a
//! session can fetch from, which decides the tools' hints and approvals.

use std::time::{Duration, Instant};

use serde_json::Value;
use tiles_intent::source::WarehouseSource;

use crate::card::Fetched;
use crate::{OMNI_FIELDS_TOOL, OMNI_TOPICS_TOOL};

/// A data source registered with the `tiles` command.
pub trait Source {
    /// Its one word name, which is also its command: `tiles <name> ...`,
    /// and its `tiles run --allow-<name>` flag. A name the built-in
    /// commands use is never reached.
    fn name(&self) -> &'static str;

    /// Its line in `tiles --help`, such as `tiles warehouse <...>`.
    fn usage(&self) -> &'static str;

    /// Its lines in `tiles doctor`, read without reaching the network and
    /// holding no secret.
    fn doctor(&self) -> Vec<String>;

    /// Runs `tiles <name> <args>`.
    ///
    /// # Errors
    ///
    /// One line saying what failed, with no secret in it.
    fn command(&self, args: &[String]) -> Result<String, String>;

    /// What it adds to a session while the user has configured it, read
    /// without reaching the network; `None` while it is not configured.
    fn offer(&self) -> Option<Offer> {
        None
    }

    /// The hosts it reaches, in words for the `tiles doctor` network line,
    /// such as `bigquery.googleapis.com and oauth2.googleapis.com`.
    fn network(&self) -> Option<&'static str> {
        None
    }

    /// Runs one of the tools its [`Offer`] lists. The text goes to the
    /// model, so it holds metadata and never a row.
    ///
    /// # Errors
    ///
    /// One line naming what failed and the fix, with no secret in it.
    fn tool(&self, name: &str, _arguments: &Value) -> Result<String, String> {
        Err(format!("{} has no tool {name}", self.name()))
    }

    /// The rows of a `data.warehouse` intent, when this source runs
    /// warehouse queries; `None` when it does not. `started` is when the
    /// tool call began, so the cards of one call share one time budget.
    ///
    /// # Errors
    ///
    /// The refusal, one line with no secret and no row value in it.
    fn fetch(
        &self,
        _query: &WarehouseSource,
        _started: Instant,
    ) -> Option<Result<Fetched, String>> {
        None
    }

    /// Whether the values of a card drawn from its rows go to the model,
    /// which the user opts into by a variable of the source's own.
    fn values_to_model(&self) -> bool {
        false
    }

    /// What `tiles setup` found of it on this machine, read without
    /// reaching the network and without reading or copying a secret;
    /// `None` when it has nothing to report.
    fn found(&self) -> Option<Found> {
        None
    }
}

/// What `tiles setup` found of a source on this machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    /// One line naming what it found, such as a login file and its type.
    pub line: String,
    /// The commands that finish setting it up, in the order to run them;
    /// empty when it is set up or nothing of it was found.
    pub next: Vec<String>,
}

/// One MCP tool a source offers.
#[derive(Debug, Clone, PartialEq)]
pub struct Tool {
    /// Its name, `tiles_<source>_<what>`.
    pub name: &'static str,
    /// Its title.
    pub title: &'static str,
    /// What it does, for the model.
    pub description: &'static str,
    /// Its input schema.
    pub input: Value,
}

/// What a configured source adds to a session.
#[derive(Debug, Clone, PartialEq)]
pub struct Offer {
    /// The source's [`Source::name`], which `tiles run --allow-<name>`
    /// takes.
    pub name: &'static str,
    /// The name its rows go by in a chart result, such as `BigQuery`.
    pub origin: &'static str,
    /// Its tools, beside `tiles_chart` and `tiles_theme`.
    pub tools: Vec<Tool>,
    /// The sentences it adds to the session instruction.
    pub instruction: String,
    /// The line it adds to `tiles_chart`'s description.
    pub chart_line: String,
    /// The longest one `tiles_chart` call may take while it fetches, so a
    /// harness waits that long for the tool.
    pub tool_timeout: Duration,
}

/// What a session can fetch rows from: Omni when it is configured, and
/// each registered source that is.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Reach {
    /// Whether Omni is configured.
    pub omni: bool,
    /// What each configured source offers, in registration order.
    pub offers: Vec<Offer>,
}

impl Reach {
    /// Only Omni, as configured or not.
    #[must_use]
    pub fn omni(omni: bool) -> Self {
        Self {
            omni,
            offers: Vec::new(),
        }
    }

    /// Whether any tool can fetch data from outside this machine, so no
    /// tool is marked read-only and every one asks before it runs.
    #[must_use]
    pub fn fetches(&self) -> bool {
        self.omni || !self.offers.is_empty()
    }

    /// The tools beside `tiles_chart` and `tiles_theme`: Omni's, then each
    /// source's.
    #[must_use]
    pub fn data_tools(&self) -> Vec<&'static str> {
        let mut tools = Vec::new();
        if self.omni {
            tools.extend([OMNI_TOPICS_TOOL, OMNI_FIELDS_TOOL]);
        }
        tools.extend(
            self.offers
                .iter()
                .flat_map(|o| o.tools.iter().map(|t| t.name)),
        );
        tools
    }

    /// The longest a `tiles_chart` call may take, of every source's.
    #[must_use]
    pub fn tool_timeout(&self) -> Option<Duration> {
        self.offers.iter().map(|o| o.tool_timeout).max()
    }

    /// Whether every source this session fetches from is in `allowed`, the
    /// names the user passed as `tiles run --allow-<name>` (`omni` among
    /// them), so the harness may run the tools without asking.
    #[must_use]
    pub fn all_allowed(&self, allowed: &[String]) -> bool {
        let named = |n: &str| allowed.iter().any(|a| a == n);
        (!self.omni || named("omni")) && self.offers.iter().all(|o| named(o.name))
    }
}
