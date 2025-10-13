use std::env;
use std::error::Error;
use std::fmt;
use std::os::unix::net::UnixStream;
use std::process;

use greetd_ipc::{codec::SyncCodec, AuthMessageType, Request, Response};

#[derive(Debug)]
struct LoginError(String);

impl fmt::Display for LoginError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "There is an error: {}", self.0)
    }
}

impl Error for LoginError {}

pub struct GreetD {
    pub stream: UnixStream,
}

impl GreetD {
    pub fn new() -> Self {
        let socket = env::var("GREETD_SOCK");
        if socket.is_err() {
            eprintln!("GREETD_SOCK must be defined");
            process::exit(1);
        }
        match UnixStream::connect(socket.unwrap()) {
            Ok(stream) => GreetD { stream },

            Err(err) => {
                eprintln!("{}", err);
                process::exit(1);
            }
        }
    }

    pub fn login(
        &mut self,
        username: String,
        password: String,
        cmd: Vec<String>,
    ) -> Result<(), Box<dyn Error>> {
        // Start session creation
        Request::CreateSession { username }.write_to(&mut self.stream)?;
        // Loop to handle all auth messages (e.g., password, 2FA, etc)
        loop {
            let response = Response::read_from(&mut self.stream)?;
            match response {
                Response::AuthMessage { auth_message: _, auth_message_type } => {
                    match auth_message_type {
                        AuthMessageType::Secret => {
                            // Send password
                            Request::PostAuthMessageResponse {
                                response: Some(password.clone()),
                            }.write_to(&mut self.stream)?;
                        }
                        _ => {
                            // For any other prompt, just send empty response
                            Request::PostAuthMessageResponse {
                                response: None,
                            }.write_to(&mut self.stream)?;
                        }
                    }
                }
                Response::Success => {
                    // Auth succeeded, now start session
                    Request::StartSession { cmd: cmd.clone() }.write_to(&mut self.stream)?;
                    let resp = Response::read_from(&mut self.stream)?;
                    match resp {
                         Response::Success => return Ok(()),
                        Response::Error { .. } | Response::AuthMessage { .. } => {
                            return Err(Box::new(LoginError("Session failed to start".into())))
                        }
                    }
                }
                Response::Error { .. } => {
                    return Err(Box::new(LoginError("Wrong username or password".into())))
                }
                _ => {
                    return Err(Box::new(LoginError("Unknown error".into())))
                }
            }
        }
    }

    pub fn cancel(&mut self) {
        let _ = Request::CancelSession.write_to(&mut self.stream);
        let _ = Response::read_from(&mut self.stream);
    }
}
