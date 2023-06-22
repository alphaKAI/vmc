#![feature(io_error_more)]
use std::io::prelude::*;
use std::net::TcpStream;
use vmc_common::{
    protocol::{server_negotiation, NSRequest, NSResponse, Request, Response},
    types::SerializedDataContainer,
    SERVER_HOST, SERVER_PORT,
};

fn normalize_ipv6(ipv6_addr: &str) -> String {
    #[cfg(target_os = "windows")]
    {
        if let Some(idx) = ipv6_addr.find('%') {
            ipv6_addr[..idx].to_string()
        } else {
            ipv6_addr.to_string()
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        if ipv6_addr.contains('%') {
            ipv6_addr.to_string()
        } else {
            panic!("on unix env, socpe_id(ifname) must be contained.")
        }
    }
}

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Argument required.",
        ));
    }

    #[derive(PartialEq)]
    enum Mode {
        QueryIPv4,
        QueryIPv6,
        QueryIpv6OrV4,
        List,
    }

    let mode = match args[1].as_str() {
        "list" => Mode::List,
        "ip" => Mode::QueryIpv6OrV4,
        "ipv4" => Mode::QueryIPv4,
        "ipv6" => Mode::QueryIPv6,
        _ => {
            panic!("Unkown command was given: {}", args[1]);
        }
    };

    let mut server = TcpStream::connect(format!("{SERVER_HOST}:{SERVER_PORT}"))?;

    if !server_negotiation(&mut server) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::ConnectionRefused,
            "protocol version mismatched",
        ));
    }

    match mode {
        Mode::QueryIPv4 | Mode::QueryIPv6 | Mode::QueryIpv6OrV4 => {
            let q_hostname = args[2].clone();

            server
                .write_all(
                    &SerializedDataContainer::from_serializable_data(&Request::NameService(
                        NSRequest::QueryIp(q_hostname),
                    ))
                    .unwrap()
                    .to_one_vec(),
                )
                .unwrap();
        }
        Mode::List => {
            server
                .write_all(
                    &SerializedDataContainer::from_serializable_data(&Request::NameService(
                        NSRequest::GetMachineList,
                    ))
                    .unwrap()
                    .to_one_vec(),
                )
                .unwrap();
        }
    };

    let sdc = SerializedDataContainer::from_reader(&mut server).unwrap();
    match sdc.to_serializable_data::<Response>().unwrap() {
        Response::NameService(ns_res) => match ns_res {
            NSResponse::Ip(ret) => {
                if let Some(mi) = ret {
                    match mode {
                        Mode::QueryIPv4 => println!("{}", mi.ipv4_addr),
                        Mode::QueryIPv6 => println!(
                            "{}",
                            mi.ipv6_addr
                                .map(|e| normalize_ipv6(&e))
                                .expect("your queried host does not have an ipv6 addr.")
                        ),
                        Mode::QueryIpv6OrV4 => {
                            if let Some(ipv6_addr) = mi.ipv6_addr {
                                let ipv6_addr = normalize_ipv6(&ipv6_addr);
                                println!("{ipv6_addr}");
                            } else {
                                println!("{}", mi.ipv4_addr);
                            }
                        }
                        _ => panic!("never reach here"),
                    }
                } else {
                    eprintln!("your queried hostname is not registered in server");

                    return Err(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        "No such a hostname",
                    ));
                }
            }
            NSResponse::MachineList(machines) => {
                println!("machine list");
                for machine in machines.iter() {
                    print!("{} : {}", machine.hostname, machine.ipv4_addr);
                    if let Some(ipv6_addr) = &machine.ipv6_addr {
                        println!(" ( {ipv6_addr} )");
                    } else {
                        println!();
                    }
                }
            }
        },
        _ => todo!(),
    }

    Ok(())
}
