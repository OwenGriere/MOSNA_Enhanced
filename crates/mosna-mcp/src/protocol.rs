//! Model Context Protocol over stdio: one JSON-RPC message per line.
//!
//! Written out rather than taken from an SDK: the server needs four methods
//! (`initialize`, `ping`, `tools/list`, `tools/call`), no async runtime, and
//! nothing that would pull the workspace's minimum Rust version up.

use std::io::{BufRead, Write};

use serde_json::{json, Value};

use crate::server::Server;

pub const PROTOCOL_VERSION: &str = "2025-06-18";

const INSTRUCTIONS: &str = "\
MOSNA has two implementations of the same three steps — network (Delaunay), \
assortativity, niches — reading the same configuration.yaml and the same \
nodes_{patient}-{id}_{sample}-{id}.{ext} files: MOSNA_Enhanced (Rust) and \
MOSNA_GUI (Python). Before running anything, call check_config and check_nodes: \
the pipelines themselves only check that columns exist. Each finding says which \
implementation it affects. run_step starts a step in the background and checks \
its inputs first; poll it with job_status. Compare runs with compare_networks \
(expect exactly 1 on the same inputs), compare_assortativity (counts exact, \
permutation statistics within Monte-Carlo error) and compare_partitions \
(Python is unseeded: its own reruns agree at ARI ≈ 0.5). The server ships in \
both repositories; `backends` says which implementations it found.";

/// Serve until stdin closes.
pub fn serve(server: &Server, input: impl BufRead, mut output: impl Write) -> std::io::Result<()> {
    for line in input.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if let Some(response) = handle_line(server, &line) {
            serde_json::to_writer(&mut output, &response)?;
            output.write_all(b"\n")?;
            output.flush()?;
        }
    }
    server.jobs.shutdown();
    Ok(())
}

/// Answer one message; `None` for a notification, which gets no answer.
pub fn handle_line(server: &Server, line: &str) -> Option<Value> {
    let message: Value = match serde_json::from_str(line) {
        Ok(message) => message,
        Err(error) => return Some(error_response(Value::Null, -32700, &error.to_string())),
    };
    let id = message.get("id").cloned()?;
    let method = message.get("method").and_then(Value::as_str).unwrap_or("");
    let params = message.get("params").cloned().unwrap_or(Value::Null);

    let result = match method {
        "initialize" => {
            let requested = params
                .get("protocolVersion")
                .and_then(Value::as_str)
                .unwrap_or(PROTOCOL_VERSION);
            json!({
                "protocolVersion": negotiate(requested),
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "mosna", "version": env!("CARGO_PKG_VERSION") },
                "instructions": INSTRUCTIONS,
            })
        }
        "ping" => json!({}),
        "tools/list" => json!({ "tools": tool_definitions() }),
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let arguments = params.get("arguments").cloned().unwrap_or(json!({}));
            match server.call(name, &arguments) {
                Ok(value) => tool_result(&value, false),
                Err(error) => {
                    let chain: Vec<String> = error.chain().map(|e| e.to_string()).collect();
                    tool_result(&json!({ "error": chain.join(": ") }), true)
                }
            }
        }
        other => {
            return Some(error_response(
                id,
                -32601,
                &format!("method `{other}` not found"),
            ));
        }
    };
    Some(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
}

/// The client's version when it is one this server speaks, else ours.
fn negotiate(requested: &str) -> &str {
    const SUPPORTED: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];
    SUPPORTED
        .iter()
        .find(|v| **v == requested)
        .copied()
        .unwrap_or(PROTOCOL_VERSION)
}

fn tool_result(value: &Value, is_error: bool) -> Value {
    let text = serde_json::to_string_pretty(value).unwrap_or_default();
    json!({ "content": [{ "type": "text", "text": text }], "isError": is_error })
}

fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn read_only() -> Value {
    json!({ "readOnlyHint": true, "openWorldHint": false })
}

/// The tools, with their argument schemas.
pub fn tool_definitions() -> Value {
    let max_findings = json!({
        "type": "integer", "minimum": 1,
        "description": "Findings returned at most, worst first (default 40). The counts always cover all of them."
    });
    let working_dir = json!({
        "type": "string",
        "description": "Directory relative paths are resolved against, as the --working_dir of the pipelines (default: the server's current directory)."
    });
    let patient =
        json!({ "type": "string", "description": "Patient column name, as in the file names." });
    let sample =
        json!({ "type": "string", "description": "Sample column name; omit for one-level data." });

    json!([
        {
            "name": "check_config",
            "title": "Check configuration.yaml",
            "description": "Validate configuration.yaml for both implementations: the assert_params rules of each section, typed reads, and whether steps 2 and 3 can find step 1's files. Says when only one implementation would reject a value.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "config": { "type": "string", "description": "Path to configuration.yaml." },
                    "max_findings": max_findings,
                },
                "required": ["config"],
            },
            "annotations": read_only(),
        },
        {
            "name": "check_nodes",
            "title": "Check nodes files",
            "description": "Check that a directory of nodes files can become spatial networks, in both implementations: file naming (and files skipped silently, with why), identifiers Python would decode differently from Rust, readable files, coordinate columns that are numeric and finite, at least 3 distinct non-collinear cells, duplicate positions, phenotype nulls and types consistent across the cohort. Takes the Tysserand section of a config, and/or explicit columns that override it.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "config": { "type": "string", "description": "configuration.yaml whose Tysserand section describes the files." },
                    "working_dir": working_dir,
                    "nodes_dir": { "type": "string", "description": "Overrides `Nodes directory`." },
                    "extension": { "type": "string", "enum": ["parquet", "csv", "tsv"] },
                    "patient_column": patient,
                    "sample_column": { "type": "string", "description": "Overrides `Sample column name`; an empty string means one-level data." },
                    "x_column": { "type": "string" },
                    "y_column": { "type": "string" },
                    "phenotype_column": { "type": "string" },
                    "min_neighbors": { "type": "integer", "minimum": 0 },
                    "max_findings": max_findings,
                },
            },
            "annotations": read_only(),
        },
        {
            "name": "check_network",
            "title": "Check network files",
            "description": "Check a directory of networks (nodes_* + edges_* pairs) as steps 2 and 3 read them: every nodes file has its edges file, edges files hold only source/target, every edge points at an existing row, self-loops, duplicate edges, isolated cells, phenotype column present. With `config`, the directory and columns are those the given step would use (`Default` = temp/net_dir_mosna).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "config": { "type": "string" },
                    "step": { "type": "string", "enum": ["assortativity", "niches"], "description": "Which section of the config to follow (default assortativity)." },
                    "working_dir": working_dir,
                    "network_dir": { "type": "string", "description": "Without config: the directory to check." },
                    "extension": { "type": "string", "enum": ["parquet", "csv", "tsv"] },
                    "patient_column": patient,
                    "sample_column": sample,
                    "phenotype_column": { "type": "string" },
                    "max_findings": max_findings,
                },
            },
            "annotations": read_only(),
        },
        {
            "name": "run_step",
            "title": "Run a MOSNA step",
            "description": "Start one step with either implementation, in the background; returns a job id for job_status. The inputs are checked first and the step is not started if they would make it fail or give a wrong result. Overrides are written to a derived config under <working_dir>/.mosna-mcp/, never into the given file. Results go into working_dir, as with the GUI.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "step": { "type": "string", "enum": ["network", "assortativity", "niches"] },
                    "backend": { "type": "string", "enum": ["rust", "python"], "description": "rust = MOSNA_Enhanced (mosna binary), python = MOSNA_GUI (needs MOSNA_GUI_DIR)." },
                    "config": { "type": "string" },
                    "working_dir": { "type": "string" },
                    "overrides": { "type": "object", "description": "{\"Section\": {\"Key\": value}}, nested for sub-sections, e.g. {\"Niche Analysis\": {\"Aggregated nodes\": {\"n_clusters\": 8}}}." },
                    "figure_formats": { "type": "string", "description": "Rust only, e.g. \"png\" to skip the HTML figures." },
                    "results_in": { "type": "string", "description": "Rust niches only: sub-directory of Niche_Analysis for the results." },
                    "skip_preflight": { "type": "boolean" },
                    "max_findings": max_findings,
                },
                "required": ["step", "backend", "config", "working_dir"],
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "openWorldHint": false },
        },
        {
            "name": "job_status",
            "title": "Status of a run",
            "description": "Status, progress and the end of the log of a job started by run_step; on failure, the line that says why. Without job_id, lists every job.",
            "inputSchema": {
                "type": "object",
                "properties": { "job_id": { "type": "string" } },
            },
            "annotations": read_only(),
        },
        {
            "name": "cancel_job",
            "title": "Cancel a run",
            "description": "Stop a running job. What it already wrote stays in its working_dir.",
            "inputSchema": {
                "type": "object",
                "properties": { "job_id": { "type": "string" } },
                "required": ["job_id"],
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": true, "idempotentHint": true, "openWorldHint": false },
        },
        {
            "name": "compare_networks",
            "title": "Compare two network directories",
            "description": "Edge Jaccard, sample by sample, between two network directories (e.g. temp/net_dir_mosna of a Rust run and of a Python run). Delaunay is deterministic: on the same nodes the expected value is exactly 1.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "dir_a": { "type": "string" },
                    "dir_b": { "type": "string" },
                    "extension_a": { "type": "string", "enum": ["parquet", "csv", "tsv"] },
                    "extension_b": { "type": "string", "enum": ["parquet", "csv", "tsv"] },
                    "patient_column": patient,
                    "sample_column": sample,
                },
                "required": ["dir_a", "dir_b", "patient_column"],
            },
            "annotations": read_only(),
        },
        {
            "name": "compare_assortativity",
            "title": "Compare two assortativity tables",
            "description": "Compare two net_stat.csv (e.g. a Rust and a Python run of step 2) with a tolerance per kind of column: `# total`, `%`, `assort` and `RAW` are counts on the network and must be equal; MEAN, STD and Z come from permutations drawn by different generators, so they must only agree within Monte-Carlo error, which depends on n_shuffle. Returns a verdict, outliers per kind, the number of outliers expected by chance, and the worst cells.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "a": { "type": "string", "description": "net_stat.csv, or the working directory holding Assortativity/net_stat.csv." },
                    "b": { "type": "string" },
                    "n_shuffle": { "type": "integer", "minimum": 2, "description": "Permutations of both runs (`Number of shuffle`)." },
                    "n_shuffle_a": { "type": "integer", "minimum": 2 },
                    "n_shuffle_b": { "type": "integer", "minimum": 2 },
                    "config": { "type": "string", "description": "Read n_shuffle from this configuration instead." },
                    "sigmas": { "type": "number", "exclusiveMinimum": 0, "description": "Standard errors allowed on MEAN/STD/Z (default 4)." },
                },
                "required": ["a", "b"],
            },
            "annotations": read_only(),
        },
        {
            "name": "backends",
            "title": "Which implementations are here",
            "description": "Where the Rust (MOSNA_Enhanced `mosna`) and Python (MOSNA_GUI `package/`) implementations were found, how, and what to set when one is missing. The server ships in both repositories; someone may have only one.",
            "inputSchema": { "type": "object", "properties": {} },
            "annotations": read_only(),
        },
        {
            "name": "compare_partitions",
            "title": "Compare two niche partitions",
            "description": "ARI and AMI between two niche label columns on the same cells — e.g. `niches_1` written by Rust and `niches` written by Python into their nodes files. The answer says how to read it against Python's own noise floor. If a column is missing, the error lists the label columns that exist.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "dir_a": { "type": "string" },
                    "column_a": { "type": "string" },
                    "dir_b": { "type": "string" },
                    "column_b": { "type": "string" },
                    "extension": { "type": "string", "enum": ["parquet", "csv", "tsv"] },
                    "patient_column": patient,
                    "sample_column": sample,
                    "per_sample": { "type": "boolean", "description": "Also return the ARI of every sample." },
                },
                "required": ["dir_a", "column_a", "dir_b", "column_b", "patient_column"],
            },
            "annotations": read_only(),
        },
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server() -> Server {
        Server {
            locations: crate::backend::Locations::discover(&crate::backend::Probe {
                var: &|_| None,
                exe: None,
                cwd: None,
            }),
            jobs: Default::default(),
            protected: Vec::new(),
        }
    }

    #[test]
    fn initialize_echoes_a_supported_version() {
        let response = handle_line(
            &server(),
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26"}}"#,
        )
        .unwrap();
        assert_eq!(response["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(response["result"]["serverInfo"]["name"], "mosna");
    }

    #[test]
    fn notifications_get_no_answer() {
        assert!(handle_line(
            &server(),
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#
        )
        .is_none());
    }

    #[test]
    fn every_tool_is_listed_and_dispatched() {
        let server = server();
        for tool in tool_definitions().as_array().unwrap() {
            let name = tool["name"].as_str().unwrap();
            let error = server.call(name, &json!({})).err().map(|e| e.to_string());
            // Missing arguments are fine; an unknown tool is not.
            assert!(
                !error.unwrap_or_default().contains("unknown tool"),
                "{name} is listed but not dispatched"
            );
        }
    }

    #[test]
    fn a_tool_error_is_a_result_not_a_protocol_error() {
        let response = handle_line(
            &server(),
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"check_config","arguments":{"config":"/nonexistent.yaml"}}}"#,
        )
        .unwrap();
        assert_eq!(response["result"]["isError"], true);
    }

    #[test]
    fn an_unknown_method_is_a_protocol_error() {
        let response = handle_line(
            &server(),
            r#"{"jsonrpc":"2.0","id":3,"method":"resources/list"}"#,
        )
        .unwrap();
        assert_eq!(response["error"]["code"], -32601);
    }
}
