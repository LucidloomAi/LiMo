//! LiMo spike 04: can a phone find the master PC on the home network by
//! itself, and talk to it? Throwaway code; see README.md and TESTING.md.
//!
//! Exit codes: 0 success, 1 an operation failed, 2 bad arguments.

mod args;
mod browse;
mod client;
mod clock;
mod netinfo;
mod out;
mod protocol;
mod serve;
mod status;

use std::process::ExitCode;

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let code = match args::parse(&argv) {
        Ok(args::Command::Help) => {
            print!("{}", args::USAGE);
            0
        }
        Ok(args::Command::Serve(a)) => serve::run(&a),
        Ok(args::Command::Browse { seconds }) => browse::run(seconds),
        Ok(args::Command::Hello { host, port }) => client::run(&host, port),
        Ok(args::Command::Status) => status::run(),
        Err(msg) => {
            eprintln!("error: {msg}");
            eprintln!();
            eprint!("{}", args::USAGE);
            2
        }
    };
    ExitCode::from(code)
}
