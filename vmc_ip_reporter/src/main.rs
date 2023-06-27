use network_interface::{Addr, NetworkInterface, NetworkInterfaceConfig};
use std::fs::File;
use std::io::Read;
use std::net::TcpStream;
use std::path::Path;
use std::{str, thread, time};
use vmc_common::protocol::server_negotiation;
use vmc_common::types::PortforwardList;
use vmc_common::{
    protocol::{NSRequest, Request},
    types::{AutoReConnectTcpStream, MachineInfo, SerializedDataContainer},
    ETH_NAME, IPV4_PREFIX_LIST, IPV6_PREFIX, SERVER_HOST, SERVER_PORT,
};

#[cfg(not(target_os = "windows"))]
static PORT_FORWARD_FILE_PATH: &str = "/etc/vmc_port_forward.json";
#[cfg(target_os = "windows")]
static PORT_FORWARD_FILE_PATH: &str = "C:\\etc\\vmc_port_forward.json";

fn get_ipv4addr(eth_name: &str) -> Option<String> {
    let network_interfaces = NetworkInterface::show().unwrap();

    for itf in network_interfaces.iter() {
        if itf.name == eth_name {
            for addr in itf.addr.iter() {
                if let Addr::V4(ipv4_addr) = addr {
                    let ipv4_addr = &ipv4_addr.ip.to_string();
                    for ip_prefix in IPV4_PREFIX_LIST.iter() {
                        if ipv4_addr.starts_with(ip_prefix) {
                            return Some(ipv4_addr.clone());
                        }
                    }
                }
            }
        }
    }

    None
}

fn get_ipv6addr(eth_name: &str) -> Option<String> {
    let network_interfaces = NetworkInterface::show().unwrap();

    for itf in network_interfaces.iter() {
        if itf.name == eth_name {
            for addr in itf.addr.iter() {
                if let Addr::V6(ipv6_addr) = addr {
                    let ipv6_addr = &ipv6_addr.ip.to_string();
                    if ipv6_addr.starts_with(IPV6_PREFIX) {
                        return Some(format!("{ipv6_addr}%{eth_name}"));
                    }
                }
            }
        }
    }

    None
}

fn get_hostname() -> Option<String> {
    hostname::get().ok().map(|os_str| {
        os_str
            .into_string()
            .expect("failed to convert OsString into String")
    })
}

fn get_port_forward_list() -> PortforwardList {
    let path = Path::new(PORT_FORWARD_FILE_PATH);

    if let Ok(mut file) = File::open(path) {
        let mut s = String::new();
        file.read_to_string(&mut s).unwrap();
        serde_json::from_str::<PortforwardList>(&s).expect("failed to parse forward list")
    } else {
        PortforwardList::new(vec![])
    }
}

fn main() -> std::io::Result<()> {
    let sleep_sec = time::Duration::from_secs(30);
    let mut server = AutoReConnectTcpStream::new(
        format!("{SERVER_HOST}:{SERVER_PORT}"),
        sleep_sec,
        Some(Box::new(|mut stream: TcpStream| {
            if !server_negotiation(&mut stream) {
                panic!("protocol version mismatched");
            }
        })),
    );
    server.set_verbosity(true);

    if !server_negotiation(&mut server.stream) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::ConnectionRefused,
            "protocol version mismatched",
        ));
    }

    loop {
        let (hostname, ipv4_addr, ipv6_addr) = (
            get_hostname().expect("failed to get hostname"),
            get_ipv4addr(ETH_NAME).expect("failed to get ipv4 addr"),
            get_ipv6addr(ETH_NAME),
        );
        let m = Request::NameService(NSRequest::Heartbeat(
            MachineInfo {
                hostname,
                ipv4_addr,
                ipv6_addr,
            },
            get_port_forward_list(),
        ));
        let sdc = SerializedDataContainer::from_serializable_data(&m).unwrap();

        println!("Send heartbeat to server. msg: {m:?}, bytes: {sdc:?}");

        server.write_all(&sdc.to_one_vec()).unwrap();

        thread::sleep(sleep_sec);
    }
}
