use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [cmd, path] = args.as_slice() else {
        return usage();
    };
    if cmd != "check" {
        return usage();
    }
    let checked = std::fs::read_to_string(path)
        .map_err(|e| e.to_string())
        .and_then(|text| beacon::config::parse(&text));
    match checked {
        Ok(_) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{path}: {e}");
            ExitCode::from(2)
        }
    }
}

fn usage() -> ExitCode {
    eprintln!("usage: beacon check <path>");
    ExitCode::from(2)
}
