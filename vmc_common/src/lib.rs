use serde::{Serialize, Deserialize};
use rmp_serde::{self, Serializer};
use std::io::Read;
use std::{convert::TryInto, mem::size_of};
use rand::prelude::*;

include!("SERVER_CONFIG.rs");
pub const CLIENT_REPORTER_PORT: u16 = 54000;
pub const CLIENT_QUERY_PORT: u16 = 54001;
pub const CLIENT_PORT_BASE: u16 = 54002;
pub const CLIENT_PORT_MAX: u16 = 55000;

pub fn get_client_addr(client_ip: &str) -> String {
    let mut rng = rand::thread_rng();
    let port: u16 = rng.gen_range(CLIENT_PORT_BASE..CLIENT_PORT_MAX);

    format!("{}:{}", client_ip, port)
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MachineInfo {
    pub hostname: String,
    pub ipaddr: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum NSRequest {
    Heartbeat(MachineInfo),
    QueryIp(String),
    GetMachineList,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum NSResponse {
    Ip(Option<MachineInfo>),
    MachineList(Vec<MachineInfo>)
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
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ExecResponse {
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Request {
    NameService(NSRequest),
    ClipBoard(CBRequest),
    Execute(ExecRequest),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Response {
    NameService(NSResponse),
    ClipBoard(CBResponse),
    Execute(ExecResponse),
}

#[derive(Debug)]
pub struct SerializedDataContainer {
    size: usize,
    data: Vec<u8>,
}

impl SerializedDataContainer {
    pub fn new(v: &[u8]) -> Self {
        Self {
            size: v.len(),
            data: v.to_owned(),
        }
    }

    pub fn to_one_vec(&self) -> Vec<u8> {
        let mut ret = vec![];

        ret.append(&mut self.size.to_le_bytes().to_vec());
        ret.append(&mut self.data.clone());

        ret
    }

    pub fn from_reader<T>(reader: &mut T) -> Result<Self, std::io::Error>
    where
        T: Read,
    {
        let mut size_buffer = [0; size_of::<usize>()];
        reader.read_exact(&mut size_buffer).and_then(|_| {
            let size = usize::from_le_bytes(size_buffer);
            println!("size : {:?}", size);
            let mut data = vec![];

            reader.take(size as u64).read_to_end(&mut data)?;

            Ok(Self { size, data })
        })
    }

    pub fn from_one_vec(v: Vec<u8>) -> Option<Self> {
        if v.len() >= size_of::<usize>() {
            let size = usize::from_le_bytes(
                v[0..size_of::<usize>()]
                    .try_into()
                    .expect("Failed to parse size of the data container"),
            );
            let data = v[size_of::<usize>()..size_of::<usize>() + size]
                .try_into()
                .expect("Failed to get data of the data container");

            Some(Self { size, data })
        } else {
            None
        }
    }

    pub fn from_serializable_data<T>(t: &T) -> Option<Self>
    where
        T: Serialize,
    {
        let mut data = vec![];
        t.serialize(&mut Serializer::new(&mut data)).ok().map(|_| {
            let size = data.len();
            Self { size, data }
        })
    }

    pub fn to_serializable_data<T: for<'de> Deserialize<'de>>(&self) -> Option<T> {
        rmp_serde::from_slice(&self.data).ok()
    }
}

