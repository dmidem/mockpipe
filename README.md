# MockPipe

An in-memory, thread-safe, bidirectional pipe for Rust applications. It provides functionality for reading and writing data with optional timeout support. **MockPipe** utilizes an internal in-memory circular buffer without relying on operating system resources and implements Rust's standard `Read` and `Write` traits. This makes it a useful tool for testing applications that use communication mechanisms like sockets, pipes, or serial ports.

## Example Usage

### Loopback pipe

A loopback pipe allows data written to the pipe to be readable from the same pipe:

```rust
use std::io::{Read, Write};

use mockpipe::MockPipe;

let mut pipe = MockPipe::loopback(1024);

let write_data = b"hello";
pipe.write_all(write_data).unwrap();

let mut read_data = [0u8; 5];
pipe.read_exact(&mut read_data).unwrap();

assert_eq!(&read_data, write_data);
```

### Pair of pipes

Two interconnected pipe instances that can exchange data in a full-duplex manner:

```rust
use std::io::{Read, Write};

use mockpipe::MockPipe;

let (mut pipe1, mut pipe2) = MockPipe::pair(1024);;

let write_data = b"hello";
pipe1.write_all(write_data).unwrap();

let mut read_data = [0u8; 5];
pipe2.read_exact(&mut read_data).unwrap();

assert_eq!(&read_data, write_data);
```

More examples can be found in the `examples` folder in the root of this repository.

## Features

- **Loopback mode:** Create a pipe that writes data into a buffer and allows reading the same data back from the same buffer, simulating a loopback interface.
- **Paired pipes:** Create two pipe instances that can exchange data in a full-duplex manner, simulating a communication channel between two endpoints.
- **Timeout support:** Specify a timeout for reading and writing operations to test different behaviors in blocking and non-blocking scenarios.
- **No OS resource usage & No unsafe code:** Operates without relying on operating system resources and is implemented entirely with safe Rust, without any `unsafe` blocks.
- **In-memory operation:** Does not consume OS-level resources, ideal for unit testing.
- **Standard IO trait support:** Implements `std::io::Read` and `std::io::Write` traits for seamless integration with Rust's I/O ecosystem.

## License

Licensed under either of Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE)) or MIT license ([LICENSE-MIT](LICENSE-MIT)) at your option.
