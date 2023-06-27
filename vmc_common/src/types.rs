use rmp_serde::{self, Serializer};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::thread;
use std::{convert::TryInto, mem::size_of};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MachineInfo {
    pub hostname: String,
    pub ipv4_addr: String,
    pub ipv6_addr: Option<String>,
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

pub struct AutoReConnectTcpStream {
    host_info: String,
    retry_interval: std::time::Duration,
    pub stream: TcpStream,
    verbose: bool,
    reconnect_callback: Option<Box<dyn Fn(TcpStream)>>,
}

impl AutoReConnectTcpStream {
    pub fn new(
        host_info: String,
        retry_interval: std::time::Duration,
        reconnect_callback: Option<Box<dyn Fn(TcpStream)>>,
    ) -> Self {
        let stream = Self::get_connection(
            &host_info,
            retry_interval,
            false,
            reconnect_callback.as_ref(),
        );
        Self {
            host_info,
            retry_interval,
            stream,
            verbose: false,
            reconnect_callback,
        }
    }

    pub fn set_verbosity(&mut self, v: bool) {
        self.verbose = v
    }

    fn get_connection(
        host_info: &str,
        retry_interval: std::time::Duration,
        verbose: bool,
        reconnect_callback: Option<&Box<dyn Fn(TcpStream)>>,
    ) -> TcpStream {
        loop {
            if verbose {
                println!("Connecting to {host_info} ...");
            }
            if let Ok(new_sock) = TcpStream::connect(host_info) {
                if verbose {
                    println!(" -> Connected!");
                    if let Some(cb) = reconnect_callback {
                        cb(new_sock.try_clone().unwrap());
                    }
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
                self.stream = Self::get_connection(
                    &self.host_info,
                    self.retry_interval,
                    self.verbose,
                    self.reconnect_callback.as_ref(),
                )
            } else {
                break;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct PortforwardSpec {
    pub host_port: u16,
    pub guest_port: u16,
}

impl PortforwardSpec {
    pub fn new(host_port: u16, guest_port: u16) -> Self {
        Self {
            host_port,
            guest_port,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PortforwardList {
    pub forwards: Vec<PortforwardSpec>,
}

impl PortforwardList {
    pub fn new(forwards: Vec<PortforwardSpec>) -> Self {
        Self { forwards }
    }

    pub fn has_elem(&self, forward: &PortforwardSpec) -> bool {
        for t_forward in self.forwards.iter() {
            if *t_forward == *forward {
                return true;
            }
        }

        false
    }

    pub fn append_elem(&mut self, forward: PortforwardSpec) {
        self.forwards.push(forward);
    }

    pub fn merge_elem(&mut self, forward_list: &PortforwardList) -> Vec<PortforwardSpec> {
        let mut ret = vec![];

        for forward in forward_list.forwards.iter() {
            if !self.has_elem(forward) {
                self.append_elem(forward.clone());
                ret.push(forward.clone());
            }
        }

        ret
    }
}
