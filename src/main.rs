use std::{
    collections::HashMap,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener, TcpStream},
    os::fd::{AsFd, AsRawFd, BorrowedFd},
    sync::{LazyLock, Mutex},
};

use nix::poll::{PollFd, PollFlags, PollTimeout};
use socket2::*;

use crate::{
    common::process_message_read,
    concurrent::{handle_accept, handle_read, handle_write, read_response, send_request},
};

const MAX_MSG_SIZE: usize = 65535;
const MSG_HEADER_SIZE: usize = 8;
const RESPONSE_STATUS_SIZE: usize = 4;
const MAX_COMMANDS_SIZE: usize = 1024;

pub struct Connection {
    stream: TcpStream,
    want_read: bool,
    want_write: bool,
    want_close: bool,

    incoming_data: Vec<u8>, //INFO: Data from TCP to parse
    outgoing_data: Vec<u8>, //INFO: Data for TCP to send
}

impl Connection {
    pub fn get_fd(&self) -> usize {
        // let fd = self.stream.as_ref().unwrap().as_raw_fd();
        let fd = self.stream.as_raw_fd();
        fd as usize
    }
}

#[derive(Default, Debug)]
pub struct Response {
    pub status: i32,
    pub data: Option<Vec<u8>>,
}

static PLACE_HOLDER: LazyLock<Mutex<HashMap<String, String>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

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

    pub fn get_message(bytes: &[u8], offset: usize, size: usize) -> String {
        let message = match str::from_utf8(&bytes[offset..(offset + size)]) {
            Ok(msg) => msg,
            Err(e) => {
                println!("Failed to parse message buffer {:?}", e);
                ""
            }
        };

        message.to_string()
    }

    // pub fn generate_message_buffer(message: &[u8]) -> [u8; MAX_MSG_SIZE + MSG_HEADER_SIZE] {
    //     let mut buffer: [u8; MAX_MSG_SIZE + MSG_HEADER_SIZE] = [0; MAX_MSG_SIZE + MSG_HEADER_SIZE];

    //     let message_len_bytes = message.len().to_ne_bytes();
    //     buffer[0..MSG_HEADER_SIZE].copy_from_slice(&message_len_bytes);
    //     buffer[MSG_HEADER_SIZE..(MSG_HEADER_SIZE + message.len())].copy_from_slice(message);

    //     buffer
    // }

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
}

pub mod concurrent {

    use crate::{
        Connection, MAX_COMMANDS_SIZE, MAX_MSG_SIZE, MSG_HEADER_SIZE, PLACE_HOLDER,
        RESPONSE_STATUS_SIZE, Response,
        common::{get_header, get_message, read_full, write_full},
    };
    use std::{
        io::{Read, Repeat, Write},
        net::{TcpListener, TcpStream},
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
            stream: con.0,
            want_read: true,
            want_write: false,
            want_close: false,
            incoming_data: vec![],
            outgoing_data: vec![],
        })
    }

    pub fn handle_read(conn: &mut Connection) -> usize {
        let mut buf: [u8; MAX_MSG_SIZE + MSG_HEADER_SIZE] = [0; MAX_MSG_SIZE + MSG_HEADER_SIZE];
        // let read_size = match conn.stream.as_ref().unwrap().read(&mut buf) {
        let read_size = match conn.stream.read(&mut buf) {
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
                conn.want_close = true;
                return 0;
            }
        };

        read_size
    }

    pub fn handle_write(conn: &mut Connection) -> usize {
        let write_size = match conn
            .stream
            // .as_ref()
            // .unwrap()
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

        let commands_len = get_header(&conn.incoming_data.as_slice()[0..MSG_HEADER_SIZE]);
        if commands_len > MAX_COMMANDS_SIZE {
            conn.want_close = true;
            return false;
        };

        if MAX_MSG_SIZE < conn.incoming_data.len() {
            return false;
        }
        println!(
            "CAME LEN: {:?} {} {}",
            &conn.incoming_data,
            commands_len,
            conn.incoming_data.len()
        );

        let mut cmd: Vec<String> = Vec::with_capacity(commands_len);
        let parsed_size = parse_request(conn.incoming_data.as_slice(), commands_len, &mut cmd);
        if parsed_size == 0 {
            conn.want_close = true;
            return false;
        }
        println!("Out: {:?}", cmd);

        let mut offset = 0usize;
        while offset < cmd.len() {
            let response = process_request(&cmd[offset..], &mut offset);
            generate_response(response, &mut conn.outgoing_data);
        }

        buf_consume(&mut conn.incoming_data, parsed_size);

        return true;
    }

    pub fn buf_append(buf: &mut Vec<u8>, data: &[u8], len: usize) {
        buf.extend_from_slice(&data[0..len]);
    }

    pub fn buf_consume(buf: &mut Vec<u8>, len: usize) {
        buf.drain(0..len);
    }

    pub fn parse_request(data: &[u8], size: usize, out: &mut Vec<String>) -> usize {
        let mut offset = 0; //INFO: We want offset after every read from data, in order to not track some shit sizes
        let commands_number = get_header(&data[offset..MSG_HEADER_SIZE]);
        offset += MSG_HEADER_SIZE;

        if commands_number == 0 {
            return 0;
        };

        if commands_number > MAX_COMMANDS_SIZE {
            return 0;
        };

        //INFO: Especially there
        println!("Commands number: {commands_number} {}", out.len());
        while out.len() < commands_number {
            let command_len = get_header(&data[offset..offset + MSG_HEADER_SIZE]);
            offset += MSG_HEADER_SIZE;

            let msg = get_message(data, offset, command_len);
            offset += command_len;

            out.push(msg);
        }

        offset
    }

    pub fn send_request(stream: &mut TcpStream, commands: &Vec<&str>) -> usize {
        let mut final_buffer = [0u8; MSG_HEADER_SIZE + MAX_MSG_SIZE];
        final_buffer[0..MSG_HEADER_SIZE].copy_from_slice(&commands.len().to_ne_bytes());
        let mut offset = MSG_HEADER_SIZE;

        println!("BYTES: {:?} {}", &final_buffer[0..offset], commands.len());

        for c in commands.iter() {
            final_buffer[offset..offset + MSG_HEADER_SIZE].copy_from_slice(&c.len().to_ne_bytes());
            offset += MSG_HEADER_SIZE;

            final_buffer[offset..(offset + c.len())].copy_from_slice(c.as_bytes());
            offset += c.len();
        }

        println!("SEND LEN: {offset}");
        let wsize = write_full(stream, &final_buffer[0..offset], offset);
        wsize
    }

    pub fn process_request(cmd: &[String], offset: &mut usize) -> Response {
        let command = cmd[0].as_str();
        match command {
            "get" => {
                println!("Processing get req: {:?}", &cmd);
                let map = PLACE_HOLDER.lock().unwrap();
                if !map.contains_key(cmd[1].as_str()) {
                    *offset += 2;
                    return Response {
                        status: -2,
                        data: None,
                    };
                };

                let value = map.get(cmd[1].as_str()).unwrap();

                *offset += 2;
                Response {
                    status: 0,
                    data: Some(value.to_string().as_bytes().to_vec()),
                }
            }
            "set" => {
                println!("Processing set req: {:?}", &cmd);
                let mut map = PLACE_HOLDER.lock().unwrap();
                let value = map
                    .insert(cmd[1].to_string(), cmd[2].to_string())
                    .unwrap_or_else(|| cmd[2].to_string());

                *offset += 3;
                Response {
                    status: 0,
                    data: Some(value.as_bytes().to_vec()),
                }
            }
            "del" => {
                println!("Processing del req: {:?}", &cmd);
                let mut map = PLACE_HOLDER.lock().unwrap();
                let value = map.remove(cmd[1].as_str()).unwrap();
                *offset += 2;
                Response {
                    status: 0,
                    data: Some(value.as_bytes().to_vec()),
                }
            }
            _ => Response {
                status: -1,
                data: None,
            },
        }
    }

    pub fn generate_response(resp: Response, out: &mut Vec<u8>) {
        println!("Response: {:?}", resp);
        let resp_data = match resp.data {
            Some(r) => r,
            None => {
                vec![]
            }
        };

        buf_append(
            out,
            &(resp_data.len() + RESPONSE_STATUS_SIZE).to_ne_bytes(),
            MSG_HEADER_SIZE,
        );
        buf_append(out, &resp.status.to_ne_bytes(), RESPONSE_STATUS_SIZE);
        buf_append(out, &resp_data, resp_data.len());
    }

    pub fn read_response(stream: &mut TcpStream) -> usize {
        let mut bytes = [0u8; MSG_HEADER_SIZE + MAX_MSG_SIZE];
        let rsize = read_full(stream, &mut bytes[0..MSG_HEADER_SIZE], MSG_HEADER_SIZE);
        if rsize == 0 {
            return 0;
        }

        let mut len_copy = [0u8; MSG_HEADER_SIZE];
        len_copy.copy_from_slice(&bytes[0..MSG_HEADER_SIZE]);
        let response_size = usize::from_ne_bytes(len_copy);

        // println!("Data len {:?}", &bytes[0..MSG_HEADER_SIZE]);

        let rsize = read_full(
            stream,
            &mut bytes[MSG_HEADER_SIZE..MSG_HEADER_SIZE + response_size],
            response_size,
        );

        // println!(
        //     "Response len {:?}",
        //     &bytes[MSG_HEADER_SIZE..MSG_HEADER_SIZE + response_size]
        // );
        if rsize == 0 {
            return 0;
        }

        let message = get_message(
            &bytes,
            MSG_HEADER_SIZE + RESPONSE_STATUS_SIZE,
            response_size,
        );

        println!(
            "Response: {}",
            message,
            // &bytes[MSG_HEADER_SIZE..MSG_HEADER_SIZE + RESPONSE_STATUS_SIZE]
        );

        rsize
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
                    println!("Connection is none");
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

                let fd = connection.stream.as_raw_fd();
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

        let commands = vec![
            "get", "hello", ////
            "set", "hello", "nothing", ////
            "get", "hello", ////
            "get", "server", ////
            "del", "hello", ////
            "get", "hello", ////
        ];

        let wsize = send_request(&mut stream, &commands);
        println!("Wrote size: {}", wsize);

        loop {
            let rsize = read_response(&mut stream);
            // println!("Read size {}", rsize);
            if rsize == 0 {
                break;
            }
        }

        // let mut str: Vec<u8> = vec![65; MAX_MSG_SIZE - 200];
        // str.extend_from_slice(local_addr.as_bytes());

        // let mut queries = vec![];
        // let msg1 = format!("Hello from client! {}", local_addr);
        // let msg2 = format!("AGAIN_Hello from client! {}", address);
        // let msg3 = format!("LAST_Hello from client! {}", address);

        // queries.push(str.as_slice());
        // queries.push(msg1.as_bytes());
        // queries.push(msg2.as_bytes());
        // queries.push(msg3.as_bytes());

        // loop {
        //     for q in queries.iter() {
        //         let wsize = process_message_write(&mut stream, q);
        //         if wsize == 0 {
        //             println!("Failed to query from client");
        //         }
        //     }

        //     for i in 0..queries.len() {
        //         let rsize = process_message_read(&mut stream);
        //         println!("Read from server:");
        //     }
        // }
    }

    Ok(())
}

pub mod test {
    #[test]
    #[cfg(feature = "client")]
    pub fn multiple_connections() {}
}
