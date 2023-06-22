use std::{io::Write, net::TcpStream};

use crate::types::{MachineInfo, SerializedDataContainer};
use ring::digest::{Context, SHA256};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub enum NSRequest {
    Heartbeat(MachineInfo),
    QueryIp(String),
    GetMachineList,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum NSResponse {
    Ip(Option<MachineInfo>),
    MachineList(Vec<MachineInfo>),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum CBRequest {
    SetClipboard(String),
    GetClipboard,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum CBResponse {
    GetClipboard(String),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ExecRequest {
    Execute(Vec<String>),
    Open(String),
    GetEnvVar(String),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ExecResponse {
    GetEnvVar(Option<String>),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum NTFRequest {
    Notification(Option<String>, String),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Request {
    Negotiation(Vec<u8>), // 256(SHA256 bits) / 8 = 32 byte
    NameService(NSRequest),
    ClipBoard(CBRequest),
    Execute(ExecRequest),
    Notification(NTFRequest),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Response {
    NegotiationResult(bool),
    NameService(NSResponse),
    ClipBoard(CBResponse),
    Execute(ExecResponse),
}

const PROTOCOL_SRC: &str = include_str!("protocol.rs");

pub fn calc_protocol_digest() -> Vec<u8> {
    let mut ctx = Context::new(&SHA256);

    ctx.update(PROTOCOL_SRC.as_bytes());

    ctx.finish().as_ref().to_vec()
}

pub fn server_negotiation(server: &mut TcpStream) -> bool {
    let client_digest = calc_protocol_digest();

    server
        .write_all(
            &SerializedDataContainer::from_serializable_data(&Request::Negotiation(client_digest))
                .unwrap()
                .to_one_vec(),
        )
        .unwrap();

    let sdc = SerializedDataContainer::from_reader(server).unwrap();
    if let Response::NegotiationResult(result) = sdc.to_serializable_data::<Response>().unwrap() {
        result
    } else {
        false
    }
}
