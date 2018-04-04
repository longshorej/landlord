use chan_signal::{notify, Signal};
use libc;
use proto::*;
use std::{fs, io, path, thread};
use std::io::prelude::*;
use std::os::unix::net::UnixStream;
use std::sync::mpsc::*;
use tar::Builder;


/// Binds everything together and ensures that events received from a given `reader` will
/// be handled accordingly.
pub fn handle_events<NewS>(pid: i32, mut stream: UnixStream, reader: Receiver<Input>, mut new_stream: NewS) -> io::Result<i32>
    where NewS: FnMut() -> io::Result<UnixStream> {

    let mut stdout = io::stdout();
    let mut stderr = io::stderr();

    let handler_reader = || { reader.recv().map_err(|e| io::Error::new(io::ErrorKind::BrokenPipe, e)) };
    let handler_writer = |bs: Vec<u8>| { stream.write_all(&bs) };
    let session_writer = |bs: Vec<u8>| { new_stream().and_then(|ref mut s| { s.write_all(&bs) }) };
    let std_out = |bs: Vec<u8>| { stdout.write_all(&bs) };
    let std_err = |bs: Vec<u8>| { stderr.write_all(&bs) };

    input_handler(pid, handler_reader, handler_writer, session_writer, std_out, std_err)
}

/// Signals to the OS which signals we are interested in, and then
/// spawns a thread to wait for them and forward them to the
/// provided `sender`.
pub fn spawn_and_handle_signals(sender: Sender<Input>) {
    let all_signals = [
        Signal::ABRT, Signal::ALRM, Signal::BUS, Signal::CHLD, Signal::CONT,
        Signal::FPE, Signal::HUP, Signal::ILL, Signal::INT, Signal::IO,
        Signal::KILL, Signal::PIPE, Signal::PROF, Signal::QUIT, Signal::SEGV,
        Signal::STOP, Signal::SYS, Signal::TERM, Signal::TRAP, Signal::TSTP,
        Signal::TTIN, Signal::TTOU, Signal::URG, Signal::USR1, Signal::USR2,
        Signal::VTALRM, Signal::WINCH, Signal::XCPU, Signal::XFSZ,
    ];

    // chan_signal doesn't have a public function for coverting a signal to its integer code
    // so we have to do that ourselves..

    let as_sig = |s: &Signal| {
        match *s {
            Signal::HUP => libc::SIGHUP, Signal::INT => libc::SIGINT,
            Signal::QUIT => libc::SIGQUIT, Signal::ILL => libc::SIGILL,
            Signal::ABRT => libc::SIGABRT, Signal::FPE => libc::SIGFPE,
            Signal::KILL => libc::SIGKILL, Signal::SEGV => libc::SIGSEGV,
            Signal::PIPE => libc::SIGPIPE, Signal::ALRM => libc::SIGALRM,
            Signal::TERM => libc::SIGTERM, Signal::USR1 => libc::SIGUSR1,
            Signal::USR2 => libc::SIGUSR2, Signal::CHLD => libc::SIGCHLD,
            Signal::CONT => libc::SIGCONT, Signal::STOP => libc::SIGSTOP,
            Signal::TSTP => libc::SIGTSTP, Signal::TTIN => libc::SIGTTIN,
            Signal::TTOU => libc::SIGTTOU, Signal::BUS => libc::SIGBUS,
            Signal::PROF => libc::SIGPROF, Signal::SYS => libc::SIGSYS,
            Signal::TRAP => libc::SIGTRAP, Signal::URG => libc::SIGURG,
            Signal::VTALRM => libc::SIGVTALRM, Signal::XCPU => libc::SIGXCPU,
            Signal::XFSZ => libc::SIGXFSZ, Signal::IO => libc::SIGIO,
            Signal::WINCH => libc::SIGWINCH, _ => 1
        }
    };

    let signal = notify(&all_signals);

    thread::spawn(move || {
        loop {
            if let Some(s) = signal.recv() {
                if let Err(e) = sender.send(Input::Signal(as_sig(&s))) {
                    eprintln!("landlord: signal handler crashed, {:?}", e);
                    return;
                }
            }
        }
    });
}

/// Spawns a thread and consumes stdin, forwarding a copy of
/// the consumed data to provided `sender`
pub fn spawn_and_handle_stdin(sender: Sender<Input>) {
    thread::spawn(move || {
        let stdin = io::stdin();
        let mut stdin_lock = stdin.lock();
        let mut buffer = vec![0; 1024];

        loop {
            let result =
                stdin_lock
                    .read(&mut buffer)
                    .and_then(|num| {
                        buffer.truncate(num);

                        sender
                            .send(Input::StdIn(buffer.clone()))
                            .map_err(|e| io::Error::new(io::ErrorKind::BrokenPipe, format!("{:?}", e)))
                            .map(|_| num)
                    });

            match result {
                Ok(num) if num == 0 => {
                    // @TODO how do we signify to landlord that this is EOF? we might need
                    //       a protocol. Note that we can't closet the socket as we still want
                    //       to receive stdout/stderr/exit code data
                    //       one way to hit this case is to CTRL+D in your tty
                    return
                },
                Ok(_) => (),
                Err(ref err) if err.kind() == io::ErrorKind::Interrupted => (),
                Err(e) => {
                    eprintln!("landlord: stdin crashed, {:?}", e);
                    return;
                }
            }
        }
    });
}

/// Spawns a thread and reads data from the provided `stream`. The actual logic
/// of how much to read is done via the
pub fn spawn_and_handle_stream_read(mut stream: UnixStream, sender: Sender<Input>) {
    thread::spawn(move || {
        let s = &mut stream;
        let r = |n: usize| read_bytes(s, n);
        let m = |msg: Input| sender
            .send(msg)
            .map_err(|e| io::Error::new(io::ErrorKind::BrokenPipe, e));

        read_handler(r, m)
    });
}

/// Writes the provided `class_path` to the provided `stream` and starts the process. Returns
/// the process id (from landlordd's perpsective). Upon successful completion, the process
/// is running and any data subsequently written to `stream` is stdin.
pub fn install_fs_and_start(class_path: &Vec<String>, class: &String, args: &Vec<String>, stream: &mut UnixStream) -> io::Result<i32> {
    // given a list of class path entries, these are written to the tar via their position in
    // the vector. Meaning the first entry will be named "0", second "1", and so on. This
    // allows the user to specify any combination of directories and files without us having
    // to find some common parent path string.

    let cp_range: Vec<usize> = (0..(class_path.len())).collect();
    let cp_args: Vec<String> = cp_range.iter().map(|i| format!("{}", i)).collect();
    let descriptor = format!(
        "l-cp\u{0000}{}\u{0000}{}{}\n",
        cp_args.join(":"),
        class,
        if args.is_empty() {
            format!("")
        } else {
            format!("\u{0000}{}", args.join("\u{0000}"))
        }
    );

    stream
        .write_all(descriptor.as_bytes())
        .and_then(|_| {
            let tar_padding_stream = BlockSizeWriter::new(stream, 10240);

            let mut tar_builder = Builder::new(tar_padding_stream);

            class_path
                .iter()
                .enumerate()
                .fold(Ok(()), |accum, (i, path)| {
                    accum.and_then(|_| {
                        fs::canonicalize(path).and_then(|path| {
                            let path_struct = path::Path::new(&path);
                            let name = format!("{}", i);

                            if path_struct.is_file() {
                                fs::File::open(path_struct)
                                    .and_then(|ref mut f| tar_builder.append_file(name, f))
                            } else if path_struct.is_dir() {
                                tar_builder.append_dir_all(name, path.clone())
                            } else {
                                Ok(())
                            }
                        })
                    })
                })
                .and_then(|_| {
                    tar_builder
                        .finish()
                        .and(tar_builder.into_inner())
                        .and_then(|ref mut stream| stream.finish())
                        .and_then(|ref mut stream|
                            match stream {
                                &mut None =>
                                    Err(io::Error::new(io::ErrorKind::InvalidInput, "Unable to acquire stream (finish() called before?)")),

                                &mut Some(ref mut stream) =>
                                    read_pid_handler(stream)
                                        .ok_or(io::Error::new(io::ErrorKind::InvalidInput, "Unable to parse pid"))

                            }
                        )
                })
        })
}

/// Tracks num bytes written to the provided `stream` and ensures
/// that zero-padded blocks are written. Implemented because
/// landlordd requires block size of 20, but the tar lib
/// doesn't support that
struct BlockSizeWriter<W: Write> {
    stream: Option<W>,
    written: usize,
    block_size: usize
}

impl<W: Write> Write for BlockSizeWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self.stream {
            None => Err(io::Error::new(io::ErrorKind::Other, "stream closed")),
            Some(ref mut s) => {
                match s.write(buf) {
                    Ok(size) => {
                        self.written += size;

                        Ok(size)
                    }

                    Err(err) => Err(err)
                }
            }
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self.stream {
            None => Err(io::Error::new(io::ErrorKind::Other, "stream closed")),
            Some(ref mut s) => s.flush()
        }
    }
}

impl<W: Write> BlockSizeWriter<W> {
    pub fn new(obj: W, block_size: usize) -> BlockSizeWriter<W> {
        BlockSizeWriter {
            stream: Some(obj),
            written: 0,
            block_size
        }
    }

    pub fn finish(&mut self) -> io::Result<Option<W>> {
        let operation =
            if let Some(ref mut stream) = self.stream {
                let bytes_left = self.block_size - (self.written % self.block_size);
                let bytes = vec![0; bytes_left];

                stream
                    .write_all(&bytes)
                    .and(stream.flush())
            } else {
                Ok(())
            };

        operation.map(|_| self.stream.take())
    }
}