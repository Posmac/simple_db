pub const MAX_MSG_SIZE: usize = 4096 * 1024;
pub const MSG_HEADER_SIZE: usize = 8;
pub const MAX_COMMANDS_SIZE: usize = 1024;

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
