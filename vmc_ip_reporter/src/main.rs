use std::{fs::File, io::prelude::*, path::Path};
use std::{process::Command, str};
use std::{thread, time};
use vmc_common::{
    MachineInfo, NSRequest, Request, SerializedDataContainer, SERVER_HOST, SERVER_PORT, AutoReConnectTcpStream,
};

fn get_ipaddr(eth_name: &str) -> Option<String> {
    let output = Command::new("sh")
        .arg("-c")
        .arg(format!("ip addr show dev {eth_name} | grep inet"))
        .output()
        .expect("failed to execute process")
        .stdout;
    let output: Vec<_> = str::from_utf8(&output).unwrap().split(' ').collect();

    for e in &output {
        if e.starts_with("172") || e.starts_with("192") {
            return Some(e[0..e.len() - 3].to_string());
        }
    }

    None
}

fn get_hostname() -> Option<String> {
    let path = Path::new("/etc/hostname");

    if let Ok(mut file) = File::open(path) {
        let mut s = String::new();
        file.read_to_string(&mut s).unwrap();
        return Some(s.trim().to_string());
    }

    None
}

fn main() -> std::io::Result<()> {
    let sleep_sec = time::Duration::from_secs(30);
    let mut sock = AutoReConnectTcpStream::new(format!("{SERVER_HOST}:{SERVER_PORT}"), sleep_sec);
    sock.set_verbosity(true);

    loop {
        let (ipaddr, hostname) = (get_ipaddr("eth0").unwrap(), get_hostname().unwrap());
        let m = Request::NameService(NSRequest::Heartbeat(MachineInfo { hostname, ipaddr }));
        let sdc = SerializedDataContainer::from_serializable_data(&m).unwrap();

        println!("Send heartbeat to server with {sdc:?}");

        sock.write_all(&sdc.to_one_vec()).unwrap();

        thread::sleep(sleep_sec);
    }
}

