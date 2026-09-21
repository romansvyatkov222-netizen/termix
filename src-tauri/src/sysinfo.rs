//! Remote host stats: CPU / RAM / disks / uptime of the connected VDS.
//!
//! One short-lived exec channel per call (same pattern as `ssh_home_via_exec`
//! and `archive_create` in `sftp.rs`): the terminal pty channel and the SFTP
//! subsystem are untouched. POSIX/Linux only — `/proc`, `df`, `free` —
//! anything else degrades field-by-field (`None`) instead of failing whole.

use crate::errors::err_code;
use crate::models::{DiskUsage, SystemStats};
use crate::state::AppState;
use tauri::State;
use tokio::io::AsyncReadExt;

// Keep in sync with the `===" markers parsed below.
const STATS_SCRIPT: &str = r#"echo "===HOST==="; hostname
echo "===OS==="; (grep '^PRETTY_NAME=' /etc/os-release 2>/dev/null | cut -d= -f2- | tr -d '"'; uname -srm)
echo "===UPTIME==="; cat /proc/uptime 2>/dev/null
echo "===CPU==="; nproc 2>/dev/null; cat /proc/loadavg 2>/dev/null
echo "===CPUSTAT1==="; grep '^cpu ' /proc/stat 2>/dev/null
sleep 0.5
echo "===CPUSTAT2==="; grep '^cpu ' /proc/stat 2>/dev/null
echo "===MEM==="; grep -E '^(MemTotal|MemFree|MemAvailable|Buffers|Cached):' /proc/meminfo 2>/dev/null
echo "===DF==="; df -B1 -P -x tmpfs -x devtmpfs -x overlay 2>/dev/null || df -B1 -P 2>/dev/null
echo "===END===""#;

async fn run_stats_script(state: &State<'_, AppState>) -> Result<String, String> {
    let handle = {
        let guard = state.conn.lock().await;
        let conn = guard
            .as_ref()
            .ok_or_else(|| err_code::NOT_CONNECTED.to_string())?;
        conn.handle.clone()
    };
    let channel = {
        let h = handle.lock().await;
        h.channel_open_session()
            .await
            .map_err(|_| err_code::STATS_NO_SHELL.to_string())?
    };
    channel
        .exec(true, STATS_SCRIPT)
        .await
        .map_err(|_| err_code::STATS_NO_SHELL.to_string())?;
    let mut stream = channel.into_stream();
    let mut buf = Vec::new();
    tokio::time::timeout(
        std::time::Duration::from_secs(12),
        stream.read_to_end(&mut buf),
    )
    .await
    .map_err(|_| err_code::STATS_FAILED.to_string())?
    .map_err(|e| e.to_string())?;
    let out = String::from_utf8_lossy(&buf).to_string();
    if out.trim().is_empty() {
        return Err(err_code::STATS_FAILED.to_string());
    }
    Ok(out)
}

/// Split script output into `marker -> lines` sections.
fn sections(out: &str) -> std::collections::HashMap<&str, Vec<&str>> {
    let mut map: std::collections::HashMap<&str, Vec<&str>> = std::collections::HashMap::new();
    let mut cur: Option<&str> = None;
    for line in out.lines() {
        let t = line.trim();
        if t.starts_with("===") && t.ends_with("===") && t.len() > 6 {
            cur = Some(match &t[3..t.len() - 3] {
                "HOST" => "HOST",
                "OS" => "OS",
                "UPTIME" => "UPTIME",
                "CPU" => "CPU",
                "CPUSTAT1" => "CPUSTAT1",
                "CPUSTAT2" => "CPUSTAT2",
                "MEM" => "MEM",
                "DF" => "DF",
                _ => continue,
            });
            map.entry(cur.unwrap_or("?")).or_default();
        } else if let Some(k) = cur {
            map.entry(k).or_default().push(line);
        }
    }
    map
}

fn parse_u64(s: &str) -> Option<u64> {
    s.parse::<u64>().ok()
}

fn parse_f64(s: &str) -> Option<f64> {
    s.parse::<f64>().ok()
}

/// `%cpu` from two `/proc/stat` snapshots: 100 * busy_delta / total_delta.
fn cpu_percent(s1: &str, s2: &str) -> Option<f64> {
    let nums = |l: &str| -> Option<Vec<u64>> {
        let mut it = l.split_whitespace();
        if it.next()? != "cpu" {
            return None;
        }
        it.map(parse_u64).collect::<Option<Vec<u64>>>()
    };
    let a = nums(s1.trim())?;
    let b = nums(s2.trim())?;
    if a.len() < 5 || b.len() < 5 || a.len() != b.len() {
        return None;
    }
    // idle = idle + iowait (fields 4,5); total = sum of all.
    let idle_a = a[3] + a.get(4).copied().unwrap_or(0);
    let idle_b = b[3] + b.get(4).copied().unwrap_or(0);
    let total_a: u64 = a.iter().sum();
    let total_b: u64 = b.iter().sum();
    if total_b <= total_a {
        return None;
    }
    let busy = (total_b - total_a).saturating_sub(idle_b.saturating_sub(idle_a));
    Some(busy as f64 / (total_b - total_a) as f64 * 100.0)
}

fn parse_meminfo(lines: &[&str]) -> (Option<u64>, Option<u64>) {
    let mut total = None;
    let mut avail: Option<u64> = None;
    let mut free = None;
    let mut buffers = None;
    let mut cached = None;
    for l in lines {
        let mut it = l.split_whitespace();
        let key = it.next().unwrap_or("");
        let val = it.next().and_then(parse_u64);
        match key {
            "MemTotal:" => total = val.map(|v| v * 1024),
            "MemAvailable:" => avail = val.map(|v| v * 1024),
            "MemFree:" => free = val.map(|v| v * 1024),
            "Buffers:" => buffers = val.map(|v| v * 1024),
            "Cached:" => cached = val.map(|v| v * 1024),
            _ => {}
        }
    }
    // Old kernels lack MemAvailable: free + buffers + cached approximation.
    let available = avail.or_else(|| match (free, buffers, cached) {
        (Some(f), b, c) => Some(f + b.unwrap_or(0) + c.unwrap_or(0)),
        _ => None,
    });
    let used = match (total, available) {
        (Some(t), Some(a)) => Some(t.saturating_sub(a)),
        _ => None,
    };
    (total, used)
}

fn parse_df(lines: &[&str]) -> Vec<DiskUsage> {
    let mut out = Vec::new();
    for l in lines {
        let l = l.trim();
        if l.is_empty() || l.starts_with("Filesystem") {
            continue;
        }
        // `df -P`: 6 columns, mount point last (may contain spaces — join rest).
        let parts: Vec<&str> = l.split_whitespace().collect();
        if parts.len() < 6 {
            continue;
        }
        let (total, used, avail) = match (
            parse_u64(parts[1]),
            parse_u64(parts[2]),
            parse_u64(parts[3]),
        ) {
            (Some(t), Some(u), Some(a)) => (t, u, a),
            _ => continue,
        };
        let use_pct = parts[4].trim_end_matches('%').parse::<f64>().unwrap_or(0.0);
        let mount = parts[5..].join(" ");
        out.push(DiskUsage {
            mount,
            total,
            used,
            avail,
            use_pct,
        });
    }
    out
}

#[tauri::command]
pub(crate) async fn system_stats(state: State<'_, AppState>) -> Result<SystemStats, String> {
    let out = run_stats_script(&state).await?;
    let sec = sections(&out);

    let hostname = sec
        .get("HOST")
        .and_then(|v| v.iter().find(|l| !l.trim().is_empty()))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let os_lines: Vec<&str> = sec.get("OS").cloned().unwrap_or_default();
    let os_pretty = os_lines
        .iter()
        .find(|l| !l.trim().is_empty())
        .map(|s| s.trim().to_string());
    let kernel = os_lines
        .iter()
        .skip(1)
        .find(|l| !l.trim().is_empty())
        .map(|s| s.trim().to_string());

    let uptime_secs = sec
        .get("UPTIME")
        .and_then(|v| v.first())
        .and_then(|l| l.split_whitespace().next())
        .and_then(parse_f64)
        .map(|f| f as u64);

    let cpu_lines: Vec<&str> = sec.get("CPU").cloned().unwrap_or_default();
    let cpu_count = cpu_lines.first().and_then(|l| l.trim().parse::<u32>().ok());
    let (load1, load5, load15) = cpu_lines
        .get(1)
        .map(|l| {
            let mut it = l.split_whitespace();
            (
                it.next().and_then(parse_f64),
                it.next().and_then(parse_f64),
                it.next().and_then(parse_f64),
            )
        })
        .unwrap_or((None, None, None));

    let cpu_percent = match (
        sec.get("CPUSTAT1").and_then(|v| v.first()),
        sec.get("CPUSTAT2").and_then(|v| v.first()),
    ) {
        (Some(a), Some(b)) => cpu_percent(a, b),
        _ => None,
    };

    let (mem_total, mem_used) = parse_meminfo(&sec.get("MEM").cloned().unwrap_or_default());
    let disks = parse_df(&sec.get("DF").cloned().unwrap_or_default());

    Ok(SystemStats {
        hostname,
        os_pretty,
        kernel,
        uptime_secs,
        cpu_count,
        load1,
        load5,
        load15,
        cpu_percent,
        mem_total,
        mem_used,
        disks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "===HOST===\nvds-1\n===OS===\nUbuntu 22.04.5 LTS\nLinux 5.15.0 x86_64\n===UPTIME===\n12345.67 23456.78\n===CPU===\n4\n1.25 0.80 0.45 2/123 456\n===CPUSTAT1===\ncpu  100 0 200 700 0 0 0 0 0 0\n===CPUSTAT2===\ncpu  150 0 250 800 0 0 0 0 0 0\n===MEM===\nMemTotal:        4024548 kB\nMemFree:          500000 kB\nMemAvailable:    3000000 kB\nBuffers:           50000 kB\nCached:           800000 kB\n===DF===\nFilesystem 1B-blocks Used Available Use% Mounted on\n/dev/vda1 100000000 40000000 60000000 40% /\n/dev/vda2 50000000 10000000 40000000 20% /home\n===END===\n";

    #[test]
    fn parses_sample() {
        let sec = sections(SAMPLE);
        assert_eq!(sec.get("HOST").unwrap()[0].trim(), "vds-1");
        // busy 100 / total 200 => 50%
        let pct = cpu_percent(
            sec.get("CPUSTAT1").unwrap()[0],
            sec.get("CPUSTAT2").unwrap()[0],
        )
        .unwrap();
        assert!((pct - 50.0).abs() < 0.01);
        let (total, used) = parse_meminfo(&sec.get("MEM").cloned().unwrap());
        assert_eq!(total, Some(4024548 * 1024));
        assert_eq!(used, Some((4024548 - 3000000) * 1024));
        let disks = parse_df(&sec.get("DF").cloned().unwrap());
        assert_eq!(disks.len(), 2);
        assert_eq!(disks[0].mount, "/");
        assert!((disks[0].use_pct - 40.0).abs() < 0.01);
    }

    #[test]
    fn meminfo_fallback_without_available() {
        let lines = vec![
            "MemTotal: 1000 kB",
            "MemFree: 400 kB",
            "Buffers: 100 kB",
            "Cached: 200 kB",
        ];
        let (total, used) = parse_meminfo(&lines);
        assert_eq!(total, Some(1000 * 1024));
        assert_eq!(used, Some(300 * 1024));
    }

    #[test]
    fn empty_output_sections() {
        let sec = sections("garbage\n===END===\n");
        assert!(sec.get("HOST").map(|v| v.is_empty()).unwrap_or(true));
        assert!(parse_df(&[]).is_empty());
    }
}
