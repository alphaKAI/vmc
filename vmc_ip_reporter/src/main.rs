use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::{env, thread, time};
use std::{process::Command, str};
use vmc_common::{
    AutoReConnectTcpStream, MachineInfo, NSRequest, Request, SerializedDataContainer, ETH_NAME,
    FALLBACK_HOST_NAME, IP_PREFIX_LIST, SERVER_HOST, SERVER_PORT,
};

fn get_ipaddr(eth_name: &str) -> Option<String> {
    // iproute2
    let output = Command::new("sh")
        .arg("-c")
        .arg(format!("ip addr show dev {eth_name} | grep inet"))
        .output()
        .expect("failed to execute process")
        .stdout;
    let output: Vec<_> = str::from_utf8(&output).unwrap().split(' ').collect();

    for e in &output {
        for ip_prefix in IP_PREFIX_LIST.iter() {
            if e.starts_with(ip_prefix) {
                return Some(e[0..e.len() - 3].to_string());
            }
        }
    }

    // ifconfig
    let output = Command::new("sh")
        .arg("-c")
        .arg(format!("ifconfig {eth_name} | grep inet"))
        .output()
        .expect("failed to execute process")
        .stdout;
    let output: Vec<_> = str::from_utf8(&output).unwrap().split(' ').collect();

    for e in &output {
        for ip_prefix in IP_PREFIX_LIST.iter() {
            if e.starts_with(ip_prefix) {
                return Some(e[0..e.len() - 3].to_string());
            }
        }
    }

    None
}

fn get_hostname() -> Option<String> {
    match env::var("HOST").ok() {
        Some(hostname) => Some(hostname),
        None => {
            let path = Path::new("/etc/hostname");

            if let Ok(mut file) = File::open(path) {
                let mut s = String::new();
                file.read_to_string(&mut s).unwrap();
                return Some(s.trim().to_string());
            }

            Some(FALLBACK_HOST_NAME.to_string())
        }
    }
}

fn main() -> std::io::Result<()> {
    let sleep_sec = time::Duration::from_secs(30);
    let mut sock = AutoReConnectTcpStream::new(format!("{SERVER_HOST}:{SERVER_PORT}"), sleep_sec);
    sock.set_verbosity(true);

    loop {
        let (ipaddr, hostname) = (get_ipaddr(ETH_NAME).unwrap(), get_hostname().unwrap());
        let m = Request::NameService(NSRequest::Heartbeat(MachineInfo { hostname, ipaddr }));
        let sdc = SerializedDataContainer::from_serializable_data(&m).unwrap();

        println!("Send heartbeat to server with {sdc:?}");

        sock.write_all(&sdc.to_one_vec()).unwrap();

        thread::sleep(sleep_sec);
    }
}
