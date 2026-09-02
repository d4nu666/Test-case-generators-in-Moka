use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OsSpecific {
    Unified(String),
    UnixWin {
        unix: String,
        win: String,
    },
    LinuxMacWin {
        linux: String,
        mac: String,
        win: String,
    },
}

impl OsSpecific {
    pub fn get(&self) -> &str {
        match self {
            OsSpecific::Unified(run) => run,
            OsSpecific::UnixWin { unix, win } => {
                if cfg!(windows) {
                    win
                } else {
                    unix
                }
            }
            OsSpecific::LinuxMacWin { linux, mac, win } => {
                if cfg!(windows) {
                    win
                } else if cfg!(target_os = "macos") {
                    mac
                } else {
                    linux
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunOption {
    pub run: OsSpecific,
    #[serde(default)]
    pub compile: Option<OsSpecific>,
    #[serde(default)]
    pub watch: Vec<String>,
    #[serde(default)]
    pub ignore: Vec<String>,
}

impl RunOption {
    pub fn run(&self) -> &str {
        self.run.get()
    }
    pub fn compile(&self) -> Option<&str> {
        self.compile.as_ref().map(|c| c.get())
    }
}
