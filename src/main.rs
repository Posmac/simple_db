use core::default;
use std::{
    collections::HashMap,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener, TcpStream},
    os::fd::{AsFd, AsRawFd, BorrowedFd},
    sync::{LazyLock, Mutex},
};

use nix::poll::{PollFd, PollFlags, PollTimeout};
use socket2::*;

use crate::{
    hashtable::HMap,
    network::{Connection, handle_accept, handle_read, handle_write, read_response, send_request},
};

pub mod common;
pub mod hashtable;
pub mod network;
pub mod redis;
pub mod transport;
pub mod tvl;

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
            // println!("Conn: {:?}", &connections);

            poll_args.clear();

            let pfd = PollFd::new(bfd, PollFlags::POLLIN);
            poll_args.push(pfd);

            for connection in connections.iter() {
                if connection.is_none() {
                    // println!("Connection is none");
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

            let rv = nix::poll::poll(&mut poll_args, PollTimeout::try_from(500).unwrap());
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

                // println!("Accepted {} {}", connections.len(), fd);

                let fd = handle.get_fd();
                if connections.len() <= fd {
                    connections.resize_with(fd + 1, Default::default);
                }

                connections[fd] = Some(handle);
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
                    None => {}
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

        println!("New client: {:#?}", &stream);

        let hello = "hello".repeat(50); ////
        let commands = vec![
            // "get",
            // hello.as_str(),
            // "set",
            // "hello",
            // "nothing", ////
            // "get",
            // "hello", ////
            // "get",
            // "server", ////
            // "del",
            // "hello", ////
            // "get",
            // "hello", ////
        ];

        let wsize = send_request(&mut stream, &commands);
        println!("Wrote size: {}", wsize);

        loop {
            let rsize = read_response(&mut stream);
            // println!("Read size {}", rsize);
            if rsize == 0 {
                // break;
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
