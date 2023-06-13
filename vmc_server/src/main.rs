use chrono::Local;
use std::io::prelude::*;
use std::{
    collections::HashMap,
    net::TcpListener,
    process::Command,
    sync::{Arc, Mutex},
    thread,
};
use vmc_common::{
    CBResponse, MachineInfo, NSRequest, NSResponse, Request, Response, SerializedDataContainer,
};

use log::info;
use std::env;

#[derive(Debug, Default)]
struct MachineMap {
    map: HashMap<String, String>,
}

impl MachineMap {
    fn insert(&mut self, hostname: String, ipaddr: String) {
        self.map.insert(hostname, ipaddr);
    }

    fn get(&self, hostname: &str) -> Option<&String> {
        self.map.get(hostname)
    }

    fn iter(&self) -> std::collections::hash_map::Iter<String, String> {
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

    for client in server.incoming().flatten() {
        let mmap = mmap.clone();
        let mut client = client.try_clone().unwrap();
        thread::spawn(move || loop {
            info!("[{}] Data arrives from {:?}", Local::now(), client);

            if let Ok(sdc) = SerializedDataContainer::from_reader(&mut client) {
                match sdc.to_serializable_data::<Request>().unwrap() {
                    Request::NameService(ns) => match ns {
                        NSRequest::Heartbeat(mi) => {
                            let mut mmap = mmap.lock().unwrap();
                            info!("New MachineInfo registered! : {:?}", &mi);
                            mmap.insert(mi.hostname, mi.ipaddr);
                        }
                        NSRequest::QueryIp(hostname) => {
                            let mmap = mmap.lock().unwrap();
                            let msg = Response::NameService(NSResponse::Ip(
                                mmap.get(&hostname).map(|ipaddr| MachineInfo {
                                    hostname: hostname.clone(),
                                    ipaddr: ipaddr.clone(),
                                }),
                            ));
                            info!("Queired from client: {:?}", msg);
                            client
                                .write_all(
                                    &SerializedDataContainer::from_serializable_data(
                                        &Response::NameService(NSResponse::Ip(
                                            mmap.get(&hostname).map(|ipaddr| MachineInfo {
                                                hostname,
                                                ipaddr: ipaddr.clone(),
                                            }),
                                        )),
                                    )
                                    .unwrap()
                                    .to_one_vec(),
                                )
                                .unwrap();
                        }
                        NSRequest::GetMachineList => {
                            let mut machines = vec![];

                            let mmap = mmap.lock().unwrap();
                            for (k, v) in mmap.iter() {
                                let mi = MachineInfo {
                                    hostname: k.to_string(),
                                    ipaddr: v.to_string(),
                                };
                                machines.push(mi);
                            }

                            info!("Requst MachineList from client: {:?}", client);

                            client
                                .write_all(
                                    &SerializedDataContainer::from_serializable_data(
                                        &Response::NameService(NSResponse::MachineList(machines)),
                                    )
                                    .unwrap()
                                    .to_one_vec(),
                                )
                                .unwrap();
                        }
                    },
                    Request::ClipBoard(cb) => match cb {
                        vmc_common::CBRequest::SetClipboard(s) => {
                            info!("Request SetClipboard");
                            if cli_clipboard::set_contents(s).is_err() {
                                eprintln!("Failed to set a data to ClipBoard.");
                            }
                        }
                        vmc_common::CBRequest::GetClipboard => {
                            info!("Request GetClipboard");

                            let cb_content =
                                cli_clipboard::get_contents().unwrap_or_else(|_| String::new());

                            client
                                .write_all(
                                    &SerializedDataContainer::from_serializable_data(
                                        &Response::ClipBoard(CBResponse::GetClipboard(cb_content)),
                                    )
                                    .unwrap()
                                    .to_one_vec(),
                                )
                                .unwrap();
                        }
                    },
                    Request::Execute(exec) => match exec {
                        vmc_common::ExecRequest::Execute(args) => {
                            let mut cmd_args = vec!["/C", "start"];
                            for arg in args.iter() {
                                cmd_args.push(arg.as_str());
                            }

                            let _ = Command::new("cmd")
                                .args(cmd_args)
                                .output()
                                .expect("failed to execute process");
                        }
                        vmc_common::ExecRequest::Open(path) => {
                            let path = path.replace('/', "\\");

                            let _ = Command::new("cmd")
                                .args(vec!["/C", "start", &path])
                                .output()
                                .expect("failed to execute process");
                        }
                    },
                }
            } else {
                info!("Connection closed.");
                return;
            }
        });
    }
}

