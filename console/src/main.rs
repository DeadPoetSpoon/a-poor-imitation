mod command;
mod network;

use std::{
    io::{self, IsTerminal, Write},
    sync::mpsc,
    thread,
};

use command::Command;
use network::{Reply, Request};

fn exchange(sender: &mpsc::Sender<Request>, command: Option<Command>) -> io::Result<Reply> {
    let (reply_sender, receiver) = mpsc::channel();
    let request = match command {
        Some(command) => Request::Execute(command, reply_sender),
        None => Request::Status(reply_sender),
    };
    sender
        .send(request)
        .map_err(|_| io::Error::other("Network worker stopped"))?;
    receiver
        .recv()
        .map_err(|_| io::Error::other("Network worker stopped"))
}

fn print_reply(reply: &Reply, color: bool) {
    let (text, code) = match &reply.output {
        Ok(text) => (text, "36"),
        Err(text) => (text, "31"),
    };
    if !text.is_empty() {
        if color {
            println!("\x1b[{code}m{text}\x1b[0m");
        } else {
            println!("{text}");
        }
    }
}

fn supports_color() -> bool {
    if std::env::var("TERM").is_ok_and(|term| term == "dumb") {
        return false;
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Console::{
            ENABLE_VIRTUAL_TERMINAL_PROCESSING, GetConsoleMode, GetStdHandle, STD_OUTPUT_HANDLE,
            SetConsoleMode,
        };
        unsafe {
            let handle = GetStdHandle(STD_OUTPUT_HANDLE);
            let mut mode = 0;
            GetConsoleMode(handle, &mut mode) != 0
                && SetConsoleMode(handle, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING) != 0
        }
    }
    #[cfg(not(windows))]
    {
        true
    }
}

fn terminal(sender: &mpsc::Sender<Request>) -> io::Result<()> {
    let interactive = io::stdin().is_terminal() && io::stdout().is_terminal();
    let color = interactive && std::env::var_os("NO_COLOR").is_none() && supports_color();
    println!("World console — offline. Type help for commands.");
    let mut line = String::new();
    loop {
        let status = exchange(sender, None)?;
        print_reply(&status, color);
        if interactive {
            let prompt = status.connection.as_deref().unwrap_or("offline");
            if color {
                print!("\x1b[32m{prompt}>\x1b[0m ");
            } else {
                print!("{prompt}> ");
            }
            io::stdout().flush()?;
        }
        line.clear();
        let command = if io::stdin().read_line(&mut line)? == 0 {
            Some(Command::Exit)
        } else {
            match command::parse(&line) {
                Ok(command) => command,
                Err(error) => {
                    print_reply(
                        &Reply {
                            output: Err(error),
                            connection: status.connection,
                        },
                        color,
                    );
                    continue;
                }
            }
        };
        let Some(command) = command else { continue };
        let exiting = matches!(command, Command::Exit);
        print_reply(&exchange(sender, Some(command))?, color);
        if exiting {
            return Ok(());
        }
    }
}

fn main() -> io::Result<()> {
    let (sender, receiver) = mpsc::channel();
    let worker = thread::Builder::new()
        .name("world-network".into())
        .spawn(move || network::run(receiver))?;
    let result = terminal(&sender);
    drop(sender);
    worker
        .join()
        .map_err(|_| io::Error::other("Network worker panicked"))?;
    result
}
