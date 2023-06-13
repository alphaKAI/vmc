#![feature(io_error_more)]
use std::io::prelude::*;
use std::net::TcpStream;
use vmc_common::{
    NSRequest, NSResponse, Request, Response, SerializedDataContainer, SERVER_HOST, SERVER_PORT,
};

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

    let server_addr = format!("{SERVER_HOST}:{SERVER_PORT}");
    let mut sock = TcpStream::connect(server_addr)?;

    match mode {
        Mode::Query => {
            let q_hostname = args[2].clone();

            sock.write_all(
                &SerializedDataContainer::from_serializable_data(&Request::NameService(
                    NSRequest::QueryIp(q_hostname),
                ))
                .unwrap()
                .to_one_vec(),
            )
            .unwrap();
        }
        Mode::List => {
            sock.write_all(
                &SerializedDataContainer::from_serializable_data(&Request::NameService(
                    NSRequest::GetMachineList,
                ))
                .unwrap()
                .to_one_vec(),
            )
            .unwrap();
        }
    };

    let sdc = SerializedDataContainer::from_reader(&mut sock).unwrap();
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
        _ => todo!(),
    }

    Ok(())
}
