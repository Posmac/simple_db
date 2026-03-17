use std::{
    io::{Read, Write},
    net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener, TcpStream},
    os::fd::IntoRawFd,
};

use socket2::*;

pub mod server {
    use std::{
        io::{Read, Write},
        net::TcpStream,
    };

    pub fn do_something(stream: &mut TcpStream) {
        let mut buf: [u8; 64] = [0; 64];
        let read_size = match stream.read(&mut buf) {
            Ok(size) => size,
            Err(e) => {
                println!("Failed to read from tcp stream {:?}", e);
                0
            }
        };

        if read_size <= 0 {
            println!("Readed buffer size is <= 0");
            return;
        }

        println!("Read: {:?}", buf);

        let hello = String::from("HELLO");

        let wsize = match stream.write(hello.as_bytes()) {
            Ok(size) => size,
            Err(e) => {
                println!("Failed to write bytes {:?}", e);
                0
            }
        };

        println!("Wrote {} bytes", wsize);
    }
}

pub mod client {}

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
            let conn = listener.accept()?;

            let mut stream = conn.0;
            server::do_something(&mut stream);
        }

        println!("Hello, world!");
    }

    #[cfg(feature = "client")]
    {
        let socket = Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP))
            .expect("Failed to create a socket");

        let addr = Ipv4Addr::new(127, 0, 0, 1);
        let address = SocketAddr::V4(SocketAddrV4::new(addr, 8080));

        socket
            .connect(&SockAddr::from(address))
            .expect("failed to connect to the server");

        let mut stream: TcpStream = socket.into();

        let msg = String::from("Hello from client");
        let wsize = match stream.write(msg.as_bytes()) {
            Ok(size) => size,
            Err(e) => {
                println!("Failed to write to a stream, {}", e);
                0
            }
        };

        println!("Bytes wrote: {}", wsize);

        let mut buf: [u8; 64] = [0; 64];
        let rsize = match stream.read(&mut buf) {
            Ok(size) => size,
            Err(e) => {
                println!("Failed to read bytes: {:?}", e);
                0
            }
        };

        println!("Bytes read: {}", rsize);
    }

    Ok(())
}
