use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    os::fd::AsRawFd,
};

use crate::{
    common::{MAX_COMMANDS_SIZE, MAX_MSG_SIZE, MSG_HEADER_SIZE, get_header, get_message},
    redis::{do_del, do_get, do_set},
    transport::{read_full, write_full},
    tvl::{
        DATA_TYPE_SIZE, DataType, RESPONSE_CODE_SIZE, ResponseCode, buf_append, buf_consume,
        response_begin, response_end,
    },
};

#[derive(Debug)]
pub struct Connection {
    pub stream: TcpStream,
    pub want_read: bool,
    pub want_write: bool,
    pub want_close: bool,

    pub incoming_data: Vec<u8>, //INFO: Data from TCP to parse
    pub outgoing_data: Vec<u8>, //INFO: Data for TCP to send
}

impl Connection {
    pub fn get_fd(&self) -> usize {
        // let fd = self.stream.as_ref().unwrap().as_raw_fd();
        let fd = self.stream.as_raw_fd();
        fd as usize
    }
}

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
    // let read_size = read_full(stream, bytes, size)
    let read_size = match conn.stream.read(&mut buf) {
        Ok(0) => {
            conn.want_close = true;
            return 0;
        }
        Ok(v) => {
            // println!("Read: {v}");
            buf_append(&mut conn.incoming_data, &buf, v);

            while try_one_request(conn) {}

            if conn.outgoing_data.len() > 0 {
                conn.want_read = false;
                conn.want_write = true;

                // println!("Response total size: {}", conn.outgoing_data.len());

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

    let total_data_len = get_header(&conn.incoming_data.as_slice()[0..MSG_HEADER_SIZE]);
    println!(
        "CAME LEN: {:?} {} {}",
        &conn.incoming_data.len(),
        total_data_len,
        conn.incoming_data.len()
    );

    if conn.incoming_data.len() < total_data_len {
        return false;
    }

    if MAX_MSG_SIZE < conn.incoming_data.len() {
        return false;
    }

    let commands_len =
        get_header(&conn.incoming_data.as_slice()[MSG_HEADER_SIZE..MSG_HEADER_SIZE * 2]);

    if commands_len > MAX_COMMANDS_SIZE {
        conn.want_close = true;
        return false;
    };

    let mut cmd: Vec<String> = Vec::with_capacity(commands_len);
    let parsed_size = parse_request(conn.incoming_data.as_slice(), &mut cmd);
    if parsed_size == 0 {
        conn.want_close = true;
        return false;
    }
    println!("Out: {:?}", cmd.len());

    let mut offset = 0usize;
    while offset < cmd.len() {
        let header = response_begin(&mut conn.outgoing_data);
        println!("Header: {header}, {}", conn.outgoing_data.len());
        process_request(&cmd[offset..], &mut offset, &mut conn.outgoing_data);
        response_end(&mut conn.outgoing_data, header);
    }

    buf_consume(&mut conn.incoming_data, parsed_size);

    return true;
}

pub fn parse_request(data: &[u8], out: &mut Vec<String>) -> usize {
    let mut offset = 0; //INFO: We want offset after every read from data, in order to not track some shit sizes
    let commands_number = get_header(&data[offset + MSG_HEADER_SIZE..MSG_HEADER_SIZE * 2]);
    offset += MSG_HEADER_SIZE * 2;

    println!("Commands number: {commands_number} {}", out.len());
    if commands_number == 0 {
        return 0;
    };

    if commands_number > MAX_COMMANDS_SIZE {
        return 0;
    };

    //INFO: Especially there
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
    let mut offset = MSG_HEADER_SIZE * 2;
    final_buffer[MSG_HEADER_SIZE..MSG_HEADER_SIZE + MSG_HEADER_SIZE]
        .copy_from_slice(&commands.len().to_ne_bytes()); //set offset

    for c in commands.iter() {
        println!("C: {}", c.len());
        final_buffer[offset..offset + MSG_HEADER_SIZE].copy_from_slice(&c.len().to_ne_bytes());
        offset += MSG_HEADER_SIZE;

        final_buffer[offset..(offset + c.len())].copy_from_slice(c.as_bytes());
        offset += c.len();
    }

    println!("SEND LEN: {offset}");
    final_buffer[0..MSG_HEADER_SIZE].copy_from_slice(&offset.to_ne_bytes()); //set offset
    println!(
        "BYTES: {:?} {}",
        &final_buffer[0..offset].len(),
        commands.len()
    );

    let wsize = write_full(stream, &final_buffer[0..offset], offset);
    wsize
}

pub fn process_request(cmd: &[String], offset: &mut usize, out: &mut Vec<u8>) {
    let command = cmd[0].as_str();
    match command {
        "get" => {
            println!("Processing get req: {:?}", &cmd);
            *offset += 2;
            do_get(cmd, out)
        }
        "set" => {
            println!("Processing set req: {:?}", &cmd);
            *offset += 3;
            do_set(cmd, out)
        }
        "del" => {
            println!("Processing del req: {:?}", &cmd);
            *offset += 2;
            do_del(cmd, out)
        }
        _ => {}
    }
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
    let rsize = read_full(
        stream,
        &mut bytes[MSG_HEADER_SIZE..MSG_HEADER_SIZE + response_size],
        response_size,
    );
    if rsize == 0 {
        return 0;
    }

    println!(
        "Response size(no header): {}, total: {}",
        rsize,
        MSG_HEADER_SIZE + rsize
    );

    let data_type_code =
        DataType::from_ne_bytes(&bytes[MSG_HEADER_SIZE..MSG_HEADER_SIZE + DATA_TYPE_SIZE]);
    let response_code = ResponseCode::from_ne_bytes(
        &bytes[MSG_HEADER_SIZE + DATA_TYPE_SIZE
            ..MSG_HEADER_SIZE + DATA_TYPE_SIZE + RESPONSE_CODE_SIZE],
    );
    let offset = MSG_HEADER_SIZE + DATA_TYPE_SIZE + RESPONSE_CODE_SIZE;
    match data_type_code {
        DataType::NIL => {
            println!("(nil)");
        }
        DataType::ERR => {
            let message = get_str(&bytes[offset..]);
            println!("Response: error: {:?}, message: {}", response_code, message);
        }
        DataType::STR => {
            let message = get_str(&bytes[offset..]);
            println!("Response: code: {:?}, message: {}", response_code, message);
        }
        DataType::INT => {
            let mut int_b = [0u8; size_of::<i32>()];
            int_b.copy_from_slice(&bytes[offset..offset + size_of::<i32>()]);
            let v = i32::from_ne_bytes(int_b);
            println!("Response: code: {:?}, value(int): {}", response_code, v);
        }
        DataType::FLT => {
            let mut int_b = [0u8; size_of::<f64>()];
            int_b.copy_from_slice(&bytes[offset..offset + size_of::<f64>()]);
            let v = f64::from_ne_bytes(int_b);
            println!("Response: code: {:?}, value(float): {}", response_code, v);
        }
        DataType::ARR => {
            unimplemented!();
        }
    };
    rsize
}

pub fn get_str(bytes: &[u8]) -> String {
    let mut size_b = [0u8; size_of::<usize>()];
    size_b.copy_from_slice(&bytes[0..size_of::<usize>()]);

    let str_size = usize::from_ne_bytes(size_b);

    get_message(&bytes[..], size_of::<usize>(), str_size)
}
