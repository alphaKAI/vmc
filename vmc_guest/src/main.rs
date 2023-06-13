#![feature(io_error_more)]
use serde::{Deserialize, Serialize};
use std::io::{self, Read};
use std::path::Path;
use std::{env, process::Command, str};
use strum::{EnumIter, IntoEnumIterator};
use vmc_common::{
    AutoReConnectTcpStream, CBRequest, CBResponse, ExecRequest, Request, Response,
    SerializedDataContainer, SERVER_HOST, SERVER_PORT,
};

const MOUNT_LIST_FILE: &str = ".mount_list.json";

#[derive(Debug, Serialize, Deserialize)]
struct MountEntry {
    #[serde(rename = "end-point")]
    pub end_point: String,
    #[serde(rename = "mount-point")]
    pub mount_point: String,
    #[serde(rename = "remote-path")]
    pub remote_path: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct MountList {
    #[serde(rename = "mount-list")]
    pub mount_list: Vec<MountEntry>,
}

fn expand_tilde(s: &str) -> String {
    if s.starts_with('~') {
        s.replace('~', &env::var("HOME").unwrap())
    } else {
        s.to_owned()
    }
}

impl MountList {
    pub fn is_subdir(&self, dir: &Path) -> Option<&MountEntry> {
        for entry in self.mount_list.iter() {
            let abs_mount_point = std::fs::canonicalize(&entry.mount_point).unwrap();
            let dir = std::fs::canonicalize(dir).unwrap();

            if dir.starts_with(abs_mount_point) {
                return Some(entry);
            }
        }

        None
    }

    pub fn try_convert_to_remote_path(&self, path: &str) -> Option<String> {
        if let Ok(path) = std::fs::canonicalize(path) {
            if let Some(entry) = self.is_subdir(&path) {
                let path = cutoff_prefix(path.to_str()?, &entry.mount_point);
                // NOTE: target is win
                let path = path.replace('/', "\\");
                let remote_path = format!("{}\\{path}", entry.remote_path);

                return Some(remote_path);
            }
        }

        None
    }
}

fn load_mount_list() -> Option<MountList> {
    let home_dir = env::var("HOME").ok()?;
    let file_path = format!("{home_dir}/{MOUNT_LIST_FILE}");
    let path = Path::new(&file_path);

    if path.exists() {
        let content = String::from_utf8(std::fs::read(path).ok()?).ok()?;
        return serde_json::from_str::<MountList>(&content)
            .ok()
            .map(|mut mount_list| {
                for e in mount_list.mount_list.iter_mut() {
                    e.mount_point = expand_tilde(&e.mount_point)
                }

                mount_list
            });
    }

    None
}

fn cutoff_prefix(pat: &str, prefix: &str) -> String {
    if let Some(strip) = pat.strip_prefix(prefix) {
        strip.to_string()
    } else {
        pat.to_string()
    }
}

#[allow(dead_code)]
fn is_sub_of_cifs_dir(dir: &str) -> bool {
    let output = Command::new("sh")
        .arg("-c")
        .arg("mount")
        .output()
        .expect("failed to execute mount")
        .stdout;

    let outputs: Vec<_> = str::from_utf8(&output).unwrap().split('\n').collect();

    for output in outputs {
        let es: Vec<_> = output.split(' ').collect();
        if es.len() == 1 {
            break;
        }

        let mount_point = es[2];
        let mount_type = es[4];

        if mount_type == "cifs" && dir.starts_with(mount_point) {
            return true;
        }
    }

    false
}

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();

    let mut sock = AutoReConnectTcpStream::new(
        format!("{SERVER_HOST}:{SERVER_PORT}"),
        std::time::Duration::from_secs(5),
    );

    #[derive(PartialEq, Debug, EnumIter)]
    enum Mode {
        ClipBoardSet,
        ClipBoardGet,
        Execute,
        Open,
        Help,
        ToWinPath,
    }

    let mode = if args.len() < 2 {
        eprintln!("Argument Required");

        Mode::Help
    } else {
        match args[1].as_str() {
            "cb-set" => Mode::ClipBoardSet,
            "cb-get" => Mode::ClipBoardGet,
            "exec" => {
                if args.len() < 3 {
                    panic!("Too few args for {}", args[1]);
                }
                Mode::Execute
            }
            "open" => {
                if args.len() != 3 {
                    panic!("{} command requires only one arg", args[1]);
                }
                Mode::Open
            }
            "help" => Mode::Help,
            "to-win-path" => {
                if args.len() != 3 {
                    panic!("{} command requires only one arg", args[1]);
                }
                Mode::ToWinPath
            }
            _ => {
                eprintln!("Unkown command was given: {}", args[1]);

                Mode::Help
            }
        }
    };

    let mount_list = load_mount_list();

    let recv_required = match mode {
        // TODO: Support binary format
        Mode::ClipBoardSet => {
            let mut buf = String::new();

            io::stdin().lock().read_to_string(&mut buf).unwrap();

            sock.write_all(
                &SerializedDataContainer::from_serializable_data(&Request::ClipBoard(
                    CBRequest::SetClipboard(buf),
                ))
                .unwrap()
                .to_one_vec(),
            )
            .unwrap();

            false
        }
        Mode::ClipBoardGet => {
            sock.write_all(
                &SerializedDataContainer::from_serializable_data(&Request::ClipBoard(
                    CBRequest::GetClipboard,
                ))
                .unwrap()
                .to_one_vec(),
            )
            .unwrap();

            true
        }
        // TODO: Share stdio like SSH
        Mode::Execute => {
            let mut cmd_args = args[2..].to_vec();

            if let Some(mount_list) = mount_list {
                for e in cmd_args.iter_mut() {
                    if let Some(p) = mount_list.try_convert_to_remote_path(e) {
                        *e = p;
                    }
                }
            }

            sock.write_all(
                &SerializedDataContainer::from_serializable_data(&Request::Execute(
                    ExecRequest::Execute(cmd_args),
                ))
                .unwrap()
                .to_one_vec(),
            )
            .unwrap();

            false
        }
        Mode::Open => {
            let arg = args[2].clone();
            let path =
                mount_list.and_then(|mount_list| mount_list.try_convert_to_remote_path(&arg));

            if let Some(path) = path {
                sock.write_all(
                    &SerializedDataContainer::from_serializable_data(&Request::Execute(
                        ExecRequest::Open(path),
                    ))
                    .unwrap()
                    .to_one_vec(),
                )
                .unwrap();
            } else {
                panic!("[Error] your specified path is not located on subdir of mount point.");
            }

            false
        }
        Mode::ToWinPath => {
            let p = Path::new(&args[2]);

            if let Some(mount_list) = mount_list {
                if let Some(p) = mount_list.try_convert_to_remote_path(p.to_str().unwrap()) {
                    println!("{p}");
                    return Ok(());
                }
            }

            println!("GIVEN_PATH_IS_NOT_SUBDIR_OF_MOUNT_POINT");

            false
        }
        Mode::Help => {
            println!("provided sub-commands:");
            for mode in Mode::iter() {
                println!(" - {mode:?}");
            }
            return Ok(());
        }
    };

    if !recv_required {
        return Ok(());
    }

    if let Ok(sdc) = SerializedDataContainer::from_reader(&mut sock.stream) {
        match sdc.to_serializable_data::<Response>().unwrap() {
            Response::ClipBoard(cb_res) => match cb_res {
                CBResponse::GetClipboard(s) => {
                    println!("{s}");
                }
            },
            _ => todo!(),
        }
    } else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NetworkUnreachable,
            "Failed to recv response from server",
        ));
    }

    Ok(())
}
