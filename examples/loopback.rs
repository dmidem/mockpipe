use std::io::{Read, Write};

use mockpipe::MockPipe;

fn main() {
    let mut pipe = MockPipe::loopback(1024);

    let write_data = b"hello";
    pipe.write_all(write_data).unwrap();

    let mut read_data = [0u8; 5];
    pipe.read_exact(&mut read_data).unwrap();

    assert_eq!(&read_data, write_data);
}
