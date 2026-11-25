# Metric rules

Metric rules in `hosts.<name>.rules`, `{"id", "metric": "disk"|"memory"|"load", "above": <n>, "for_min": <n>}`,
act like checks in `/status`: down once the metric (disk or memory % used per mount in `disks`, load1 per
core) stays above `above` for `for_min` minutes, up once it stays at or below it as long. Without `rules`:
`disk-full` > 90 for 10 min, `memory-full` > 90 for 10 min, `load-high` > 2 per core for 15 min; `"rules": []`
turns them off.
