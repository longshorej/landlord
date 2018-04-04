extern crate landlord;

use landlord::args::*;
use landlord::bindings::*;
use std::{env, process, str};
use std::os::unix::net::UnixStream;
use std::sync::mpsc::*;

const VERSION: &'static str = env!("CARGO_PKG_VERSION");

const USAGE: &'static str = "Usage: landlord [-options] class [args...]
           (to execute a class)
   or  landlord [-options] -jar jarfile [args...]
           (to execute a jar file)
where options include:
    -cp <class search path of directories and zip/jar files> -classpath <class search path of directories and zip/jar files>
                  A : separated list of directories, JAR archives,
                  and ZIP archives to search for class files.
    -D<name>=<value>
                  set a system property
    -version      print product version and exit
    -showversion  print product version and continue
    -? -help      print this help message
    -socket       path to landlord UNIX domain socket";

fn main() {
    let args: Vec<String> = env::args().collect();
    let parsed = parse_java_args(&args[1..].to_vec());
    let errors = validate_java_args(&parsed);
    let (tx, rx) = channel();

    if parsed.version {
        eprintln!("landlord version \"{}\"", VERSION);
    }

    if errors.is_empty() {
        match parsed.mode {
            ExecutionMode::Class { ref class, ref args } => {
                let socket_path = parsed.socket.to_string();

                match &mut UnixStream::connect(socket_path.to_string()) {
                    &mut Err(ref mut e) => {
                        eprintln!("landlord: failed to connect to socket: {:?}", e);

                        process::exit(1);
                    },

                    &mut Ok(ref mut stream) => {
                        let result =
                            install_fs_and_start(&parsed.cp, class, args, stream)
                                .and_then(|pid| stream.try_clone().map(|s1| (pid, s1)))
                                .and_then(|(pid, s1)| stream.try_clone().map(|s2| (pid, s1, s2)))
                                .and_then(|(pid, s1, s2)| {
                                    spawn_and_handle_signals(tx.clone());
                                    spawn_and_handle_stdin(tx.clone());
                                    spawn_and_handle_stream_read(s1, tx.clone());

                                    handle_events(pid, s2, rx, || UnixStream::connect(socket_path.to_string()))
                                });

                        let code = match result {
                            Ok(c)  => c,

                            Err(e) => {
                                eprintln!("landlord: {:?}", e);
                                1
                            }
                        };

                        process::exit(code);
                    }
                };
            }

            ExecutionMode::Exit { code } => {
                process::exit(code);
            }

            ExecutionMode::Help { code } => {
                eprintln!("{}", USAGE);

                process::exit(code);
            }

            ExecutionMode::JarFile { file: _file, args: _args } => {
                eprintln!("landlord: `-jar` currently unsupported");

                process::exit(1);
            }
        }
    } else {
        errors.iter().for_each(|e| println!("landlord: {}", e));

        process::exit(1);
    }
}


