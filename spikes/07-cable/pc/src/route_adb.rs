//! The comparison: Google's own adb.exe, run as a separate program, the way
//! LiMo's phone development tool runs it.
//!
//! adb.exe starts a server that keeps running after the call that started
//! it and listens on 127.0.0.1:5037, where any program on this PC could
//! drive the phone while it runs. So every step by this route ends with
//! `adb kill-server`, and then looks that nothing listens there any more.
//!
//! The serial number adb names the phone by stays in this program's memory
//! and in adb's command line (`-s`); on screen only its last four
//! characters are shown, and it is written to no file.

#![forbid(unsafe_code)]

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use crate::cable::{Answer, Cable, CableError, INSTALL_LIMIT, OpenProblem, SHELL_LIMIT, Via};
use crate::phone::{Ask, PACKAGE, masked_serial};
use crate::route_libusb::adb_server_running;
use crate::ui::Ui;

/// Google's adb.exe, found.
#[derive(Debug, Clone)]
pub struct AdbExe {
    path: PathBuf,
}

/// What one run of adb.exe gave.
#[derive(Debug, Clone, Default)]
pub struct Ran {
    /// Its exit code; None when it was stopped.
    pub code: Option<i32>,
    /// What it printed on its standard output.
    pub out: String,
    /// What it printed on its standard error.
    pub err: String,
}

/// The program's file name.
fn exe_name() -> &'static str {
    if cfg!(windows) { "adb.exe" } else { "adb" }
}

impl AdbExe {
    /// Finds adb: the path given, else on the PATH, else in `platform-tools`
    /// under ANDROID_HOME, ANDROID_SDK_ROOT or `%LOCALAPPDATA%\Android\Sdk`.
    pub fn find(given: Option<&Path>) -> Option<AdbExe> {
        if let Some(path) = given {
            return path.is_file().then(|| AdbExe { path: path.to_path_buf() });
        }
        let mut places: Vec<PathBuf> = std::env::var_os("PATH")
            .map(|p| std::env::split_paths(&p).collect())
            .unwrap_or_default();
        for var in ["ANDROID_HOME", "ANDROID_SDK_ROOT"] {
            if let Some(root) = std::env::var_os(var) {
                places.push(PathBuf::from(root).join("platform-tools"));
            }
        }
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            places.push(PathBuf::from(local).join("Android").join("Sdk").join("platform-tools"));
        }
        places.into_iter().map(|p| p.join(exe_name())).find(|p| p.is_file()).map(|path| AdbExe { path })
    }

    /// Runs adb with `args`, `input` on its standard input if given, for at
    /// most `limit`.
    pub fn run(&self, args: &[&str], input: Option<&[u8]>, limit: Duration) -> Result<Ran, CableError> {
        let mut child = Command::new(&self.path)
            .args(args)
            .stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| CableError::Failed(format!("adb could not be started: {e}")))?;
        if let (Some(bytes), Some(mut stdin)) = (input, child.stdin.take()) {
            let bytes = zeroize::Zeroizing::new(bytes.to_vec());
            std::thread::spawn(move || {
                let _ = stdin.write_all(&bytes);
            });
        }
        let out = reader(child.stdout.take());
        let err = reader(child.stderr.take());
        let code = wait(&mut child, limit)?;
        Ok(Ran { code, out: out.join().unwrap_or_default(), err: err.join().unwrap_or_default() })
    }

    /// Starts adb's server with no pipes at all, so that it can never hold
    /// this program's output open.
    pub fn start_server(&self) -> Result<(), CableError> {
        let mut child = Command::new(&self.path)
            .arg("start-server")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| CableError::Failed(format!("adb could not be started: {e}")))?;
        wait(&mut child, SHELL_LIMIT).map(|_| ())
    }

    /// Stops adb's server, and says whether anything still listens on its
    /// port.
    pub fn kill_server(&self) -> Vec<String> {
        let mut said = vec!["> adb kill-server".to_string()];
        if let Err(e) = self.run(&["kill-server"], None, SHELL_LIMIT) {
            said.push(format!("adb kill-server: {e}."));
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while adb_server_running() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(200));
        }
        said.push(if adb_server_running() {
            "Something still listens on 127.0.0.1:5037 after adb kill-server: adb's server, or another program's. \
             Write that down, and stop it before the next step."
                .to_string()
        } else {
            "adb's server is stopped: nothing listens on 127.0.0.1:5037.".to_string()
        });
        said
    }

    /// The phones adb lists: each serial with its state.
    pub fn devices(&self) -> Result<Vec<(String, String)>, CableError> {
        let ran = self.run(&["devices"], None, SHELL_LIMIT)?;
        Ok(devices(&ran.out))
    }
}

/// Reads `adb devices`: one `SERIAL<tab>STATE` line a phone, after its first
/// line; adb's own notes (starting with `*`) are left out.
pub fn devices(text: &str) -> Vec<(String, String)> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('*') && !l.starts_with("List of devices"))
        .filter_map(|l| {
            let mut parts = l.split_whitespace();
            Some((parts.next()?.to_string(), parts.next()?.to_string()))
        })
        .collect()
}

fn reader<R: Read + Send + 'static>(stream: Option<R>) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut stream) = stream {
            let _ = stream.read_to_end(&mut bytes);
        }
        String::from_utf8_lossy(&bytes).into_owned()
    })
}

fn wait(child: &mut Child, limit: Duration) -> Result<Option<i32>, CableError> {
    let deadline = Instant::now() + limit;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status.code()),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(CableError::Timeout(limit));
            }
            Err(e) => return Err(CableError::Failed(format!("adb could not be waited for: {e}"))),
        }
    }
}

/// The phone through adb.exe, held for one step.
pub struct AdbCable {
    exe: AdbExe,
    serial: String,
}

impl AdbCable {
    fn shell_args<'a>(&'a self, command: &'a str) -> [&'a str; 5] {
        ["-s", self.serial.as_str(), "shell", "-T", command]
    }
}

/// adb's answer for an install or a removal: `Success`, or the line that
/// holds the failure, or what it printed.
pub fn android_answer(ran: &Ran) -> String {
    let all = format!("{}\n{}", ran.out, ran.err);
    let lines: Vec<&str> = all.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    if ran.code == Some(0) && lines.contains(&"Success") {
        return "Success".to_string();
    }
    lines
        .iter()
        .find(|l| l.contains("Failure") || l.contains("failed"))
        .map(|l| l.to_string())
        .unwrap_or_else(|| lines.join(" "))
}

impl Cable for AdbCable {
    fn via(&self) -> Via {
        Via::Adb
    }

    fn shell(&self, ask: &Ask) -> Result<Answer, CableError> {
        let command = ask.command();
        let ran = self.exe.run(&self.shell_args(&command), None, SHELL_LIMIT)?;
        Ok(Answer { text: format!("{}{}", ran.out, ran.err), exit: ran.code })
    }

    fn shell_with_input(&self, ask: &Ask, input: &[u8]) -> Result<Answer, CableError> {
        let command = ask.command();
        let ran = self.exe.run(&self.shell_args(&command), Some(input), SHELL_LIMIT)?;
        Ok(Answer { text: format!("{}{}", ran.out, ran.err), exit: ran.code })
    }

    fn install(&self, apk: &Path) -> Result<String, CableError> {
        let path = apk.to_string_lossy().into_owned();
        let ran = self.exe.run(&["-s", self.serial.as_str(), "install", "-r", &path], None, INSTALL_LIMIT)?;
        Ok(android_answer(&ran))
    }

    fn uninstall(&self) -> Result<String, CableError> {
        let ran = self.exe.run(&["-s", self.serial.as_str(), "uninstall", PACKAGE], None, SHELL_LIMIT)?;
        Ok(android_answer(&ran))
    }

    fn reverse(&self, port: u16) -> Result<(), CableError> {
        let spec = format!("tcp:{port}");
        let ran = self.exe.run(&["-s", self.serial.as_str(), "reverse", &spec, &spec], None, SHELL_LIMIT)?;
        if ran.code == Some(0) { Ok(()) } else { Err(CableError::Failed(format!("{}{}", ran.out, ran.err).trim().to_string())) }
    }

    fn reverse_remove(&self, port: u16) -> Result<(), CableError> {
        let spec = format!("tcp:{port}");
        let ran = self.exe.run(&["-s", self.serial.as_str(), "reverse", "--remove", &spec], None, SHELL_LIMIT)?;
        if ran.code == Some(0) { Ok(()) } else { Err(CableError::Failed(format!("{}{}", ran.out, ran.err).trim().to_string())) }
    }

    fn close(self: Box<Self>) -> Vec<String> {
        self.exe.kill_server()
    }
}

/// Opens the one phone through adb.exe.
pub fn open(exe: Option<AdbExe>, ui: &mut dyn Ui) -> Result<Box<dyn Cable>, OpenProblem> {
    let exe = exe.ok_or(OpenProblem::AdbMissing)?;
    ui.say("> adb start-server (it runs until this step ends, when this program stops it)");
    exe.start_server().map_err(|e| OpenProblem::Other(format!("adb start-server: {e}")))?;
    ui.say("> adb devices");
    let listed = match exe.devices() {
        Ok(listed) => listed,
        Err(e) => {
            for line in exe.kill_server() {
                ui.say(&line);
            }
            return Err(OpenProblem::Other(format!("adb devices: {e}")));
        }
    };
    for (serial, state) in &listed {
        ui.say(&format!("adb lists {} as {state}", masked_serial(serial)));
    }
    let problem = match listed.as_slice() {
        [] => Some(OpenProblem::NoPhone),
        [(_, state)] if state == "device" => None,
        [(_, state)] if state == "unauthorized" => {
            Some(OpenProblem::NotAllowed("adb lists the phone as unauthorized".to_string()))
        }
        [(_, state)] => Some(OpenProblem::Other(format!("adb lists the phone as {state}"))),
        more => Some(OpenProblem::TwoPhones(more.len())),
    };
    if let Some(problem) = problem {
        for line in exe.kill_server() {
            ui.say(&line);
        }
        return Err(problem);
    }
    let serial = listed[0].0.clone();
    ui.say(&format!("Every call below goes as adb -s {} ..., the shell ones as adb -s {} shell -T COMMAND.", masked_serial(&serial), masked_serial(&serial)));
    Ok(Box::new(AdbCable { exe, serial }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adb_devices_is_read_with_its_notes_left_out() {
        let text = "* daemon not running; starting now at tcp:5037\r\n* daemon started successfully\r\nList of devices attached\r\nR5CN20ABCD1A2B\tdevice\r\nemulator-5554\tunauthorized\r\n\r\n";
        assert_eq!(
            devices(text),
            vec![
                ("R5CN20ABCD1A2B".to_string(), "device".to_string()),
                ("emulator-5554".to_string(), "unauthorized".to_string())
            ]
        );
        assert!(devices("List of devices attached\r\n\r\n").is_empty());
    }

    #[test]
    fn android_answers_are_read_from_either_stream() {
        let ok = Ran { code: Some(0), out: "Performing Streamed Install\r\nSuccess\r\n".to_string(), err: String::new() };
        assert_eq!(android_answer(&ok), "Success");
        let refused = Ran {
            code: Some(1),
            out: "Performing Streamed Install\r\n".to_string(),
            err: "adb: failed to install x.apk: Failure [INSTALL_FAILED_UPDATE_INCOMPATIBLE: signatures do not match]\r\n"
                .to_string(),
        };
        assert!(android_answer(&refused).contains("INSTALL_FAILED_UPDATE_INCOMPATIBLE"));
        let gone = Ran { code: Some(1), out: "Failure [DELETE_FAILED_INTERNAL_ERROR]\r\n".to_string(), err: String::new() };
        assert_eq!(android_answer(&gone), "Failure [DELETE_FAILED_INTERNAL_ERROR]");
    }
}
