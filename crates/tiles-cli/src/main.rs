//! The `tiles` binary: the `tiles` command with the data sources this build
//! registers through [`tiles_mcp::source::Source`] (ADR-031): with the
//! `warehouse` feature, which only the private build has, the `BigQuery` and
//! dbt warehouse (ADR-034); a build from the public tree registers none
//! (ADR-059).

use std::process::ExitCode;

fn main() -> ExitCode {
    tiles_cli::main(&[])
}
