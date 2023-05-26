use std::net::UdpSocket;
use std::{fs::File, io::prelude::*, path::Path};
use std::{process::Command, str};
use std::{thread, time};
use vmc_common::{
    MachineInfo, NSRequest, Request, SerializedDataContainer, CLIENT_REPORTER_PORT, SERVER_HOST,
    SERVER_PORT,
};

fn get_ipaddr() -> Option<String> {
    let output = Command::new("sh")
        .arg("-c")
        .arg("ip addr show dev eth0 | grep inet")
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
    let server_addr = format!("{}:{}", SERVER_HOST, SERVER_PORT);
    let sock = UdpSocket::bind(format!("0.0.0.0:{}", CLIENT_REPORTER_PORT))?;

    let sleep_sec = time::Duration::from_secs(30);

    loop {
        let (ipaddr, hostname) = (get_ipaddr().unwrap(), get_hostname().unwrap());
        let m = Request::NameService(NSRequest::Heartbeat(MachineInfo { hostname, ipaddr }));
        let sdc = SerializedDataContainer::from_serializable_data(&m).unwrap();

        println!("Send heartbeat to server with {:?}", sdc);

        sock.send_to(&sdc.to_one_vec(), &server_addr).unwrap();

        thread::sleep(sleep_sec);
    }
}
