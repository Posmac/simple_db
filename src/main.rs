use std::{
    net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener, TcpStream},
    os::fd::{AsFd, AsRawFd, BorrowedFd},
    str::ParseBoolError,
};

use libc::{pipe, pollfd};
use nix::poll::{self, PollFd, PollFlags, PollTimeout};
use socket2::*;

use crate::{
    common::{process_message_read, process_message_write},
    concurrent::{handle_accept, handle_read, handle_write},
};

const MAX_MSG_SIZE: usize = 4096;
const MSG_HEADER_SIZE: usize = 8;

#[derive(Default)]
pub struct Connection {
    stream: Option<TcpStream>,
    addr: Option<SocketAddr>,
    want_read: bool,
    want_write: bool,
    want_close: bool,

    incoming_data: Vec<u8>,
    outgoing_data: Vec<u8>,
}

impl Connection {
    pub fn get_fd(&self) -> usize {
        let fd = self.stream.as_ref().unwrap().as_raw_fd();
        fd as usize
    }
}

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

    // pub fn client_process(stream: &mut TcpStream, message: &[u8]) -> usize {
    //     process_message_write(stream, message);

    //     println!("Client sent: {:?} {}", message, message.len());

    //     let message = process_message_read(stream);

    //     println!("Client recv: {}", message);

    //     0
    // }

    // pub fn server_process(stream: &mut TcpStream) -> usize {
    //     let message = process_message_read(stream);

    //     println!("Server recv: {}", message);

    //     let message = format!("Server resend: {message}");
    //     process_message_write(stream, message.as_bytes())
    // }
}

pub mod concurrent {

    use crate::{Connection, MAX_MSG_SIZE, MSG_HEADER_SIZE};
    use std::{
        io::{BufRead, Read, Write},
        net::TcpListener,
    };

    pub fn handle_accept(listener: &TcpListener) -> Option<Connection> {
        let con = match listener.accept() {
            Ok(v) => v,
            Err(e) => {
                println!("Failed to get connection {:?}.", e);
                return None;
            }
        };

        let _ = listener.set_nonblocking(true);

        Some(Connection {
            stream: Some(con.0),
            want_read: true,
            want_write: false,
            want_close: false,
            incoming_data: vec![],
            outgoing_data: vec![],
            addr: Some(con.1),
        })
    }

    pub fn handle_read(conn: &mut Connection) {
        let mut buf: [u8; MAX_MSG_SIZE] = [0; MAX_MSG_SIZE];
        let read_size = match conn.stream.as_ref().unwrap().read(&mut buf) {
            Ok(0) => {
                conn.want_close = true;
                return;
            }
            Ok(v) => v,
            Err(e) => {
                println!("Failed to read {:?}", e);
                return;
            }
        };

        if conn.outgoing_data.len() > 0 {
            conn.want_read = false;
            conn.want_write = true;
        }

        //     buf_append();
        //     try_one_request();
    }

    pub fn handle_write(conn: &mut Connection) {
        let write_size = match conn
            .stream
            .as_ref()
            .unwrap()
            .write(conn.outgoing_data.as_slice())
        {
            Ok(0) => {
                conn.want_close = true;
                return;
            }
            Ok(size) => {
                buf_consume(&mut conn.outgoing_data, size);
            }
            Err(e) => {
                println!("Failed to read from stream {:?}", e);
                return;
            }
        };

        if conn.outgoing_data.len() > 0 {
            conn.want_read = true;
            conn.want_write = false;
        }
    }

    // pub fn try_one_request(conn: &mut Connection) -> bool {
    //     if conn.incoming_data.len() < MSG_HEADER_SIZE {
    //         return false;
    //     }

    //     let mut len_buffer = [0u8; MSG_HEADER_SIZE];
    //     len_buffer.copy_from_slice(conn.incoming_data.as_slice());
    //     let len = usize::from_ne_bytes(len_buffer);

    //     if len > MAX_MSG_SIZE {
    //         conn.want_close = true;
    //         return false;
    //     };

    //     if MSG_HEADER_SIZE + MAX_MSG_SIZE > conn.incoming_data.len() {
    //         return false;
    //     };

    //     buf_consume(&mut conn.incoming_data, MSG_HEADER_SIZE + len);

    //     return true;
    // }

    pub fn buf_append(buf: &mut Vec<u8>, data: &[u8]) {
        buf.extend_from_slice(data);
    }

    pub fn buf_consume(buf: &mut Vec<u8>, len: usize) {
        buf.drain(0..len);
    }
}

#[allow(unreachable_code)]
fn main() -> Result<(), std::io::Error> {
    // #[cfg(feature = "server")]
    {
        let socket = Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP))
            .expect("Failed to create socket");

        let addr = Ipv4Addr::new(0, 0, 0, 0);
        let address = SocketAddr::V4(SocketAddrV4::new(addr, 8080));

        socket.set_reuse_address(true)?;
        socket.set_nonblocking(true)?;

        socket.bind(&address.into()).expect("Failed to bind socket");
        socket.listen(128).expect("failed to listen");
        let listener: TcpListener = socket.into();
        let fd = listener.as_raw_fd();
        let bfd = unsafe { BorrowedFd::borrow_raw(fd) };

        let mut connections: Vec<Option<Connection>> = vec![];
        let mut poll_args: Vec<PollFd> = vec![];

        loop {
            poll_args.clear();

            let pfd = PollFd::new(bfd, PollFlags::POLLIN);
            poll_args.push(pfd);

            for connection in connections.iter() {
                let mut events = PollFlags::POLLERR;
                let connection = connection.as_ref().unwrap();

                if connection.want_read {
                    events |= PollFlags::POLLIN;
                }

                if connection.want_write {
                    events |= PollFlags::POLLOUT;
                }

                let fd = connection.stream.as_ref().unwrap().as_raw_fd();
                let bfd = unsafe { BorrowedFd::borrow_raw(fd) };
                let pfd = PollFd::new(bfd, events);

                poll_args.push(pfd);
            }

            let rv = nix::poll::poll(&mut poll_args, PollTimeout::NONE);
            match rv {
                Ok(pfd) => {
                    if pfd < 0 {
                        println!("Failed to poll pfds, returned {}", pfd);
                        continue;
                    }
                }
                Err(e) => {
                    println!("Failed to poll pfds, {:?}", e);
                    continue;
                }
            }

            if poll_args[0].any().unwrap_or_default() {
                let handle = match handle_accept(&listener) {
                    Some(h) => h,
                    None => continue,
                };

                let fd = handle.get_fd();
                if connections.len() <= fd {
                    connections.resize_with(handle.get_fd(), Default::default);
                    connections[fd] = Some(handle);
                }
            }

            for pfd_id in 1..poll_args.len() {
                let pfd = &poll_args[pfd_id];
                let conn = &mut connections[pfd.as_fd().as_raw_fd() as usize];

                let mut connection = conn.as_mut().unwrap();

                match pfd.revents() {
                    Some(e) => {
                        if e.intersects(PollFlags::POLLIN) {
                            handle_read(&mut connection);
                        }
                        if e.intersects(PollFlags::POLLOUT) {
                            handle_write(&mut connection);
                        }
                        if e.intersects(PollFlags::POLLERR) || connection.want_close {
                            conn.take();
                        }
                    }
                    None => todo!(),
                }
            }
        }

        //INFO: In C we should call close?
        //INFO: NO!
    }

    #[cfg(feature = "client")]
    {
        let socket = Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP))
            .expect("Failed to create a socket");

        let addr = Ipv4Addr::new(127, 0, 0, 1);
        let address = SocketAddr::V4(SocketAddrV4::new(addr, 8080));

        socket.set_reuse_address(true)?;
        //INFO: NO NEED?
        //socket.set_nonblocking(true)?;

        socket
            .connect(&SockAddr::from(address))
            .expect("failed to connect to the server");

        let mut stream: TcpStream = socket.into();

        let str: Vec<u8> = vec![255; MAX_MSG_SIZE];
        let mut queries = vec![];
        queries.push(str.as_slice());
        queries.push("Hello from client!".as_bytes());
        queries.push("AGAIN...Hello from client!".as_bytes());
        queries.push("LAST...Hello from client!".as_bytes());

        for q in queries.iter() {
            let wsize = process_message_write(&mut stream, q);
            if wsize == 0 {
                println!("Failed to query from client");
            }
        }

        for i in 0..queries.len() {
            let rsize = process_message_read(&mut stream);
            println!("Read from server: {rsize}");
        }

        loop {}
    }

    Ok(())
}
