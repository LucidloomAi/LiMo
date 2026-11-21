//! The echo server on this PC's own loopback, for `reverse`: what the test
//! app on the phone sends to 127.0.0.1:47810 on the phone comes, through the
//! cable, to 127.0.0.1:47810 here, and goes back as it came.
//!
//! It listens on 127.0.0.1 only (never on the home network, so Windows'
//! firewall is not asked), takes one connection, sends back every byte as it
//! comes, and keeps the connection until the phone closes it, within a time
//! limit; then it stops listening. It never closes first: adb's streams
//! have no half close, so a side that stops sending ends the whole stream,
//! and bytes still on their way back are lost (which the first run on the
//! phone showed, as `cut_short`). The test app therefore never stops
//! sending to say it is done: it reads every byte back, then closes.

#![forbid(unsafe_code)]

use std::io::{ErrorKind, Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::time::{Duration, Instant};

/// What one connection did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Echoed {
    /// How many bytes came and went back.
    pub bytes: u64,
    /// How long the connection lasted.
    pub took: Duration,
}

/// The echo server, listening.
#[derive(Debug)]
pub struct Echo {
    listener: TcpListener,
}

impl Echo {
    /// Listens on 127.0.0.1 at `port`. The error says, in words, why not
    /// (most often: another program listens there).
    pub fn bind(port: u16) -> Result<Echo, String> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port)).map_err(|e| {
            if e.kind() == ErrorKind::AddrInUse {
                format!("Another program on this PC already listens on 127.0.0.1:{port}, so the echo server cannot.")
            } else {
                format!("The echo server could not listen on 127.0.0.1:{port}: {e}.")
            }
        })?;
        listener.set_nonblocking(true).map_err(|e| format!("The echo server could not be set up: {e}."))?;
        Ok(Echo { listener })
    }

    /// The port it listens on.
    pub fn port(&self) -> u16 {
        self.listener.local_addr().map(|a| a.port()).unwrap_or(0)
    }

    /// Waits up to `wait` for one connection, echoes it to its end (at most
    /// `limit` for the whole connection), and stops listening. None when no
    /// connection came in time.
    pub fn serve_one(self, wait: Duration, limit: Duration) -> Result<Option<Echoed>, String> {
        let deadline = Instant::now() + wait;
        let stream = loop {
            match self.listener.accept() {
                Ok((stream, _)) => break stream,
                Err(e) if e.kind() == ErrorKind::WouldBlock => {
                    if Instant::now() >= deadline {
                        return Ok(None);
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(e) => return Err(format!("The echo server could not take the connection: {e}.")),
            }
        };
        drop(self.listener);
        echo(stream, limit).map(Some)
    }
}

/// Sends back every byte that comes, until the other side closes.
fn echo(mut stream: TcpStream, limit: Duration) -> Result<Echoed, String> {
    let started = Instant::now();
    stream.set_nonblocking(false).map_err(|e| e.to_string())?;
    stream.set_read_timeout(Some(limit)).map_err(|e| e.to_string())?;
    stream.set_write_timeout(Some(limit)).map_err(|e| e.to_string())?;
    let mut buffer = vec![0u8; 64 * 1024];
    let mut bytes = 0u64;
    loop {
        if started.elapsed() > limit {
            return Err(format!("The connection was still open after {} seconds.", limit.as_secs()));
        }
        let n = match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(e) if e.kind() == ErrorKind::TimedOut || e.kind() == ErrorKind::WouldBlock => {
                return Err(format!(
                    "The phone kept the connection open for {} seconds without closing it, after {bytes} bytes.",
                    limit.as_secs()
                ));
            }
            Err(e) => return Err(format!("Reading from the phone's connection failed after {bytes} bytes: {e}.")),
        };
        stream
            .write_all(&buffer[..n])
            .map_err(|e| format!("Writing back to the phone's connection failed after {bytes} bytes: {e}."))?;
        bytes += n as u64;
    }
    let _ = stream.flush();
    Ok(Echoed { bytes, took: started.elapsed() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Shutdown;

    #[test]
    fn what_comes_goes_back_and_then_it_stops_listening() {
        let echo = Echo::bind(0).unwrap();
        let port = echo.port();
        let server = std::thread::spawn(move || echo.serve_one(Duration::from_secs(5), Duration::from_secs(5)));
        let mut client = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
        let sent: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
        let mut writer = client.try_clone().unwrap();
        let to_send = sent.clone();
        let sender = std::thread::spawn(move || {
            writer.write_all(&to_send).unwrap();
            writer.shutdown(Shutdown::Write).unwrap();
        });
        let mut back = Vec::new();
        client.read_to_end(&mut back).unwrap();
        sender.join().unwrap();
        assert_eq!(back, sent);
        let echoed = server.join().unwrap().unwrap().unwrap();
        assert_eq!(echoed.bytes, sent.len() as u64);
        assert!(TcpStream::connect((Ipv4Addr::LOCALHOST, port)).is_err(), "no longer listening");
    }

    #[test]
    fn it_keeps_the_connection_until_the_phone_has_read_everything_back_and_closed() {
        let echo = Echo::bind(0).unwrap();
        let port = echo.port();
        let server = std::thread::spawn(move || echo.serve_one(Duration::from_secs(5), Duration::from_secs(5)));
        // As the test app: send, read every byte back without ever stopping
        // sending first, wait a while, then close.
        let mut client = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
        let sent: Vec<u8> = (0..100_000u32).map(|i| (i % 7) as u8).collect();
        let mut writer = client.try_clone().unwrap();
        let to_send = sent.clone();
        let sender = std::thread::spawn(move || writer.write_all(&to_send).unwrap());
        let mut back = vec![0u8; sent.len()];
        client.read_exact(&mut back).unwrap();
        sender.join().unwrap();
        assert_eq!(back, sent);
        std::thread::sleep(Duration::from_millis(300));
        drop(client);
        let echoed = server.join().unwrap().unwrap().unwrap();
        assert!(echoed.took >= Duration::from_millis(300), "the echo stayed until the phone closed");
        assert_eq!(echoed.bytes, sent.len() as u64);
    }

    #[test]
    fn a_phone_that_never_closes_is_said_after_the_limit() {
        let echo = Echo::bind(0).unwrap();
        let port = echo.port();
        let server = std::thread::spawn(move || echo.serve_one(Duration::from_secs(5), Duration::from_millis(500)));
        let _client = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
        let said = server.join().unwrap().unwrap_err();
        assert!(said.contains("without closing it"), "{said}");
    }

    #[test]
    fn a_port_in_use_is_said_and_no_connection_is_none() {
        let first = Echo::bind(0).unwrap();
        let taken = first.port();
        assert!(Echo::bind(taken).unwrap_err().contains("already listens"));
        assert_eq!(first.serve_one(Duration::from_millis(100), Duration::from_secs(1)).unwrap(), None);
    }
}
