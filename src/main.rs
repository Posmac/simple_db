use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener, TcpStream};

use socket2::*;

use crate::common::{process_message_read, process_message_write};

const MAX_MSG_SIZE: usize = 4096;
const MSG_HEADER_SIZE: usize = 8;

pub mod common {
    use std::{
        io::{ErrorKind, Read, Write},
        net::TcpStream,
    };

    use crate::{MAX_MSG_SIZE, MSG_HEADER_SIZE};
    //WARNING: ANALOG from TcpStream: stream.read_exact(&mut bytes[..size])?;
    #[allow(unreachable_code)]
    pub fn read_full(stream: &mut TcpStream, bytes: &mut [u8], size: usize) -> usize {
        assert!(size <= bytes.len());
        let mut total: usize = 0;
        while total < size {
            let rsize = match stream.read(&mut bytes[total..size]) {
                Ok(0) => {
                    break;
                }
                Ok(v) => v,
                Err(e) if e.kind() == ErrorKind::Interrupted => {
                    continue;
                }
                Err(e) => {
                    println!("Failed to read from stream {:?}", e);
                    break;
                }
            };

            total += rsize;
        }
        total
    }

    //WARNING: ANALOG from TcpStream: stream.write_all(&bytes[..size])?;
    #[allow(unreachable_code)]
    pub fn write_full(stream: &mut TcpStream, bytes: &[u8], size: usize) -> usize {
        assert!(size <= bytes.len());
        let mut total: usize = 0;
        while total < size {
            let wsize = match stream.write(&bytes[total..size]) {
                Ok(0) => {
                    println!("Write 0 to stream");
                    break;
                }
                Ok(size) => size,
                Err(e) if e.kind() == ErrorKind::Interrupted => {
                    continue;
                }
                Err(e) => {
                    println!("Failed to write to stream {:?}", e);
                    break;
                }
            };

            total += wsize;
        }
        total
    }

    pub fn get_header(bytes: &[u8]) -> usize {
        let mut bytes_copy: [u8; MSG_HEADER_SIZE] = [0; MSG_HEADER_SIZE];
        bytes_copy.clone_from_slice(&bytes[0..MSG_HEADER_SIZE]);
        let message_size = usize::from_ne_bytes(bytes_copy);

        assert!(
            message_size <= MAX_MSG_SIZE,
            "MESSAGE SIZE IS TOO LONG {}",
            message_size
        );

        message_size
    }

    pub fn generate_message_buffer(message: &[u8]) -> [u8; MAX_MSG_SIZE + MSG_HEADER_SIZE] {
        let mut buffer: [u8; MAX_MSG_SIZE + MSG_HEADER_SIZE] = [0; MAX_MSG_SIZE + MSG_HEADER_SIZE];

        let message_len_bytes = message.len().to_ne_bytes();
        buffer[0..MSG_HEADER_SIZE].copy_from_slice(&message_len_bytes);
        buffer[MSG_HEADER_SIZE..(MSG_HEADER_SIZE + message.len())].copy_from_slice(message);

        buffer
    }

    pub fn process_message_read(stream: &mut TcpStream) -> String {
        let mut buffer: [u8; MAX_MSG_SIZE + MSG_HEADER_SIZE] = [0; MAX_MSG_SIZE + MSG_HEADER_SIZE];
        let read_size = read_full(stream, &mut buffer[0..MSG_HEADER_SIZE], MSG_HEADER_SIZE);
        let message_size = get_header(&buffer);

        let read_size = read_full(
            stream,
            &mut buffer[MSG_HEADER_SIZE..(MSG_HEADER_SIZE + message_size)],
            message_size,
        );

        println!("READ: {:?} {}", read_size, message_size);

        let message = match str::from_utf8(&buffer[MSG_HEADER_SIZE..(MSG_HEADER_SIZE + read_size)])
        {
            Ok(msg) => msg,
            Err(e) => {
                println!("Failed to parse message buffer {:?}", e);
                ""
            }
        };

        message.to_string()
    }

    pub fn process_message_write(stream: &mut TcpStream, message: &[u8]) -> usize {
        let message_copy = generate_message_buffer(message);
        let len = MSG_HEADER_SIZE + message.len();

        let wsize = write_full(stream, &message_copy[0..(len)], len);

        println!("WRITE: {} {}", wsize, len);

        wsize
    }

    pub fn client_process(stream: &mut TcpStream, message: &[u8]) -> usize {
        process_message_write(stream, message);

        println!("Client sent: {:?} {}", message, message.len());

        let message = process_message_read(stream);

        println!("Client recv: {}", message);

        0
    }

    pub fn server_process(stream: &mut TcpStream) -> usize {
        let message = process_message_read(stream);

        println!("Server recv: {}", message);

        let message = format!("Server resend: {message}");
        process_message_write(stream, message.as_bytes())
    }
}

#[allow(unreachable_code)]
fn main() -> Result<(), std::io::Error> {
    #[cfg(feature = "server")]
    {
        let socket = Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP))
            .expect("Failed to create socket");

        let addr = Ipv4Addr::new(0, 0, 0, 0);
        let address = SocketAddr::V4(SocketAddrV4::new(addr, 8080));

        socket.set_reuse_address(true)?;

        socket.bind(&address.into()).expect("Failed to bind socket");
        socket.listen(128).expect("failed to listen");
        let listener: TcpListener = socket.into();

        loop {
            use std::{thread, time::Duration};

            let mut conn = listener.accept()?;

            loop {
                use crate::common::server_process;

                let stream = &mut conn.0;
                let query_res = server_process(stream);
                if query_res == 0 {
                    break;
                }
            }
        }

        //INFO: In C we should call close?
        //INFO: NO!

        println!("Hello, world!");
    }

    #[cfg(feature = "client")]
    {
        use crate::common::client_process;

        let socket = Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP))
            .expect("Failed to create a socket");

        let addr = Ipv4Addr::new(127, 0, 0, 1);
        let address = SocketAddr::V4(SocketAddrV4::new(addr, 8080));

        socket.set_reuse_address(true)?;
        socket
            .connect(&SockAddr::from(address))
            .expect("failed to connect to the server");

        let mut stream: TcpStream = socket.into();

        let mut query_res = client_process(&mut stream, "Hello from client!".as_bytes());
        // if query_res == 0 {
        //     return Ok(());
        // }

        query_res = client_process(&mut stream, "Again... Hello from client".as_bytes());
        // if query_res == 0 {
        //     return Ok(());
        // }

        query_res = client_process(&mut stream, "Last time... Hello from client".as_bytes());
        // if query_res == 0 {
        //     return Ok(());
        //
        loop {}
    }

    Ok(())
}
