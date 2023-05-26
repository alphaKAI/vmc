#![feature(io_error_more)]
use std::net::UdpSocket;
use vmc_common::{NSResponse, Request, Response, SerializedDataContainer, SERVER_HOST, SERVER_PORT, NSRequest, CLIENT_QUERY_PORT};

fn main() -> std::io::Result<()> {
    let client_addr = format!("0.0.0.0:{}", CLIENT_QUERY_PORT);

    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Argument required.",
        ));
    }

    let sock = UdpSocket::bind(client_addr)?;

    #[derive(PartialEq)]
    enum Mode {
        Query,
        List,
    }

    let mode = match args[1].as_str() {
        "list" => Mode::List,
        "ip" => Mode::Query,
        _ => {
            panic!("Unkown command was given: {}", args[1]);
        }
    };

    match mode {
        Mode::Query => {
            let q_hostname = args[2].clone();

            sock.send_to(
                &SerializedDataContainer::from_serializable_data(&Request::NameService(
                    NSRequest::QueryIp(q_hostname),
                ))
                .unwrap()
                .to_one_vec(),
                format!("{}:{}", SERVER_HOST, SERVER_PORT),
            )
            .unwrap();
        }
        Mode::List => {
            sock.send_to(
                &SerializedDataContainer::from_serializable_data(&Request::NameService(
                    NSRequest::GetMachineList,
                ))
                .unwrap()
                .to_one_vec(),
                format!("{}:{}", SERVER_HOST, SERVER_PORT),
            )
            .unwrap();
        }
    };

    let mut buf = [0u8; 1024];
    match sock.recv_from(&mut buf) {
        Ok((_, _)) => {
            let sdc = SerializedDataContainer::from_one_vec(Vec::from(buf)).unwrap();
            match sdc.to_serializable_data::<Response>().unwrap() {
                Response::NameService(ns_res) => match ns_res {
                    NSResponse::Ip(ret) => {
                        if let Some(mi) = ret {
                            println!("{}", mi.ipaddr);
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
                            println!("{} : {}", machine.hostname, machine.ipaddr);
                        }
                    }
                },
                _ => todo!()
            }
        }
        Err(_) => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NetworkUnreachable,
                "Failed to recv response from server",
            ));
        }
    }

    Ok(())
}
