/// Writes one line to stdout: the CLI's report.
#[expect(clippy::print_stdout, reason = "the CLI's one stdout writer")]
pub fn out(line: &str) {
    println!("{line}");
}

/// Writes one line to stderr: errors and the server's log.
#[expect(clippy::print_stderr, reason = "the CLI's one stderr writer")]
pub fn err(line: &str) {
    eprintln!("{line}");
}
