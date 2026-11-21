use crate::notify::Notify;
use crate::target::Target;
use std::process::ExitCode;

pub const USAGE: &str =
    "beacon serve [--config <path>] [--host <name>] [--listen <addr>] [--notify <path>]";

/// What `beacon serve` is started with; `beacon install` writes the same.
pub struct ServeArgs {
    pub target: Target,
    pub listen: String,
    pub notify: Option<String>,
}

impl ServeArgs {
    /// Parses serve's flags and validates the config; a bad flag is an error with `usage`.
    pub fn parse(args: &[&str], usage: &str) -> Result<ServeArgs, String> {
        let (target, rest) = Target::take(args)?;
        let (mut listen, mut notify) = ("127.0.0.1:7878".to_string(), None);
        for pair in rest.chunks(2) {
            match pair {
                ["--listen", v] => listen = v.to_string(),
                ["--notify", v] => notify = Some(v.to_string()),
                _ => return Err(format!("usage: {usage}")),
            }
        }
        target.host()?;
        Ok(ServeArgs {
            target,
            listen,
            notify,
        })
    }

    /// `serve --config <abs> --host <name> --listen <addr> [--notify <abs>]`: what a plist or unit runs.
    pub fn argv(&self) -> Result<Vec<String>, String> {
        let abs = |p: &str| {
            std::fs::canonicalize(p)
                .map(|a| a.display().to_string())
                .map_err(|e| format!("{p}: {e}"))
        };
        let mut argv = vec!["serve".into(), "--config".into(), abs(&self.target.path)?];
        argv.extend(["--host".into(), self.target.name.clone()]);
        argv.extend(["--listen".into(), self.listen.clone()]);
        if let Some(n) = &self.notify {
            argv.extend(["--notify".into(), abs(n)?]);
        }
        Ok(argv)
    }
}

/// `beacon serve`: answers status over HTTP until killed.
pub fn run(args: &[&str]) -> Result<ExitCode, String> {
    let s = ServeArgs::parse(args, USAGE)?;
    let notify = s.notify.as_deref().map(Notify::load).transpose()?;
    let (name, path) = (&s.target.name, &s.target.path);
    Err(crate::serve::serve(name, path, &s.listen, notify))
}
