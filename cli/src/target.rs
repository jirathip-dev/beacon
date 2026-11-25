use crate::config::{self, Host, Id, MonitorsConfig};
use crate::probe;

/// The config file and the host in it that a command acts on.
pub struct Target {
    pub path: String,
    pub name: String,
}

impl Target {
    /// Takes `--config <path>` (else `$BEACON_CONFIG`) and `--host <name>` (else this machine's) out of `args`.
    pub fn take<'a>(mut args: &[&'a str]) -> Result<(Target, Vec<&'a str>), String> {
        let (mut path, mut name, mut rest) = (std::env::var("BEACON_CONFIG").ok(), None, vec![]);
        loop {
            match args {
                ["--config", v, more @ ..] => (path, args) = (Some(v.to_string()), more),
                ["--host", v, more @ ..] => (name, args) = (Some(v.to_string()), more),
                [a, more @ ..] => {
                    rest.push(*a);
                    args = more;
                }
                [] => break,
            }
        }
        let path = path.ok_or("no config: pass --config <path> or set BEACON_CONFIG")?;
        let name = name.map_or_else(probe::hostname, Ok)?;
        Ok((Target { path, name }, rest))
    }

    /// Reads and validates the file and returns this host's block.
    pub fn host(&self) -> Result<Host, String> {
        let mut config = load(&self.path)?;
        Id::try_from(self.name.clone())
            .ok()
            .and_then(|id| config.hosts.remove(&id))
            .ok_or(format!("{}: no host {:?} in hosts", self.path, self.name))
    }
}

/// Reads and validates a config file; errors name the path.
pub fn load(path: &str) -> Result<MonitorsConfig, String> {
    std::fs::read_to_string(path)
        .map_err(|e| e.to_string())
        .and_then(|text| config::parse(&text))
        .map_err(|e| format!("{path}: {e}"))
}
