use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use base::{
    PORT,
    connection::{ClientIdentity, ClientKind, DEFAULT_USERNAME},
};

pub const HELP: &str = "Commands:
  connect [USERNAME][@IP[:PORT]]  Connect (default: spoon@127.0.0.1:5000)
  peek GENERAL_NAME             Show all synchronized Human components
  disconnect                    Close the current connection
  help                          Show this help
  exit                          Close the connection and exit

Examples:
  connect
  connect alice
  connect alice@192.168.1.10
  connect alice@192.168.1.10:5000
  connect alice@[::1]:5000
  peek 1";

#[derive(Debug)]
pub enum Command {
    Connect(ConnectionTarget),
    Peek(usize),
    Disconnect,
    Help,
    Exit,
}

#[derive(Debug, Clone)]
pub struct ConnectionTarget {
    pub identity: ClientIdentity,
    pub address: SocketAddr,
}

impl std::fmt::Display for ConnectionTarget {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}@{}", self.identity.username(), self.address)
    }
}

pub fn parse(line: &str) -> Result<Option<Command>, String> {
    let mut words = line.split_whitespace();
    let Some(name) = words.next() else {
        return Ok(None);
    };
    let argument = words.next();
    if words.next().is_some() {
        return Err("Too many arguments. Type help for command syntax.".into());
    }
    let command = match (name, argument) {
        ("connect", argument) => Command::Connect(parse_target(argument.unwrap_or(""))?),
        ("peek", Some(value)) => Command::Peek(
            value
                .parse()
                .map_err(|_| "GeneralName must be a nonnegative integer (example: peek 1)")?,
        ),
        ("disconnect", None) => Command::Disconnect,
        ("help", None) => Command::Help,
        ("exit", None) => Command::Exit,
        ("peek", None) => return Err("Usage: peek GENERAL_NAME".into()),
        ("disconnect" | "help" | "exit", Some(_)) => {
            return Err(format!("{name} does not accept arguments"));
        }
        _ => return Err(format!("Unknown command: {name}. Type help for commands.")),
    };
    Ok(Some(command))
}

fn parse_target(value: &str) -> Result<ConnectionTarget, String> {
    let (username, endpoint) = value
        .split_once('@')
        .map_or((value, None), |(username, endpoint)| {
            (username, Some(endpoint))
        });
    let identity = ClientIdentity::new(
        if username.is_empty() {
            DEFAULT_USERNAME
        } else {
            username
        },
        ClientKind::Console,
    )?;
    let address = match endpoint {
        None | Some("") => SocketAddr::from((Ipv4Addr::LOCALHOST, PORT)),
        Some(endpoint) => parse_address(endpoint)?,
    };
    if address.port() == 0 || address.ip().is_unspecified() || address.ip().is_multicast() {
        return Err("Use a unicast IP and a nonzero port".into());
    }
    Ok(ConnectionTarget { identity, address })
}

fn parse_address(endpoint: &str) -> Result<SocketAddr, String> {
    if let Some(port) = endpoint
        .strip_prefix(':')
        .filter(|port| !port.contains(':'))
    {
        return Ok(SocketAddr::from((
            Ipv4Addr::LOCALHOST,
            if port.is_empty() {
                PORT
            } else {
                port.parse().map_err(|_| "Invalid port")?
            },
        )));
    }
    let endpoint = endpoint.strip_suffix(':').unwrap_or(endpoint);
    if let Ok(address) = endpoint.parse::<SocketAddr>() {
        return Ok(address);
    }
    let ip = endpoint
        .strip_prefix('[')
        .and_then(|address| address.strip_suffix(']'))
        .unwrap_or(endpoint);
    if let Ok(ip) = ip.parse::<IpAddr>() {
        if ip.is_ipv4() || endpoint.starts_with('[') {
            return Ok(SocketAddr::new(ip, PORT));
        }
    }
    Err("Invalid address. Use IP[:PORT], or [IPv6][:PORT]".into())
}
