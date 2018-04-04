use byteorder::{ReadBytesExt, BigEndian, WriteBytesExt};
use std::io;
use std::io::prelude::*;

pub enum Input {
    Exit(i32),
    Signal(i32),
    StdIn(Vec<u8>),
    StdOut(Vec<u8>),
    StdErr(Vec<u8>)
}

/// Allocates a buffer of `num` bytes and reads that exact number
/// of bytes from `stream`
pub fn read_bytes(read: &mut Read, num: usize) -> io::Result<Vec<u8>> {
    let mut buf = vec![0; num];

    read
        .read_exact(&mut buf)
        .map(|_| buf)
}

/// core event loop that reads events from a provided reader and
/// handles the events accordingly
pub fn input_handler<R, W, SW, StdOut, StdErr>(pid: i32, mut reader: R, mut writer: W, mut single_session_writer: SW, mut std_out: StdOut, mut std_err: StdErr) -> io::Result<i32>
    where R: FnMut() -> io::Result<Input>, W: FnMut(Vec<u8>) -> io::Result<()>, StdOut: FnMut(Vec<u8>) -> io::Result<()>, SW: FnMut(Vec<u8>) -> io::Result<()>, StdErr: FnMut(Vec<u8>) -> io::Result<()> {
    loop {
        match reader() {
            Ok(Input::Exit(s)) => {
                return Ok(s);
            }

            Ok(Input::StdIn(b)) => {
                if let Err(e) = writer(b) {
                    return Err(e);
                }
            }

            Ok(Input::Signal(s)) => {
                let result =
                    encode_i32(pid)
                        .and_then(|pid_bytes| encode_i32(s).map(|sig_bytes| (pid_bytes, sig_bytes)))
                        .and_then(|(ref mut pid_bytes, ref mut sig_bytes)| {
                            let mut data = vec![];

                            data.push(b'k');
                            data.append(pid_bytes);
                            data.append(sig_bytes);

                            single_session_writer(data)
                        });

                if let Err(e) = result {
                    return Err(e);
                }
            }

            Ok(Input::StdOut(b)) => {
                if let Err(e) = std_out(b) {
                    return Err(e);
                }
            }

            Ok(Input::StdErr(b)) => {
                if let Err(e) = std_err(b) {
                    return Err(e);
                }
            }

            Err(e) => {
                return Err(e);
            }
        }
    }
}

/// manages reading the socket (landlord protocol)
pub fn read_handler<R, W>(mut reader: R, mut writer: W) -> io::Result<()>
    where R: FnMut(usize) -> io::Result<Vec<u8>>, W: FnMut(Input) -> io::Result<()> {

    loop {
        match reader(1).map(|bs| bs[0]) {
            Ok(101) => {
                // UTF8 'e'
                let result = read_payload(&mut reader)
                    .map(|p| Input::StdErr(p))
                    .and_then(|msg| writer(msg));

                if result.is_err() {
                    return result;
                }
            },
            Ok(111) => {
                // UTF8 'o'
                let result = read_payload(&mut reader)
                    .map(|p| Input::StdOut(p))
                    .and_then(|msg| writer(msg));

                if result.is_err() {
                    return result;
                }
            },
            Ok(120) => {
                // UTF8 'x'

                return reader(4)
                    .and_then(|bs| decode_i32(&bs))
                    .and_then(|code| writer(Input::Exit(code)));
            },
            Ok(other) => {
                return Err(
                    io::Error::new(
                        io::ErrorKind::InvalidInput, format!("Unknown code: {}", other)))
            },
            Err(err) => {
                return Err(err);
            }
        }
    }
}

/// reads the process id from the provided `stream`
pub fn read_pid_handler(stream: &mut Read) -> Option<i32> {
    read_bytes(stream, 4)
        .ok()
        .and_then(|bs| io::Cursor::new(bs).read_i32::<BigEndian>().ok())
}

/// given a reader, reads a landlord payload, i.e. a 4-byte encoded (big endian) size followed
/// by that number of bytes.
fn read_payload<R>(reader: &mut R) -> io::Result<Vec<u8>>
    where R: FnMut(usize) -> io::Result<Vec<u8>> {

    reader(4)
        .and_then(|bs| decode_i32(&bs))
        .and_then(|size| reader(size as usize))
}

fn decode_i32(vec: &Vec<u8>) -> io::Result<i32> {
    io::Cursor::new(vec).read_i32::<BigEndian>()
}

fn encode_i32(value: i32) -> io::Result<Vec<u8>> {
    let mut buf = vec![];

    buf
        .write_i32::<BigEndian>(value)
        .map(|_| buf)
}
