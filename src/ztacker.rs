use tokio::net::{UnixListener, UnixStream};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;
use std::error::Error;
use std::env;
use std::sync::Arc;
use std::path::Path;

static MAX_DIR_SIZE: usize = 4096;
static ZTACKER_ENV: &str = "ZTACKER_SOCK";
static ZTACKER_SOCKET_PATH: &str = "/tmp/ztacker.sock";

static ZTACKER_ERRNO_EMPTY_STACK: &[u8; 1] = b"\x04";
static ZTACKER_ERRNO_LARGE_STR: &[u8; 1] = b"\x05";

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let socket_path = match env::var(ZTACKER_ENV) {
        Ok(val) => val,
        Err(_) => String::from(ZTACKER_SOCKET_PATH),
    };

    // Remove existing socket file if present
    if Path::new(&socket_path).exists() {
        let _ = tokio::fs::remove_file(&socket_path).await;
    }

    let stack: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    let listener = UnixListener::bind(&socket_path)?;
    println!("Server is listening on {}", socket_path);

    loop {
        let (stream, _) = listener.accept().await?;
        let stack = Arc::clone(&stack);
        tokio::spawn(handle_client(stream, stack));
    }
}

async fn handle_client(mut stream: UnixStream, stack: Arc<Mutex<Vec<String>>>) {
    let mut reader = BufReader::new(&mut stream);
    let mut command: [u8; 1] = [0; 1]; // Buffer to read the command byte

    // Read the first u8 to get the command
    match reader.read_exact(&mut command).await {
        Ok(_) => {
            match command[0] {

                // Push to stack
                0x00 => {
                    let mut line = String::new();
                    match reader.read_line(&mut line).await {
                        Ok(_) => {
                            let value = line.trim_end().to_string();
                            if value.len() > MAX_DIR_SIZE {
                                eprintln!("Error: string too long");
                                let _ = stream.write_all(ZTACKER_ERRNO_LARGE_STR).await;
                            } else {
                                let mut stack = stack.lock().await;
                                stack.push(value);
                            }
                        }
                        Err(err) => eprintln!("Error reading from socket: {}", err),
                    }
                }

                // Pop off of stack
                0x01 => {
                    let mut stack = stack.lock().await;
                    if let Some(value) = stack.pop() {
                        let _ = stream.write_all(value.as_bytes()).await;
                    } else {
                        let _ = stream.write_all(ZTACKER_ERRNO_EMPTY_STACK).await;
                    }
                }

                // View stack
                0x02 => {
                    let stack = stack.lock().await;
                    if let Some(value) = stack.last() {
                        let _ = stream.write_all(value.as_bytes()).await;
                    } else {
                        let _ = stream.write_all(ZTACKER_ERRNO_EMPTY_STACK).await;
                    }
                }

                // Clear stack
                0x03 => {
                    let mut stack = stack.lock().await;
                    stack.clear();
                }

                _ => eprintln!("Unknown command"),
            }
        }
        Err(err) => eprintln!("Error reading command from socket: {}", err),
    }
}
