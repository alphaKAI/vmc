pub mod port_forward;

use chrono::Local;
use std::io::prelude::*;
use std::{
    collections::HashMap,
    net::TcpListener,
    process::Command,
    sync::{Arc, Mutex},
    thread,
};
use vmc_common::protocol::calc_protocol_digest;
use vmc_common::types::PortforwardList;
use vmc_common::{
    protocol::{
        CBRequest, CBResponse, ExecRequest, ExecResponse, NSRequest, NSResponse, NTFRequest,
        Request, Response,
    },
    types::{MachineInfo, SerializedDataContainer},
};
use winrt_notification::Toast;

use log::info;
use std::env;

use crate::port_forward::spawn_new_port_forward_thread;

#[derive(Debug)]
struct IpAddrPair {
    pub ipv4_addr: String,
    pub ipv6_addr: Option<String>,
}

impl IpAddrPair {
    pub fn new(ipv4_addr: String, ipv6_addr: Option<String>) -> Self {
        Self {
            ipv4_addr,
            ipv6_addr,
        }
    }
}

#[derive(Debug, Default)]
struct MachineMap {
    map: HashMap<String, IpAddrPair>,
}

impl MachineMap {
    fn insert(&mut self, hostname: String, ipaddr_pair: IpAddrPair) {
        self.map.insert(hostname, ipaddr_pair);
    }

    fn get(&self, hostname: &str) -> Option<&IpAddrPair> {
        self.map.get(hostname)
    }

    fn iter(&self) -> std::collections::hash_map::Iter<String, IpAddrPair> {
        self.map.iter()
    }
}

static SERVER_ADDR: &str = "0.0.0.0:12345";

fn main() {
    env::set_var("RUST_LOG", "info");
    env_logger::init();

    let server = TcpListener::bind(SERVER_ADDR).expect("Could not bind socket");

    info!("Server is started with {} !", SERVER_ADDR);

    let mmap = Arc::new(Mutex::new(MachineMap::default()));

    let forward_map: Arc<Mutex<HashMap<String, PortforwardList>>> =
        Arc::new(Mutex::new(HashMap::new()));

    for client in server.incoming().flatten() {
        let mmap = mmap.clone();
        let mut client = client.try_clone().unwrap();
        let forward_map = forward_map.clone();
        thread::spawn(move || {
            if let Ok(sdc) = SerializedDataContainer::from_reader(&mut client) {
                let req = sdc.to_serializable_data::<Request>().unwrap();
                if let Request::Negotiation(client_digesst) = req {
                    let server_digest = calc_protocol_digest();

                    let digest_match = client_digesst == server_digest;

                    client
                        .write_all(
                            &SerializedDataContainer::from_serializable_data(
                                &Response::NegotiationResult(digest_match),
                            )
                            .unwrap()
                            .to_one_vec(),
                        )
                        .unwrap();

                    if !digest_match {
                        info!("version mismatched.");
                        return;
                    }
                } else {
                    info!("Wrong connection. client must send an negotiation packet at first.");
                    return;
                }
            } else {
                println!("Connection closed.");
                return;
            }

            loop {
                info!("[{}] Data arrives from {:?}", Local::now(), client);

                if let Ok(sdc) = SerializedDataContainer::from_reader(&mut client) {
                    let req = sdc.to_serializable_data::<Request>().unwrap();
                    match req {
                        Request::Negotiation(_) => {
                            client
                                .write_all(
                                    &SerializedDataContainer::from_serializable_data(
                                        &Response::NegotiationResult(true),
                                    )
                                    .unwrap()
                                    .to_one_vec(),
                                )
                                .unwrap();
                        }
                        Request::NameService(ns) => match ns {
                            NSRequest::Heartbeat(mi, given_forward_list) => {
                                info!("NSRequest::Heartbeat({mi:?}, {given_forward_list:?})");
                                let mut mmap = mmap.lock().unwrap();
                                info!("New MachineInfo registered! : {:?}", &mi);
                                mmap.insert(
                                    mi.hostname.clone(),
                                    IpAddrPair::new(mi.ipv4_addr.clone(), mi.ipv6_addr),
                                );

                                let new_forward_list = {
                                    let mut forward_map = forward_map
                                        .lock()
                                        .expect("failed to aquire lock of forward_map");
                                    if !forward_map.contains_key(&mi.hostname) {
                                        forward_map.insert(
                                            mi.hostname.clone(),
                                            PortforwardList::new(vec![]),
                                        );
                                    }
                                    let forward_list = forward_map.get_mut(&mi.hostname).unwrap();
                                    forward_list.merge_elem(&given_forward_list)
                                };

                                for forward in new_forward_list.iter() {
                                    let src_port = forward.host_port;
                                    let dst_ip = mi.ipv4_addr.clone();
                                    let dst_port = forward.guest_port;

                                    spawn_new_port_forward_thread(src_port, dst_ip, dst_port);
                                }
                            }
                            NSRequest::QueryIp(hostname) => {
                                info!("NSRequest::QueryIp({hostname:?})");
                                let mmap = mmap.lock().unwrap();
                                let msg = Response::NameService(NSResponse::Ip(
                                    mmap.get(&hostname).map(|ipaddr_pair| MachineInfo {
                                        hostname: hostname.clone(),
                                        ipv4_addr: ipaddr_pair.ipv4_addr.clone(),
                                        ipv6_addr: ipaddr_pair.ipv6_addr.clone(),
                                    }),
                                ));
                                info!("Queired from client: {:?}", msg);
                                client
                                    .write_all(
                                        &SerializedDataContainer::from_serializable_data(
                                            &Response::NameService(NSResponse::Ip(
                                                mmap.get(&hostname).map(|ipaddr_pair| {
                                                    MachineInfo {
                                                        hostname,
                                                        ipv4_addr: ipaddr_pair.ipv4_addr.clone(),
                                                        ipv6_addr: ipaddr_pair.ipv6_addr.clone(),
                                                    }
                                                }),
                                            )),
                                        )
                                        .unwrap()
                                        .to_one_vec(),
                                    )
                                    .unwrap();
                            }
                            NSRequest::GetMachineList => {
                                info!("NSRequest::GetMachineList");
                                let mut machines = vec![];

                                let mmap = mmap.lock().unwrap();
                                for (k, v) in mmap.iter() {
                                    let mi = MachineInfo {
                                        hostname: k.to_string(),
                                        ipv4_addr: v.ipv4_addr.to_string(),
                                        ipv6_addr: v.ipv6_addr.clone(),
                                    };
                                    machines.push(mi);
                                }

                                info!("Requst MachineList from client: {:?}", client);

                                client
                                    .write_all(
                                        &SerializedDataContainer::from_serializable_data(
                                            &Response::NameService(NSResponse::MachineList(
                                                machines,
                                            )),
                                        )
                                        .unwrap()
                                        .to_one_vec(),
                                    )
                                    .unwrap();
                            }
                        },
                        Request::ClipBoard(cb) => match cb {
                            CBRequest::SetClipboard(s) => {
                                info!("CBRequest::SetClipboard({s})");
                                if cli_clipboard::set_contents(s).is_err() {
                                    eprintln!("Failed to set a data to ClipBoard.");
                                }
                            }
                            CBRequest::GetClipboard => {
                                info!("CBRequest::GetClipboard");

                                let cb_content =
                                    cli_clipboard::get_contents().unwrap_or_else(|_| String::new());

                                client
                                    .write_all(
                                        &SerializedDataContainer::from_serializable_data(
                                            &Response::ClipBoard(CBResponse::GetClipboard(
                                                cb_content,
                                            )),
                                        )
                                        .unwrap()
                                        .to_one_vec(),
                                    )
                                    .unwrap();
                            }
                        },
                        Request::Execute(exec) => match exec {
                            ExecRequest::Execute(args) => {
                                info!("ExecRequest::Execute({args:?})");

                                let mut cmd_args = vec!["/C", "start"];
                                for arg in args.iter() {
                                    cmd_args.push(arg.as_str());
                                }

                                let _ = Command::new("cmd")
                                    .args(cmd_args)
                                    .output()
                                    .expect("failed to execute process");
                            }
                            ExecRequest::Open(path) => {
                                info!("ExecRequest::Open({path})");
                                let path = path.replace('/', "\\");

                                let _ = Command::new("cmd")
                                    .args(vec!["/C", "start", &path])
                                    .output()
                                    .expect("failed to execute process");
                            }
                            ExecRequest::GetEnvVar(key) => {
                                info!("ExecRequest::GetEnvVar({key})");

                                let val = env::var(key).ok();

                                client
                                    .write_all(
                                        &SerializedDataContainer::from_serializable_data(
                                            &Response::Execute(ExecResponse::GetEnvVar(val)),
                                        )
                                        .unwrap()
                                        .to_one_vec(),
                                    )
                                    .unwrap();
                            }
                        },
                        Request::Notification(ntf) => match ntf {
                            NTFRequest::Notification(title, body) => {
                                info!("NTFRequest::Notification({title:?}, {body})");
                                let title = title.unwrap_or("Notification".to_string());

                                Toast::new(Toast::POWERSHELL_APP_ID)
                                    .title(&title)
                                    .text1(&body)
                                    .show()
                                    .expect("unable to toast");
                            }
                        },
                    }
                } else {
                    info!("Connection closed.");
                    return;
                }
            }
        });
    }
}
