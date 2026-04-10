use std::{
    io::{ErrorKind, Read, Write},
    net::TcpStream,
};

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
