//! Binary entry point: serve MCP on stdin/stdout.
//!
//! stdout carries the protocol and nothing else; anything for a human goes to
//! stderr, which MCP clients show as the server's log.

fn main() -> std::process::ExitCode {
    let server = mosna_mcp::Server::from_env();
    let stdin = std::io::stdin().lock();
    let stdout = std::io::stdout().lock();
    match mosna_mcp::protocol::serve(&server, stdin, stdout) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("mosna-mcp: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
