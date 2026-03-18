use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener, TcpStream};

use socket2::*;

use crate::common::{one_request, query};

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

    pub fn one_request(stream: &mut TcpStream) -> usize {
        //byte header + MAX_MSG_SIZE
        let mut buffer: [u8; MAX_MSG_SIZE + MSG_HEADER_SIZE] = [0; MAX_MSG_SIZE + MSG_HEADER_SIZE];
        let read_size = read_full(stream, &mut buffer[0..MSG_HEADER_SIZE], MSG_HEADER_SIZE);

        // assert!(read_size != 0, "Readed header size is zero");
        // assert!(
        //     read_size == MSG_HEADER_SIZE,
        //     "Readed size != MSG_HEADER_SIZE"
        // );

        let message_size = get_header(&buffer);

        println!("One req Header: {:?} {}", read_size, message_size);

        let read_size = read_full(
            stream,
            &mut buffer[MSG_HEADER_SIZE..(MSG_HEADER_SIZE + message_size)],
            message_size,
        );

        // assert!(read_size > 0, "Readed message size is zero");
        // assert!(
        //     read_size == message_size,
        //     "Readed message size is not that expected"
        // );

        let message = match str::from_utf8(&buffer[MSG_HEADER_SIZE..(MSG_HEADER_SIZE + read_size)])
        {
            Ok(msg) => msg,
            Err(e) => {
                println!("Failed to parse message buffer {:?}", e);
                ""
            }
        };

        println!("Server says: {}", message);

        // let reply = String::from("Server got the message, ok");
        // let mut reply_buffer = generate_message_buffer(reply.as_bytes());
        // let len = reply_buffer.len();

        // write_full(stream, &mut reply_buffer, len)
        message_size
    }

    pub fn query(stream: &mut TcpStream, message: &[u8]) -> usize {
        // assert!(
        //     message.len() <= MAX_MSG_SIZE,
        //     "Message size is too big {}",
        //     message.len()
        // );

        let message_copy = generate_message_buffer(message);
        let message_copy_len = message_copy.len();

        let wsize = write_full(stream, &message_copy[0..message_copy_len], message_copy_len);
        println!("Client says: {:?}", message);

        // assert!(wsize != 0, "Wrote size is ZERO {}", wsize);
        // assert!(
        //     wsize < message_copy.len(),
        //     "Wrote size is less then message size"
        // );

        //read respnse
        // let mut response_buffer: [u8; MSG_HEADER_SIZE + MAX_MSG_SIZE] =
        //     [0; MSG_HEADER_SIZE + MAX_MSG_SIZE];
        // let rsize = read_full(
        //     stream,
        //     &mut response_buffer[0..MSG_HEADER_SIZE],
        //     MSG_HEADER_SIZE,
        // );

        // assert!(rsize != 0, "Read size is ZERO {}", rsize);
        // assert!(
        //     rsize == MSG_HEADER_SIZE,
        //     "Header size is lower than expected"
        // );

        // let message_size = get_header(&response_buffer);

        // println!(
        //     "Query Header: {:?} {} {}",
        //     &response_buffer[0..MSG_HEADER_SIZE],
        //     rsize,
        //     message_size,
        // );

        // let read_size = read_full(
        //     stream,
        //     &mut response_buffer[MSG_HEADER_SIZE..(MSG_HEADER_SIZE + message_size)],
        //     message_size,
        // );

        // assert!(read_size > 0, "Readed message size is zero");
        // assert!(
        //     read_size < message_size,
        //     "Readed message is lower than expected"
        // );

        // let message = match str::from_utf8(
        //     &response_buffer[MSG_HEADER_SIZE..(MSG_HEADER_SIZE + message_size)],
        // ) {
        //     Ok(msg) => msg,
        //     Err(e) => {
        //         println!("Failed to parse message buffer {:?}", e);
        //         ""
        //     }
        // };

        wsize
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
                let stream = &mut conn.0;
                let query_res = one_request(stream);
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
        let socket = Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP))
            .expect("Failed to create a socket");

        let addr = Ipv4Addr::new(127, 0, 0, 1);
        let address = SocketAddr::V4(SocketAddrV4::new(addr, 8080));

        socket.set_reuse_address(true)?;
        socket
            .connect(&SockAddr::from(address))
            .expect("failed to connect to the server");

        let mut stream: TcpStream = socket.into();

        let mut query_res = query(&mut stream, "Hello from client!".as_bytes());
        // if query_res == 0 {
        //     return Ok(());
        // }

        query_res = query(&mut stream, "Again... Hello from client".as_bytes());
        // if query_res == 0 {
        //     return Ok(());
        // }

        query_res = query(&mut stream, "Last time... Hello from client".as_bytes());
        // if query_res == 0 {
        //     return Ok(());
        //
        loop {}
    }

    Ok(())
}
