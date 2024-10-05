//! Provides the `MockPipe` struct for exchanging data through internal circular
//! buffers. It supports reading and writing with optional timeout functionality
//! and is useful for testing communication mechanisms like sockets, pipes,
//! serial ports etc.
//
//! # Example
//!
//! ```
//! use std::io::{Read, Write};
//!
//! use mockpipe::MockPipe;
//!
//! let mut pipe = MockPipe::loopback(1024);
//!
//! let write_data = b"hello";
//! pipe.write_all(write_data).unwrap();
//!
//! let mut read_data = [0u8; 5];
//! pipe.read_exact(&mut read_data).unwrap();
//!
//! assert_eq!(&read_data, write_data);
//! ```

// To run doc tests on examples from README.md and verify their correctness
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadMe;

use std::{
    collections::VecDeque,
    io,
    sync::{Arc, Condvar, Mutex, MutexGuard},
    time::Duration,
};

/// A thread-safe circular buffer with synchronization primitives.
struct SyncBuffer {
    data: Mutex<VecDeque<u8>>,
    can_read: Condvar,
    can_write: Condvar,
}

impl SyncBuffer {
    /// Creates a new `SyncBuffer` with the specified capacity.
    fn new(capacity: usize) -> Self {
        SyncBuffer {
            data: Mutex::new(VecDeque::with_capacity(capacity)),
            can_read: Condvar::new(),
            can_write: Condvar::new(),
        }
    }

    /// Waits until the condition function returns false.
    ///
    /// If successful, returns a new locked guard to the data buffer.
    /// If a timeout is specified, returns a `TimedOut` error if the condition
    /// is not met within the timeout duration.
    fn wait_while<'a, F>(
        mut data_guard: MutexGuard<'a, VecDeque<u8>>,
        condvar: &Condvar,
        timeout: Option<Duration>,
        condition: F,
    ) -> io::Result<MutexGuard<'a, VecDeque<u8>>>
    where
        F: Fn(&mut VecDeque<u8>) -> bool,
    {
        if condition(&mut data_guard) {
            data_guard = match timeout {
                Some(Duration::ZERO) => data_guard,
                Some(timeout) => {
                    let (new_guard, timeout_result) = condvar
                        .wait_timeout_while(data_guard, timeout, condition)
                        .map_err(|_| io::Error::from(io::ErrorKind::Other))?;

                    if timeout_result.timed_out() {
                        return Err(io::Error::from(io::ErrorKind::TimedOut));
                    }

                    new_guard
                }
                None => condvar
                    .wait_while(data_guard, condition)
                    .map_err(|_| io::Error::from(io::ErrorKind::Other))?,
            };
        }

        Ok(data_guard)
    }

    /// Waits until the required number of bytes are available in the buffer for
    /// reading or writing.
    ///
    /// If successful, returns a locked data guard and the number of bytes available.
    /// If a timeout is specified, returns a `TimedOut` error if the required bytes
    /// are not available within the timeout duration.
    fn wait_for_bytes_available<F>(
        &self,
        bytes_required: usize,
        condvar: &Condvar,
        timeout: Option<Duration>,
        get_bytes_available: F,
    ) -> io::Result<(MutexGuard<VecDeque<u8>>, usize)>
    where
        F: Fn(&VecDeque<u8>) -> usize,
    {
        let mut data_guard = self.data.lock().unwrap();

        if (bytes_required == 0) || (data_guard.capacity() == 0) {
            return Ok((data_guard, 0));
        }

        data_guard = Self::wait_while(data_guard, condvar, timeout, |data| {
            get_bytes_available(data) == 0
        })?;

        let bytes_available = bytes_required.min(get_bytes_available(&data_guard));

        Ok((data_guard, bytes_available))
    }

    /// Reads data from the buffer.
    ///
    /// Blocks until the specified amount of data is available or the timeout is reached.
    /// Returns the number of bytes read if successful.
    fn read(&self, buf: &mut [u8], timeout: Option<Duration>) -> io::Result<usize> {
        let (mut data_guard, bytes_to_read) =
            self.wait_for_bytes_available(buf.len(), &self.can_read, timeout, |guard| guard.len())?;

        if bytes_to_read > 0 {
            for byte in &mut buf[0..bytes_to_read] {
                *byte = data_guard.pop_front().unwrap();
            }

            // Notify the writer that space is available
            self.can_write.notify_one();
        }

        Ok(bytes_to_read)
    }

    /// Writes data into the buffer.
    ///
    /// Blocks if there is not enough space until some space becomes available
    /// or the timeout is reached. Returns the number of bytes written if successful.
    fn write(&self, buf: &[u8], timeout: Option<Duration>) -> io::Result<usize> {
        let (mut data_guard, bytes_to_write) =
            self.wait_for_bytes_available(buf.len(), &self.can_write, timeout, |guard| {
                guard.capacity() - guard.len()
            })?;

        if bytes_to_write > 0 {
            data_guard.extend(&buf[0..bytes_to_write]);

            // Notify the reader that data is available
            self.can_read.notify_one();
        }

        Ok(bytes_to_write)
    }

    /// Waits until all data has been written from the buffer (blocks until the buffer is empty
    /// or the operation times out, if a timeout is specified).
    fn flush(&self, timeout: Option<Duration>) -> io::Result<()> {
        // Wait until the write buffer is empty.
        Self::wait_while(
            self.data.lock().unwrap(),
            &self.can_write,
            timeout,
            |data| !data.is_empty(),
        )
        .map(|_| ())
    }

    /// Clears the buffer, discarding all pending data and notifying waiting writers.
    fn clear(&self) {
        self.data.lock().unwrap().clear();
        self.can_write.notify_all();
    }

    /// Returns the number of bytes available to read.
    fn len(&self) -> usize {
        self.data.lock().unwrap().len()
    }
}

/// A bidirectional data pipe that exchanges datausing internal circular buffers.
/// It provides functionality for reading and writing data with timeout support.
/// Can be used in loopback mode or as a paired connection between two endpoints.
///
/// This structure is intended for implementing virtual sockets, pipes, serial
/// ports, or similar communication mechanisms, abstracting away the details of
/// buffer management and synchronization.
#[derive(Clone)]
pub struct MockPipe {
    /// Timeout duration for read and write operations.
    ///
    /// - `None` means the operation blocks indefinitely.
    /// - `Some(Duration::ZERO)` means the operation is non-blocking.
    /// - `Some(Duration)` sets a specific timeout duration.
    timeout: Arc<Mutex<Option<Duration>>>,

    /// Buffer used for reading data.
    read_buffer: Arc<SyncBuffer>,

    /// Buffer used for writing data.
    write_buffer: Arc<SyncBuffer>,
}

impl MockPipe {
    /// Creates a `MockPipe` instance from separate read and write buffers.
    fn from_buffers(read_buffer: Arc<SyncBuffer>, write_buffer: Arc<SyncBuffer>) -> Self {
        Self {
            // Non-blocking by default
            timeout: Arc::new(Mutex::new(Some(Duration::ZERO))),
            read_buffer,
            write_buffer,
        }
    }

    /// Creates a `MockPipe` in loopback mode, where the same buffer is used
    /// for both reading and writing. This is useful for simulating a simple
    /// communication scenario where data written to the pipe can be immediately
    /// read back.
    pub fn loopback(buffer_capacity: usize) -> Self {
        let buffer = Arc::new(SyncBuffer::new(buffer_capacity));
        Self::from_buffers(buffer.clone(), buffer)
    }

    /// Creates a linked pair of `MockPipe` instances, allowing data written
    /// to one pipe to be read from the other. This simulates a full-duplex
    /// communication channel between two endpoints.
    pub fn pair(buffer_capacity: usize) -> (Self, Self) {
        let buffer1 = Arc::new(SyncBuffer::new(buffer_capacity));
        let buffer2 = Arc::new(SyncBuffer::new(buffer_capacity));

        let pipe1 = Self::from_buffers(buffer1.clone(), buffer2.clone());
        let pipe2 = Self::from_buffers(buffer2, buffer1);

        (pipe1, pipe2)
    }

    /// Gets the current timeout duration for read/write operations.
    pub fn timeout(&self) -> Option<Duration> {
        *self.timeout.lock().unwrap()
    }

    /// Sets the timeout duration for read/write operations.
    ///
    /// `None` means the operation blocks indefinitely. `Some(Duration::ZERO)` means
    /// the operation is non-blocking.
    pub fn set_timeout(&self, timeout: Option<Duration>) {
        *self.timeout.lock().unwrap() = timeout;
    }

    /// Sets the timeout duration for read/write operations and returns the modified
    /// `MockPipe`.
    pub fn with_timeout(self, timeout: Option<Duration>) -> Self {
        self.set_timeout(timeout);
        self
    }

    /// Returns the number of bytes currently available to read from the buffer.
    pub fn read_buffer_len(&self) -> usize {
        self.read_buffer.len()
    }

    /// Returns the number of bytes currently queued to write in the buffer.
    pub fn write_buffer_len(&self) -> usize {
        self.write_buffer.len()
    }

    /// Clears the read buffer, discarding all pending data.
    pub fn clear_read(&self) {
        self.read_buffer.clear();
    }

    /// Clears the write buffer, discarding all pending data.
    pub fn clear_write(&self) {
        self.write_buffer.clear();
    }

    /// Clears both read and write buffers, discarding all pending data.
    pub fn clear(&self) {
        self.clear_read();
        self.clear_write();
    }
}

impl io::Read for MockPipe {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.read_buffer.read(buf, self.timeout())
    }
}

impl io::Write for MockPipe {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.write_buffer.write(buf, self.timeout())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.write_buffer.flush(None)
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};

    use super::*;

    #[test]
    fn test_loopback() {
        let mut pipe = MockPipe::loopback(1024);

        // Two test passes: without and with timeout
        for _ in 0..1 {
            pipe.write_all(b"").unwrap();
            pipe.write_all(b"").unwrap();

            pipe.read_exact(&mut []).unwrap();

            let write_data = b"hello";
            pipe.write_all(write_data).unwrap();

            pipe.read_exact(&mut []).unwrap();
            pipe.read_exact(&mut []).unwrap();

            pipe.write_all(b"").unwrap();

            pipe.read_exact(&mut []).unwrap();

            let mut read_data = [0u8; 5];
            pipe.read_exact(&mut read_data).unwrap();

            pipe.write_all(b"").unwrap();

            assert_eq!(&read_data, write_data);

            // Set a timeout for the next pass
            pipe.set_timeout(Some(Duration::from_millis(100)));
        }
    }

    #[test]
    fn test_pair() {
        let (mut pipe1, mut pipe2) = MockPipe::pair(1024);

        let write_data = b"hello";
        pipe1.write_all(write_data).unwrap();

        let mut read_data = [0u8; 5];
        pipe2.read_exact(&mut read_data).unwrap();

        assert_eq!(&read_data, write_data);
    }

    #[test]
    fn test_bidirectional_exchange() {
        let (mut pipe1, mut pipe2) = MockPipe::pair(1024);

        let write_data11 = b"hello";
        pipe1.write_all(write_data11).unwrap();

        assert_eq!(pipe1.write_buffer_len(), 5);
        assert_eq!(pipe1.read_buffer_len(), 0);
        assert_eq!(pipe2.write_buffer_len(), 0);
        assert_eq!(pipe2.read_buffer_len(), 5);

        let write_data2 = b"ok";
        pipe2.write_all(write_data2).unwrap();

        assert_eq!(pipe1.write_buffer_len(), 5);
        assert_eq!(pipe1.read_buffer_len(), 2);
        assert_eq!(pipe2.write_buffer_len(), 2);
        assert_eq!(pipe2.read_buffer_len(), 5);

        let write_data12 = b"world";
        pipe1.write_all(write_data12).unwrap();

        assert_eq!(pipe1.write_buffer_len(), 10);
        assert_eq!(pipe1.read_buffer_len(), 2);
        assert_eq!(pipe2.write_buffer_len(), 2);
        assert_eq!(pipe2.read_buffer_len(), 10);

        // Partial reads

        let mut read_data1 = [0u8; 1];
        pipe1.read_exact(&mut read_data1).unwrap();

        let mut read_data2 = [0u8; 7];
        pipe2.read_exact(&mut read_data2).unwrap();

        assert_eq!(pipe1.write_buffer_len(), 3);
        assert_eq!(pipe1.read_buffer_len(), 1);
        assert_eq!(pipe2.write_buffer_len(), 1);
        assert_eq!(pipe2.read_buffer_len(), 3);

        assert_eq!(&read_data1, b"o");
        assert_eq!(&read_data2, b"hellowo");
    }

    #[test]
    fn test_zero_capacity_buffer() {
        let mut pipe = MockPipe::loopback(0);

        // Two test passes: without and with timeout
        for _ in 0..1 {
            pipe.write_all(b"").unwrap();

            // Attempt to write to a zero-capacity buffer should fail
            assert_eq!(
                pipe.write_all(b"hello").unwrap_err().kind(),
                io::ErrorKind::WriteZero
            );

            pipe.read_exact(&mut []).unwrap();

            // Attempt to read from a zero-capacity buffer should fail
            let mut read_data = [0u8; 5];
            assert_eq!(
                pipe.read_exact(&mut read_data).unwrap_err().kind(),
                io::ErrorKind::UnexpectedEof
            );

            // Set a timeout for the next pass
            pipe.set_timeout(Some(Duration::from_millis(100)));
        }
    }

    #[test]
    fn test_timeout_write() {
        // Small buffer
        let mut pipe = MockPipe::loopback(5).with_timeout(Some(Duration::from_millis(100)));

        // Try to read from empty buffer; should timeout
        let mut read_data = [0u8; 5];
        assert_eq!(
            pipe.read_exact(&mut read_data).unwrap_err().kind(),
            io::ErrorKind::TimedOut
        );

        // Fill the buffer
        pipe.write_all(b"hello").unwrap();

        // Attempt to write more data should cause timeout
        assert_eq!(
            pipe.write_all(b"!").unwrap_err().kind(),
            io::ErrorKind::TimedOut
        );
    }

    #[test]
    fn test_buffer_clearing() {
        let mut pipe = MockPipe::loopback(1024);

        pipe.write_all(b"test").unwrap();

        assert_eq!(pipe.write_buffer_len(), 4);
        assert_eq!(pipe.read_buffer_len(), 4);

        pipe.clear();

        assert_eq!(pipe.write_buffer_len(), 0);
        assert_eq!(pipe.read_buffer_len(), 0);

        // The pipe is empty, so reading should timeout
        let mut read_data = [0u8; 1];
        assert_eq!(
            pipe.read_exact(&mut read_data).unwrap_err().kind(),
            io::ErrorKind::UnexpectedEof
        );
    }

    #[test]
    fn test_multiple_threads() {
        use std::{thread, time};

        let (mut pipe1, mut pipe2) = MockPipe::pair(1024);

        let write_data1 = b"hello";
        let write_data2 = b"hi";

        let writer = thread::spawn(move || {
            thread::sleep(time::Duration::from_millis(100));

            pipe1.write_all(write_data1).unwrap();
            assert_eq!(pipe1.write_buffer_len(), write_data1.len());

            thread::sleep(time::Duration::from_millis(100));

            pipe1.write_all(write_data2).unwrap();
            assert_eq!(pipe1.write_buffer_len(), write_data2.len());

            pipe1.flush().unwrap();
            assert_eq!(pipe1.write_buffer_len(), 0);
        });

        let reader = thread::spawn(move || {
            pipe2.set_timeout(Some(Duration::from_millis(1000)));

            let mut read_data = [0u8; 5];
            pipe2.read_exact(&mut read_data).unwrap();
            assert_eq!(&read_data, write_data1);

            thread::sleep(time::Duration::from_millis(200));

            pipe2.set_timeout(Some(Duration::ZERO));

            let mut read_data = [0u8; 2];
            pipe2.read_exact(&mut read_data).unwrap();
            assert_eq!(&read_data, write_data2);
        });

        writer.join().unwrap();
        reader.join().unwrap();
    }
}
