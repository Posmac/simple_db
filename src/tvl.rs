use core::mem::size_of;

use crate::common::{MAX_MSG_SIZE, MSG_HEADER_SIZE};

pub const RESPONSE_CODE_SIZE: usize = size_of::<i32>();
pub const DATA_TYPE_SIZE: usize = size_of::<i8>();

#[derive(Default, Debug)]
#[repr(i32)]
pub enum ResponseCode {
    #[default]
    Ok = 0,
    Error = 1,
    SetNew = 2,
    SetExisting = 3,
    GetExisting = 4,
    GetNothing = 5,
    DelOk = 6,
    DelNothing = 7,
}

impl ResponseCode {
    pub fn to_ne_bytes(self) -> [u8; 4] {
        (self as i32).to_ne_bytes()
    }

    pub fn from_ne_bytes(bytes: &[u8]) -> Self {
        let mut b = [0u8; RESPONSE_CODE_SIZE];
        b.copy_from_slice(bytes);
        let u_32 = u32::from_ne_bytes(b);
        match u_32 {
            0 => Self::Ok,
            1 => Self::Error,
            2 => Self::SetNew,
            3 => Self::SetExisting,
            4 => Self::GetExisting,
            5 => Self::GetNothing,
            6 => Self::DelOk,
            7 => Self::DelNothing,
            _ => unreachable!(),
        }
    }
}

#[derive(Default, Debug)]
#[repr(u8)]
pub enum DataType {
    #[default]
    NIL = 0, //
    ERR = 1, // Response
    STR = 2, // String
    INT = 3, // i32
    FLT = 4, // f64
    ARR = 5, // Vec<DataType>
}

impl DataType {
    pub fn to_ne_bytes(self) -> [u8; 1] {
        (self as u8).to_ne_bytes()
    }

    pub fn from_ne_bytes(bytes: &[u8]) -> Self {
        let mut b = [0u8; DATA_TYPE_SIZE];
        b.copy_from_slice(bytes);
        let u_8 = u8::from_ne_bytes(b);
        match u_8 {
            0 => Self::NIL,
            1 => Self::ERR,
            2 => Self::STR,
            3 => Self::INT,
            4 => Self::FLT,
            5 => Self::ARR,
            _ => unreachable!(),
        }
    }
}

pub fn response_begin(buf: &mut Vec<u8>) -> usize {
    let header = buf.len();
    buf_append(buf, &0usize.to_ne_bytes(), MSG_HEADER_SIZE);
    return header;
}

pub fn response_size(buf: &mut Vec<u8>, header: usize) -> usize {
    return buf.len() - header - MSG_HEADER_SIZE;
}

pub fn response_end(buf: &mut Vec<u8>, header: usize) {
    let mut msg_size = response_size(buf, header);
    if msg_size > MAX_MSG_SIZE {
        buf.resize(MSG_HEADER_SIZE + RESPONSE_CODE_SIZE, Default::default());
        out_err(buf, ResponseCode::Error, "response is too big!");
        msg_size = response_size(buf, header)
    }

    buf[header..header + MSG_HEADER_SIZE].copy_from_slice(&msg_size.to_ne_bytes());
}

pub fn out_nil(buf: &mut Vec<u8>, code: ResponseCode) {
    buf_append(buf, &DataType::NIL.to_ne_bytes(), DATA_TYPE_SIZE);
    buf_append(buf, &code.to_ne_bytes(), RESPONSE_CODE_SIZE);
}

pub fn out_str(buf: &mut Vec<u8>, str: &str, code: ResponseCode) {
    buf_append(buf, &DataType::STR.to_ne_bytes(), DATA_TYPE_SIZE);
    buf_append(buf, &code.to_ne_bytes(), RESPONSE_CODE_SIZE);
    buf_append(buf, &str.len().to_ne_bytes(), size_of::<usize>());
    buf_append(buf, str.as_bytes(), str.len());
}

pub fn out_int32(buf: &mut Vec<u8>, val: i32, code: ResponseCode) {
    buf_append(buf, &DataType::INT.to_ne_bytes(), DATA_TYPE_SIZE);
    buf_append(buf, &code.to_ne_bytes(), RESPONSE_CODE_SIZE);
    buf_append(buf, &val.to_ne_bytes(), size_of::<i32>());
}

pub fn out_float32(buf: &mut Vec<u8>, val: f32, code: ResponseCode) {
    buf_append(buf, &DataType::INT.to_ne_bytes(), DATA_TYPE_SIZE);
    buf_append(buf, &code.to_ne_bytes(), RESPONSE_CODE_SIZE);
    buf_append(buf, &val.to_ne_bytes(), size_of::<f32>());
}

pub fn out_err(buf: &mut Vec<u8>, code: ResponseCode, str: &str) {
    buf_append(buf, &DataType::ERR.to_ne_bytes(), DATA_TYPE_SIZE);
    buf_append(buf, &code.to_ne_bytes(), RESPONSE_CODE_SIZE);
    buf_append(buf, &str.len().to_ne_bytes(), size_of::<usize>());
    buf_append(buf, str.as_bytes(), str.len());
}

pub fn out_arr(buf: &mut Vec<u8>, size: usize, code: ResponseCode) {
    buf_append(buf, &DataType::ARR.to_ne_bytes(), DATA_TYPE_SIZE);
    buf_append(buf, &code.to_ne_bytes(), RESPONSE_CODE_SIZE);
    buf_append(buf, &size.to_ne_bytes(), size_of::<usize>());
}

pub fn buf_append(buf: &mut Vec<u8>, data: &[u8], len: usize) {
    buf.extend_from_slice(&data[0..len]);
}

pub fn buf_consume(buf: &mut Vec<u8>, len: usize) {
    buf.drain(0..len);
}
