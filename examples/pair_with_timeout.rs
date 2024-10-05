use std::{
    io::{ErrorKind, Read, Write},
    time::Duration,
};

use mockpipe::MockPipe;

fn main() {
    let (mut pipe1, mut pipe2) = MockPipe::pair(1024);

    pipe2.set_timeout(Some(Duration::from_millis(100)));

    let write_data = b"hello";
    pipe1.write_all(write_data).unwrap();

    let mut read_data = [0u8; 5];
    pipe2.read_exact(&mut read_data).unwrap();

    assert_eq!(&read_data, write_data);

    assert_eq!(
        pipe2.read_exact(&mut read_data).unwrap_err().kind(),
        ErrorKind::TimedOut
    );
}
