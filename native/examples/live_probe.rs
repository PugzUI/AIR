//! Probes a real AHP host with the console's own client.
//!
//! `cargo run --example live_probe -- <ws://endpoint> <token-file>` starts the
//! same `LiveFleet` worker the console starts, adds the host, and prints what
//! the console would draw: the connected hosts, their sessions, and the graph
//! the topology is built from.

use relay_native::example;
use relay_native::live::{self, LiveFleet};
use std::time::{Duration, Instant};

fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(endpoint), Some(token_path)) = (args.next(), args.next()) else {
        eprintln!("usage: live_probe <ws://endpoint> <token-file>");
        std::process::exit(2);
    };
    let token = std::fs::read_to_string(&token_path)
        .expect("read the token file")
        .trim()
        .to_owned();

    let fleet = LiveFleet::start().expect("start the AHP worker");
    fleet.add("probe", &endpoint, &token).expect("add the host");

    let deadline = Instant::now() + Duration::from_secs(30);
    let mut hosts = Vec::new();
    while Instant::now() < deadline {
        match fleet.try_event() {
            Ok(live::Event::Hosts(next)) => {
                let settled = next
                    .iter()
                    .any(|host| host.status == "connected" || host.last_error.is_some());
                hosts = next;
                if settled {
                    break;
                }
            }
            Ok(_) => {}
            Err(_) => std::thread::sleep(Duration::from_millis(25)),
        }
    }
    if hosts.is_empty() {
        eprintln!("no host state arrived from {endpoint} within 30s");
        std::process::exit(1);
    }
    for host in &hosts {
        if let Some(error) = &host.last_error {
            eprintln!("host {} reported: {error}", host.label);
        }
    }

    let sessions = live::fleet_sessions(&hosts);
    let graph = live::graph(&hosts, "");
    let connected = hosts
        .iter()
        .filter(|host| host.status == "connected")
        .count();
    println!(
        "hosts {} (connected {}) · sessions {} · graph nodes {} edges {} ({}x{})",
        hosts.len(),
        connected,
        sessions.len(),
        graph.nodes.len(),
        graph.edges.len(),
        graph.width,
        graph.height
    );
    for host in &hosts {
        println!(
            "  host {} [{}] protocol {} agents {} sessions {}",
            host.label,
            host.status,
            host.protocol_version.as_deref().unwrap_or("—"),
            host.agents.len(),
            host.sessions.len()
        );
    }
    for (host, session) in sessions.iter().take(8) {
        println!(
            "  session {} · {} · {}",
            session.title, session.provider, host.label
        );
    }
    let providers = example::advertised_agents(&hosts[0].id).count();
    println!("  example catalog providers for the first host: {providers}");
}
