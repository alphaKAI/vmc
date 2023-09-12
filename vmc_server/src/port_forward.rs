use log::{info, trace};
use std::io::prelude::*;
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

const DEFAULT_BUF_SIZE: usize = 1024;

/*
 *
 * <Typical TCP Connection>
 *   Local TCP Client ---> Remote TCP Server
 * <Proposed Method>
 *   Local TCP Client ---> Redirector TCP Server -------> Redirect TCP Client ------> Remote TCP Server
 * */

fn handle_local_client(
    header: String,
    mut local: TcpStream,
    remote_ip: Ipv4Addr,
    remote_port: u16,
) {
    info!("{header} New client connected! {:?}", local);

    let mut remote = TcpStream::connect(format!("{}:{}", remote_ip, remote_port)).unwrap();
    trace!("{header} Connect to remote is ok!");

    let local_tcp_handler = {
        let mut client = local.try_clone().unwrap();
        let mut remote = remote.try_clone().unwrap();
        let header = header.clone();
        thread::spawn(move || {
            let mut buf: [u8; DEFAULT_BUF_SIZE] = [0; DEFAULT_BUF_SIZE];
            loop {
                let n = client.read(&mut buf).unwrap_or(0);

                trace!("{header} [CLIENT] read {} bytes from client", n);
                if n == 0 {
                    trace!("{header} [CLIENT] DISCONNECT!");
                    break;
                }

                let _w = remote.write(&buf[..n]).unwrap();
                trace!("{header} [CLIENT] write {} bytes to remote", _w);
            }
        })
    };

    let mut buf: [u8; DEFAULT_BUF_SIZE] = [0; DEFAULT_BUF_SIZE];
    loop {
        let n = remote.read(&mut buf).unwrap_or(0);
        trace!("{header} [REMOTE] read {} bytes from remote", n);

        if n == 0 {
            trace!("{header} [REMOTE] DISCONNECT!");
            break;
        }

        let _w = local.write(&buf[..n]).unwrap();
        trace!("{header} [REMOTE] write {} bytes to client", _w);
    }

    local_tcp_handler.join().unwrap();

    info!("{header} transfer thread is finished!");
}

#[derive(Debug)]
pub enum PortforwardRequest {
    NewClient {
        header: String,
        client: TcpStream,
        dst_ip: Ipv4Addr,
        dst_port: u16,
    },
    StopServer,
}

pub fn spawn_new_port_forward_thread(
    src_port: u16,
    dst_ip: String,
    dst_port: u16,
    req: Sender<PortforwardRequest>,
    recv: Receiver<PortforwardRequest>,
) {
    let header = format!("[PORT FORWARDER (src: 0.0.0.0:{src_port} --> dst: {dst_ip}:{dst_port})]");
    let is_dead = Arc::new(Mutex::new(false));

    info!("{header} spawn new forward thread");
    {
        let is_dead = is_dead.clone();
        thread::spawn(move || {
            let dst_ip: Ipv4Addr = dst_ip.parse().expect("failed to parse ip addr");

            let listner = TcpListener::bind(format!("0.0.0.0:{}", src_port)).unwrap();
            info!("service started");

            for client in listner.incoming() {
                {
                    let is_dead = *is_dead.lock().unwrap();
                    if is_dead {
                        return; // this thread should be die.
                    }
                }
                let client = client.unwrap().try_clone().unwrap();
                let header = header.clone();
                req.send(PortforwardRequest::NewClient {
                    header,
                    client,
                    dst_ip,
                    dst_port,
                })
                .expect("failed to send PortforwardRequest::NewClient");
            }
        });
    }

    thread::spawn(move || loop {
        match recv.recv().expect("failed to unwrap PortforwardRequest") {
            PortforwardRequest::NewClient {
                header,
                client,
                dst_ip,
                dst_port,
            } => {
                thread::spawn(move || handle_local_client(header, client, dst_ip, dst_port));
            }
            PortforwardRequest::StopServer => {
                let mut is_dead = is_dead.lock().unwrap();
                *is_dead = true;
                return;
            }
        }
    });
}
