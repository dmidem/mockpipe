use std::{
    io::{Read, Write},
    thread,
    time::Duration,
};

use mockpipe::MockPipe;

fn main() {
    let (mut pipe1, mut pipe2) = MockPipe::pair(1024);

    let write_data = b"hello";

    let writer = thread::spawn(move || {
        pipe1.write_all(write_data).unwrap();
    });

    let reader = thread::spawn(move || {
        thread::sleep(Duration::from_millis(100));

        let mut read_data = [0u8; 5];
        pipe2.read_exact(&mut read_data).unwrap();

        assert_eq!(&read_data, write_data);
    });

    writer.join().unwrap();
    reader.join().unwrap();
}
