use log::{info, trace};
use std::collections::HashMap;
use std::io::prelude::*;
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::sync::mpsc::{Receiver, Sender};
use std::thread;

const DEFAULT_BUF_SIZE: usize = 1024;

/*
 *
 * <Typical TCP Connection>
 *   Local TCP Client ---> Remote TCP Server
 * <Proposed Method>
 *   Local TCP Client ---> Redirector TCP Server -------> Redirect TCP Client ------> Remote TCP Server
 * */

fn spawn_backend_stream(
    header: String,
    mut front_stream: TcpStream,
    remote_ip: Ipv4Addr,
    remote_port: u16,
) -> TcpStream {
    info!("{header} New client connected! {:?}", front_stream);

    let backend_stream = TcpStream::connect(format!("{}:{}", remote_ip, remote_port)).unwrap();
    trace!("{header} Connect to remote is ok!");

    let local_tcp_handler = {
        let mut front_stream = front_stream.try_clone().unwrap();
        let mut backend_stream = backend_stream.try_clone().unwrap();
        let header = header.clone();
        thread::spawn(move || {
            let mut buf: [u8; DEFAULT_BUF_SIZE] = [0; DEFAULT_BUF_SIZE];
            loop {
                let n = front_stream.read(&mut buf).unwrap_or(0);

                trace!("{header} [CLIENT] read {} bytes from client", n);
                if n == 0 {
                    trace!("{header} [CLIENT] DISCONNECT!");
                    break;
                }

                let _w = backend_stream.write(&buf[..n]).unwrap();
                trace!("{header} [CLIENT] write {} bytes to remote", _w);
            }
        })
    };

    {
        let mut remote = backend_stream.try_clone().unwrap();
        thread::spawn(move || {
            let mut buf: [u8; DEFAULT_BUF_SIZE] = [0; DEFAULT_BUF_SIZE];
            loop {
                let n = remote.read(&mut buf).unwrap_or(0);
                trace!("{header} [REMOTE] read {} bytes from remote", n);

                if n == 0 {
                    trace!("{header} [REMOTE] DISCONNECT!");
                    break;
                }

                let _w = front_stream.write(&buf[..n]).unwrap();
                trace!("{header} [REMOTE] write {} bytes to client", _w);
            }
            local_tcp_handler.join().unwrap();
            info!("{header} transfer thread is finished!");
        });
    }

    backend_stream
}

#[derive(Debug)]
pub enum PortforwardRequest {
    NewLocalClientConnected {
        frontend_stream: TcpStream,
        src_port: u16,
    },
    UpdateRoutingRule {
        src_port: u16,
        dst_ip: Ipv4Addr,
        dst_port: u16,
    },
}

/*
 *
 * local client ----> pf_front server ---[port forward]---> remote server
 *                              backend: --> remote
 *                       accept -> connect
 *                         r/w <-> r/w
 */

pub fn spawn_pf_front_server(src_port: u16, req: Sender<PortforwardRequest>) {
    thread::spawn(move || {
        let listner = TcpListener::bind(format!("0.0.0.0:{}", src_port))
            .expect("failed to bind a port of {src_port}");

        for client in listner.incoming() {
            let local_client = client.unwrap().try_clone().unwrap();
            req.send(PortforwardRequest::NewLocalClientConnected {
                frontend_stream: local_client,
                src_port,
            })
            .expect("failed to send PortforwardRequest::NewClient");
        }
    });
}

pub fn start_port_forward_service(recv: Receiver<PortforwardRequest>) {
    thread::spawn(move || {
        let mut routing_table = HashMap::<u16, (Ipv4Addr, u16)>::new();
        let mut backend_streams = HashMap::<u16, Vec<TcpStream>>::new();

        loop {
            match recv.recv().expect("failed to unwrap PortforwardRequest") {
                PortforwardRequest::NewLocalClientConnected {
                    frontend_stream: client,
                    src_port,
                } => {
                    let (dst_ip, dst_port) = routing_table
                        .get(&src_port)
                        .expect("failed to lookup routing table");
                    let header = format!(
                        "[PORT FORWARDER (src: 0.0.0.0:{src_port} --> dst: {dst_ip}:{dst_port})]"
                    );

                    let backend_stream = spawn_backend_stream(header, client, *dst_ip, *dst_port);
                    if let Some(backend_streams) = backend_streams.get_mut(dst_port) {
                        backend_streams.push(backend_stream);
                    } else {
                        backend_streams.insert(*dst_port, vec![backend_stream]);
                    }
                }
                PortforwardRequest::UpdateRoutingRule {
                    src_port,
                    dst_ip,
                    dst_port,
                } => {
                    info!("[Port Forward Service] Update Routing Table @ localhost:{src_port} -> {dst_ip:?}:{dst_port}");
                    if let Some((old_dst_ip, old_dst_port)) =
                        routing_table.insert(src_port, (dst_ip, dst_port))
                    {
                        if old_dst_ip != dst_ip || old_dst_port != dst_port {
                            info!("[Port Forward Service] Routing Rule Changed! @ [src: {src_port}] [old dst: {old_dst_ip:?}:{old_dst_port}] [new dst: {dst_ip:?}:{dst_port}]");
                            if let Some(e_backend_streams) = backend_streams.get_mut(&src_port) {
                                info!("[Port Forward Service] Close all backend streams related with old routing rule");
                                for stream in e_backend_streams {
                                    stream
                                        .shutdown(std::net::Shutdown::Both)
                                        .expect("Failed to shutdown backend stream");
                                }
                                backend_streams.remove(&src_port);
                            }
                        }
                    }
                }
            }
        }
    });
}
