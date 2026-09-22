use std::collections::{HashMap, HashSet};
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcInfo {
    pub pid: u32,
    pub command: String,
}

pub trait ProcessTable: Send + Sync {
    fn is_alive(&self, pid: u32) -> bool;
    fn list(&self) -> Vec<ProcInfo>;
    fn cwd(&self, pid: u32) -> Option<String>;
}

pub struct SystemProcessTable;

impl ProcessTable for SystemProcessTable {
    fn is_alive(&self, pid: u32) -> bool {
        Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "pid="])
            .output()
            .map(|o| o.status.success() && !o.stdout.is_empty())
            .unwrap_or(false)
    }

    fn list(&self) -> Vec<ProcInfo> {
        let Ok(out) = Command::new("ps").args(["-axo", "pid=,command="]).output() else {
            return vec![];
        };
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|line| {
                let line = line.trim_start();
                let (pid, command) = line.split_once(char::is_whitespace)?;
                Some(ProcInfo { pid: pid.parse().ok()?, command: command.trim().to_string() })
            })
            .collect()
    }

    fn cwd(&self, pid: u32) -> Option<String> {
        let out = Command::new("lsof")
            .args(["-a", "-d", "cwd", "-p", &pid.to_string(), "-Fn"])
            .output()
            .ok()?;
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .find_map(|l| l.strip_prefix('n').map(str::to_string))
    }
}

#[derive(Default)]
pub struct FakeProcessTable {
    pub alive: HashSet<u32>,
    pub procs: Vec<ProcInfo>,
    pub cwds: HashMap<u32, String>,
}

impl ProcessTable for FakeProcessTable {
    fn is_alive(&self, pid: u32) -> bool {
        self.alive.contains(&pid)
    }
    fn list(&self) -> Vec<ProcInfo> {
        self.procs.clone()
    }
    fn cwd(&self, pid: u32) -> Option<String> {
        self.cwds.get(&pid).cloned()
    }
}
