use beacon::config::MountPath;
use beacon::metrics::{DiskUsage, Gauge};
use beacon::sampler::{self, CpuTicks, Sampler};

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/tests/fixtures/sampler/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

#[test]
fn linux_load_cpu_mem_swap_and_df_parse_from_proc_and_df_text() {
    assert_eq!(
        sampler::linux_load(&fixture("linux-loadavg.txt")),
        Some([0.52, 0.58, 0.59])
    );
    let first = sampler::linux_cpu(&fixture("linux-stat-1.txt")).unwrap();
    let second = sampler::linux_cpu(&fixture("linux-stat-2.txt")).unwrap();
    assert_eq!(
        first,
        CpuTicks {
            busy: 5668,
            total: 3704867
        }
    );
    assert_eq!(
        second,
        CpuTicks {
            busy: 5828,
            total: 3705787
        }
    );
    assert_eq!(sampler::cpu_pct(first, second), Some(17.391304));
    let mem = Gauge {
        total: 8040096 * 1024,
        used: 2986639360,
    };
    let swap = Gauge {
        total: 2147479552,
        used: 1073741824,
    };
    assert_eq!(
        sampler::linux_mem(&fixture("linux-meminfo.txt")),
        Some((mem, swap))
    );
    let disk = DiskUsage {
        used: 24220598272,
        free: 38505025536,
    };
    assert_eq!(
        sampler::df(&fixture("linux-df.txt")),
        Some(disk),
        "a mount name with spaces"
    );
}

#[test]
fn macos_load_mem_swap_and_df_parse_from_the_minis_tool_output() {
    let sysctl = fixture("macos-sysctl.txt");
    assert_eq!(sampler::macos_load(&sysctl), Some([32.78, 31.37, 30.75]));
    let memsize = sysctl.lines().nth(1).unwrap();
    let mem = Gauge {
        total: 34359738368,
        used: 20307083264,
    };
    assert_eq!(
        sampler::macos_mem(&fixture("macos-vm_stat.txt"), memsize),
        Some(mem)
    );
    let swap = Gauge {
        total: 2147483648,
        used: 718400389,
    };
    assert_eq!(sampler::macos_swap(&sysctl), Some(swap));
    let disk = DiskUsage {
        used: 423378432000,
        free: 70953934848,
    };
    assert_eq!(
        sampler::df(&fixture("macos-df.txt")),
        Some(disk),
        "size minus available, not APFS's Used"
    );
}

#[test]
fn cpu_pct_is_none_when_no_tick_passed_or_a_counter_went_back() {
    let at = |busy, total| CpuTicks { busy, total };
    assert_eq!(sampler::cpu_pct(at(10, 100), at(10, 100)), None);
    assert_eq!(
        sampler::cpu_pct(at(10, 100), at(5, 200)),
        None,
        "busy wrapped"
    );
    assert_eq!(sampler::cpu_pct(at(10, 100), at(60, 200)), Some(50.0));
}

#[test]
fn garbage_parses_to_none() {
    for text in ["", "nope", "{ 1 2 }\n"] {
        assert_eq!(sampler::linux_load(text), None);
        assert_eq!(sampler::linux_cpu(text), None);
        assert_eq!(sampler::linux_mem(text), None);
        assert_eq!(sampler::macos_load(text), None);
        assert_eq!(sampler::macos_swap(text), None);
        assert_eq!(sampler::df(text), None);
    }
}

#[test]
fn sample_on_this_os_reads_load_memory_and_root_and_cpu_from_the_second() {
    let (root, gone) = (
        MountPath::try_from("/".to_string()).unwrap(),
        MountPath::try_from("/no/such/mount".to_string()).unwrap(),
    );
    let mut sampler = Sampler::new();
    let first = sampler.sample(1, &[root.clone(), gone.clone()]);
    assert!(
        first.load.is_some() && first.mem.is_some() && first.swap.is_some(),
        "{first:?}"
    );
    assert_eq!(first.cpu_pct, None);
    assert!(
        first.disks[&root].free > 0 && !first.disks.contains_key(&gone),
        "{first:?}"
    );
    std::thread::sleep(std::time::Duration::from_millis(200));
    let second = sampler.sample(2, &[]);
    assert!(
        second.cpu_pct.is_some_and(|c| (0.0..=100.0).contains(&c)),
        "{second:?}"
    );
    assert!(second.disks.is_empty());
}
