/// User, system, idle and nice cpu ticks since boot, from `host_statistics(HOST_CPU_LOAD_INFO)`.
#[cfg(target_os = "macos")]
pub fn host_cpu_load() -> Option<[u32; 4]> {
    unsafe extern "C" {
        safe fn mach_host_self() -> u32;
        fn host_statistics(host: u32, flavor: i32, info: *mut u32, count: *mut u32) -> i32;
    }
    static HOST: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    let (mut info, mut count) = ([0u32; 4], 4u32);
    // SAFETY: HOST_CPU_LOAD_INFO (3) fills at most `count` (4) u32s of `info`, both owned here.
    let ok = unsafe {
        host_statistics(
            *HOST.get_or_init(|| mach_host_self()),
            3,
            info.as_mut_ptr(),
            &raw mut count,
        )
    };
    (ok == 0).then_some(info)
}

/// Lifetime peak footprint, idle plus interrupt wake-ups, and own plus reaped children's cpu, of process `pid`.
#[cfg(target_os = "macos")]
#[derive(Debug)]
pub struct ProcUsage {
    pub peak_bytes: u64,
    pub wakeups: u64,
    pub cpu_ns: u64,
}

/// `pid`'s usage from `proc_pid_rusage(RUSAGE_INFO_V4)`, its times converted by `mach_timebase_info`.
#[cfg(target_os = "macos")]
#[must_use]
pub fn proc_usage(pid: i32) -> Option<ProcUsage> {
    unsafe extern "C" {
        fn proc_pid_rusage(pid: i32, flavor: i32, buffer: *mut u64) -> i32;
        fn mach_timebase_info(info: *mut [u32; 2]) -> i32;
    }
    let (mut info, mut base) = ([0u64; 64], [0u32; 2]);
    // SAFETY: RUSAGE_INFO_V4 (4) writes one rusage_info_v4, under the 512 bytes of `info`, owned here.
    let ok = unsafe { proc_pid_rusage(pid, 4, info.as_mut_ptr()) } == 0;
    // SAFETY: mach_timebase_info writes one {numer, denom} pair of u32s into `base`, owned here.
    let based = unsafe { mach_timebase_info(&raw mut base) } == 0 && base[1] > 0;
    let at = |i: usize| info.get(i).copied();
    let ticks = [at(2)?, at(3)?, at(12)?, at(13)?]
        .iter()
        .map(|&t| u128::from(t))
        .sum::<u128>();
    let cpu_ns = u64::try_from(ticks * u128::from(base[0]) / u128::from(base[1])).ok()?;
    (ok && based).then_some(ProcUsage {
        peak_bytes: at(30)?,
        wakeups: at(4)? + at(5)?,
        cpu_ns,
    })
}
