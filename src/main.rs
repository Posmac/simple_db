use std::{
    net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener, TcpStream},
    os::fd::{AsFd, AsRawFd, BorrowedFd},
};

use libc::if_msghdr2;
use nix::poll::{PollFd, PollFlags, PollTimeout};
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
    want_read: bool,
    want_write: bool,
    want_close: bool,

    incoming_data: Vec<u8>, //INFO: Data from TCP to parse
    outgoing_data: Vec<u8>, //INFO: Data for TCP to send
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

    use crate::{
        Connection, MAX_MSG_SIZE, MSG_HEADER_SIZE,
        common::{get_header, process_message_write},
    };
    use std::{
        io::{Read, Write},
        mem::swap,
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

        println!("Accepted new connection: {:#?}", con.1);

        Some(Connection {
            stream: Some(con.0),
            want_read: true,
            want_write: false,
            want_close: false,
            incoming_data: vec![],
            outgoing_data: vec![],
        })
    }

    pub fn handle_read(conn: &mut Connection) -> usize {
        let mut buf: [u8; MAX_MSG_SIZE + MSG_HEADER_SIZE] = [0; MAX_MSG_SIZE + MSG_HEADER_SIZE];
        let read_size = match conn.stream.as_ref().unwrap().read(&mut buf) {
            Ok(0) => {
                conn.want_close = true;
                return 0;
            }
            Ok(v) => {
                buf_append(&mut conn.incoming_data, &buf, v);

                while try_one_request(conn) {}

                if conn.outgoing_data.len() > 0 {
                    conn.want_read = false;
                    conn.want_write = true;

                    return handle_write(conn);
                };
                v
            }
            Err(e) => {
                println!("Failed to read {:?}", e);
                return 0;
            }
        };

        read_size
    }

    pub fn handle_write(conn: &mut Connection) -> usize {
        let write_size = match conn
            .stream
            .as_ref()
            .unwrap()
            .write(conn.outgoing_data.as_slice())
        {
            Ok(0) => {
                conn.want_close = true;
                return 0;
            }
            Ok(size) => {
                buf_consume(&mut conn.outgoing_data, size);
                if conn.outgoing_data.len() == 0 {
                    conn.want_read = true;
                    conn.want_write = false;
                }
                size
            }
            Err(e) => {
                println!("Failed to read from stream {:?}", e);
                return 0;
            }
        };

        write_size
    }

    pub fn try_one_request(conn: &mut Connection) -> bool {
        if conn.incoming_data.len() < MSG_HEADER_SIZE {
            return false;
        }

        let len = get_header(&conn.incoming_data.as_slice()[0..MSG_HEADER_SIZE]);

        if len > MAX_MSG_SIZE {
            conn.want_close = true;
            return false;
        };

        if MSG_HEADER_SIZE + MAX_MSG_SIZE < conn.incoming_data.len() {
            return false;
        };
        println!("There: {}", len);

        let msg = str::from_utf8(
            &conn.incoming_data.as_slice()[MSG_HEADER_SIZE..(MSG_HEADER_SIZE + len)],
        )
        .unwrap_or_default();

        println!("Client says: {} {:?}", len, msg);

        buf_append(&mut conn.outgoing_data, &len.to_ne_bytes(), MSG_HEADER_SIZE);
        buf_append(&mut conn.outgoing_data, &msg.as_bytes(), len);

        buf_consume(&mut conn.incoming_data, MSG_HEADER_SIZE + len);

        return true;
    }

    pub fn buf_append(buf: &mut Vec<u8>, data: &[u8], len: usize) {
        buf.extend_from_slice(&data[0..len]);
    }

    pub fn buf_consume(buf: &mut Vec<u8>, len: usize) {
        buf.drain(0..len);
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
                if connection.is_none() {
                    continue;
                }

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
                    connections.resize_with(fd + 1, Default::default);
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
        let local_addr = stream.peer_addr().unwrap().to_string();
        let mut str: Vec<u8> = vec![65; MAX_MSG_SIZE - 200];
        str.extend_from_slice(local_addr.as_bytes());

        let mut queries = vec![];
        let msg1 = format!("Hello from client! {}", local_addr);
        let msg2 = format!("AGAIN_Hello from client! {}", address);
        let msg3 = format!("LAST_Hello from client! {}", address);

        queries.push(str.as_slice());
        queries.push(msg1.as_bytes());
        queries.push(msg2.as_bytes());
        queries.push(msg3.as_bytes());

        loop {
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
        }
    }

    Ok(())
}

pub mod test {
    #[test]
    #[cfg(feature = "client")]
    pub fn multiple_connections() {}
}
