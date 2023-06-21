use rand::prelude::*;
use rmp_serde::{self, Serializer};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::thread;
use std::{convert::TryInto, mem::size_of};

include!("SERVER_CONFIG.rs");
pub const CLIENT_REPORTER_PORT: u16 = 54000;
pub const CLIENT_QUERY_PORT: u16 = 54001;
pub const CLIENT_PORT_BASE: u16 = 54002;
pub const CLIENT_PORT_MAX: u16 = 55000;

pub fn get_client_addr(client_ip: &str) -> String {
    let mut rng = rand::thread_rng();
    let port: u16 = rng.gen_range(CLIENT_PORT_BASE..CLIENT_PORT_MAX);

    format!("{client_ip}:{port}")
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MachineInfo {
    pub hostname: String,
    pub ipv4_addr: String,
    pub ipv6_addr: Option<String>,
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
    NameService(NSRequest),
    ClipBoard(CBRequest),
    Execute(ExecRequest),
    Notification(NTFRequest),
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

#[derive(Debug)]
pub struct AutoReConnectTcpStream {
    host_info: String,
    retry_interval: std::time::Duration,
    pub stream: TcpStream,
    verbose: bool,
}

impl AutoReConnectTcpStream {
    pub fn new(host_info: String, retry_interval: std::time::Duration) -> Self {
        let stream = Self::get_connection(&host_info, retry_interval, false);
        Self {
            host_info,
            retry_interval,
            stream,
            verbose: false,
        }
    }

    pub fn set_verbosity(&mut self, v: bool) {
        self.verbose = v
    }

    fn get_connection(
        host_info: &str,
        retry_interval: std::time::Duration,
        verbose: bool,
    ) -> TcpStream {
        loop {
            if verbose {
                println!("Connecting to {host_info} ...");
            }
            if let Ok(new_sock) = TcpStream::connect(host_info) {
                if verbose {
                    println!(" -> Connected!");
                }
                return new_sock;
            } else {
                println!(" -> Retry to connect after {retry_interval:?}");
                thread::sleep(retry_interval);
            }
        }
    }

    pub fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()> {
        loop {
            if self.stream.write_all(buf).is_err() {
                self.stream =
                    Self::get_connection(&self.host_info, self.retry_interval, self.verbose)
            } else {
                break;
            }
        }
        Ok(())
    }
}

