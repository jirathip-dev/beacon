use crate::config::{self, Id, Label, Review};
use serde::Deserialize;
use serde_json::value::RawValue;
use std::collections::BTreeMap;

#[derive(Deserialize)]
struct Top<'a> {
    #[serde(borrow)]
    hosts: BTreeMap<String, &'a RawValue>,
}

#[derive(Deserialize)]
struct HostText<'a> {
    #[serde(borrow)]
    jobs: Option<&'a RawValue>,
}

/// Sets `label`'s review in `host`'s `jobs` block, rewriting only that block's bytes; the text as it was when unchanged.
pub fn set(text: &str, host: &Id, label: &Label, review: &Review) -> Result<String, String> {
    let mut config = config::parse(text)?;
    let ours = config
        .hosts
        .get_mut(host)
        .ok_or(format!("no host {:?} in hosts", host.as_str()))?;
    if ours.jobs.get(label) == Some(review) {
        return Ok(text.into());
    }
    ours.jobs.insert(label.clone(), review.clone());
    let top: Top = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let raw = top.hosts.get(host.as_str()).ok_or("no such host")?;
    let at = |s: &str| {
        s.as_ptr()
            .addr()
            .checked_sub(text.as_ptr().addr())
            .ok_or("bad offset")
    };
    let parsed: HostText = serde_json::from_str(raw.get()).map_err(|e| e.to_string())?;
    let (start, end, lead) = if let Some(jobs) = parsed.jobs {
        let start = at(jobs.get())?;
        (start, start + jobs.get().len(), "")
    } else {
        let close = at(raw.get())? + raw.get().len() - 1;
        let last = text.get(..close).ok_or("bad offset")?.trim_end().len();
        (last, last, ",\n")
    };
    let before = text.get(..start).ok_or("bad offset")?;
    let line = before
        .rfind('\n')
        .map_or(before, |n| before.get(n + 1..).unwrap_or(""));
    let indent: String = line
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect();
    let mut block = String::from(lead);
    if parsed.jobs.is_none() {
        block += &format!("{indent}\"jobs\": ");
    }
    let lines: Vec<String> = ours
        .jobs
        .iter()
        .map(|(l, r)| {
            let (l, r) = (serde_json::to_string(l.as_str()), serde_json::to_string(r));
            format!(
                "{indent}  {}: {}",
                l.unwrap_or_default(),
                r.unwrap_or_default()
            )
        })
        .collect();
    block += &format!("{{\n{}\n{indent}}}", lines.join(",\n"));
    let after = text.get(end..).ok_or("bad offset")?;
    let new = format!("{before}{block}{after}");
    config::parse(&new)?;
    Ok(new)
}
