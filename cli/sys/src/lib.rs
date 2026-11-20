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
