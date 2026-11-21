/// True when built with `over-budget` and `BEACON_OVER_BUDGET` names `line`: the budget tests' switch.
pub fn over_budget(line: &str) -> bool {
    cfg!(feature = "over-budget") && std::env::var("BEACON_OVER_BUDGET").is_ok_and(|v| v == line)
}
