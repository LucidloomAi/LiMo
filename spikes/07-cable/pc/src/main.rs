//! limo-cable-spike.exe: one step of the cable proof a run. See README.md.

#![forbid(unsafe_code)]

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = limo_cable_spike::run(&args, &mut limo_cable_spike::ui::Terminal);
    std::process::exit(code);
}
